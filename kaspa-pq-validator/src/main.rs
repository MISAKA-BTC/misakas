//! kaspa-pq-validator — the ADR-0011 single-host validator sidecar.
//!
//! A standalone process that connects to a co-located `kaspad` over a 127.0.0.1 wRPC
//! (borsh) endpoint and, once its stake bond is active, attests to the selected-chain
//! anchor each epoch: it fetches the ready-to-sign target over wRPC, signs it with its
//! ML-DSA-87 validator key (under the equivocation-safety guard), funds a
//! `StakeAttestationShard` transaction from a UTXO at its own address, and submits it.
//! The signing primitives are shared with the in-process `--enable-validator` service via
//! `kaspa-pq-validator-core`.
//!
//! Subcommands: `run` (the validator daemon), `keygen` (generate a validator key), and
//! `status` (one-shot bond/status query). Recommended deployment: `run` beside `kaspad`
//! under systemd (ADR-0011); the node must run `--utxoindex` for the funding lookup.

mod palw_da_auto_respond;
mod palw_payload;
mod palw_provider_unbond;
mod palw_search_retrieval;
mod palw_submit;

use clap::{Parser, Subcommand};
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::dns_finality::{
    DNS_PAYLOAD_VERSION_V1, SignedEpochCheckOutcome, SignedEpochRecord, StakeAttestation, signature_fingerprint,
    single_attestation_shard, stake_attestation_message,
};
use kaspa_consensus_core::mass::{MassCalculator, UtxoCell, calc_storage_mass};
use kaspa_consensus_core::network::{EndpointKind, NetworkId, NetworkType};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};
use kaspa_core::{info, warn};
use kaspa_pq_validator_core::{
    SignedEpochStore, VALIDATOR_SEED_LEN, ValidatorKey, is_spendable, load_validator_seed, parse_stake_bond_ref, select_funding,
};
use kaspa_rpc_core::{
    GetConsensusIdentityRequest, GetPalwStateRequest, GetStakeBondRequest, GetValidatorAttestationTargetResponse,
    GetValidatorAttestationTargetsRequest, RpcError, RpcHash, RpcTransaction, api::rpc::RpcApi,
};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use rand::RngCore;
use std::collections::{HashMap, HashSet};
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Duration;

use palw_da_auto_respond::PalwDaAutoRespondArgs;
use palw_payload::PalwPayloadArgs;
use palw_provider_unbond::PalwProviderUnbondArgs;
use palw_submit::PalwSubmitArgs;

const VALIDATOR: &str = "kaspa-pq-validator";

/// Kaspa-PQ validator sidecar (ADR-0011).
#[derive(Parser, Debug)]
#[command(name = "kaspa-pq-validator", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the validator daemon: connect to the local node and attest while the bond is active.
    Run(RunArgs),
    /// Generate a new ML-DSA-87 validator key and print its identity + funding address.
    Keygen(KeygenArgs),
    /// One-shot: query the node + bond status and print it.
    Status(StatusArgs),
    /// Stake mined coins: build + submit a StakeBond tx from a UTXO at the funding address.
    Bond(BondArgs),
    /// Begin unbonding a StakeBond: build + submit a signed StakeUnbondRequest for the given
    /// bond outpoint (its locked stake becomes spendable after the unbonding window elapses).
    Unbond(UnbondArgs),
    /// Load generator: continuously spend mature UTXOs at the funding address into fan-out
    /// NATIVE transfers, flooding the node's mempool with valid ML-DSA transactions.
    Spam(SpamArgs),
    /// kaspa-pq EVM Lane (§7.2): create an EVM_DEPOSIT_LOCK output funding an EVM address —
    /// the UTXO side of a bridge deposit. Claim it on a mining node afterwards via
    /// submitEvmDepositClaim(txid, 0).
    DepositLock(DepositLockArgs),
    /// kaspa-pq EVM Lane (§9.2): submit a deposit-claim for a previously-created
    /// EVM_DEPOSIT_LOCK outpoint (`txid:index`). Run against a MINING node so the claim
    /// is included in an accepting chain block, which executes it and credits the EVM address.
    Claim(ClaimArgs),
    /// One-shot headless balance: query the node's `getBalancesByAddresses` for one or more
    /// `misaka:`/`misakatest:` addresses over wRPC and print each balance, then exit (no
    /// interactive wallet needed). The node must run --utxoindex.
    Balance(BalanceArgs),
    /// Submit one staged PALW provider-bond/unbond/manifest/leaf/certificate wire payload and wait
    /// for its selected-chain change outpoint before the next dependency layer (inclusion, not
    /// finality).
    PalwSubmit(PalwSubmitArgs),
    /// Build an offline, consensus-validated PALW Borsh payload for staged submission.
    PalwPayload(PalwPayloadArgs),
    /// Owner-authorized PALW provider exit: submit the request or sweep released collateral.
    PalwProviderUnbond(PalwProviderUnbondArgs),
    /// Inspect bounded, sink-pinned PALW batch/provider state after selected-chain carrier inclusion.
    PalwStatus(PalwStatusArgs),
    /// Off-node automatic DA-challenge (0x3b) responder: discover open challenges on the operator's
    /// provider bonds and answer them with an owner-signed response before their deadline.
    PalwDaAutoRespond(PalwDaAutoRespondArgs),
    /// Node-side retrieval service (measurement gate, ADR node-anchored-web-search-da): run ONE
    /// egress-guarded provider search against the operator-local SearXNG, convert the typed
    /// outcome into a canonical SearchSnapshotV1 bound to this node's network/genesis/DAA, and
    /// write the exact DA object bytes + manifest for ConsensusApi::palw_admit_search_snapshot.
    PalwSearchRetrieval(PalwSearchRetrievalArgs),
    /// Scheduler side of a search JobSpec: pin query/policy/bounds/validity BEFORE retrieval and
    /// sign the canonical PalwSearchAssignmentV1 with an ML-DSA-87 scheduler key. The printed
    /// assignment id is what the retrieval snapshot must reference.
    PalwSearchAssign(PalwSearchAssignArgs),
    /// One-shot: fetch a block by hash (with transactions) and print its coinbase facts —
    /// the subsidy S (from the coinbase payload) and every coinbase output's value + SPK — as
    /// parseable `key: value` lines. Used by the Phase-0 harness to record the minted block's
    /// subsidy. NOTE: an algo-4 block's OWN coinbase pays its mergeset (the algo-3 base blocks it
    /// merges), NOT its providers; provider payouts appear in a later block that merges this one.
    GetBlock(GetBlockArgs),
    /// Review §8 — locate the SETTLEMENT of an algo-4 (ReplicaPalw) block's provider rewards: walk
    /// the DAG above --source-block to the first CHAIN block that merges it, classify the merge
    /// (blue = paid / red = pays 0, PALW-014 weight-0 fork), derive the EXACT expected provider A/B
    /// values with the real consensus split functions, and (when --provider-a-spk/--provider-b-spk
    /// are given) assert the merging block's coinbase pays exactly those values to exactly those
    /// scripts. Exit: 0 verified-pass, 1 mismatch/fail, 2 not-yet-merged, 3 partial (no SPKs given).
    FindRewardSettlement(FindRewardSettlementArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// Local node wRPC (borsh) endpoint, host:port. Bind the node's RPC to 127.0.0.1 only.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Stake-bond outpoint backing this validator, "txid_hex:index". Required (together
    /// with --validator-key and --signed-epoch-db) to attest; otherwise observe-only.
    #[arg(long, env = "KASPA_PQ_STAKE_BOND")]
    stake_bond: Option<String>,

    /// Path to the ML-DSA-87 validator signing key (32-byte seed, hex). Required to attest.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: Option<String>,

    /// Path to the persistent equivocation-safety log (JSON). Required to attest — the
    /// guard cannot be enforced across restarts without it. Back this file up.
    #[arg(long, env = "KASPA_PQ_SIGNED_EPOCH_DB")]
    signed_epoch_db: Option<String>,

    /// Compute eligibility + the attestation target and sign it locally, but never submit.
    #[arg(long, env = "KASPA_PQ_DRY_RUN")]
    dry_run: bool,

    /// Expected node network id; refuse to start on mismatch (ADR-0011 §"Same network").
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Seconds between attestation rounds. Each round attests at most the ONE current
    /// canonical-ready epoch, so this poll period MUST be ≤ an epoch's wall-clock duration for a
    /// single validator to cover EVERY epoch and reach the DNS stake-depth threshold. ALL kaspa-pq
    /// networks (mainnet/testnet/devnet/simnet) run at 10 BPS (`BlockrateParams::new::<10>()`,
    /// target_time_per_block = 100 ms) with `attestation_epoch_length_blue_score = 100`, so an
    /// epoch is ≈ 10 s — hence the default 3 s (≈3 polls/epoch, keeps a single validator caught up
    /// on every network). Revisiting the same epoch within a run is deduped (no re-sign / no
    /// rebroadcast), so a small value only adds cheap local-node RPC polls; raise it only if you
    /// deliberately throttle the chain to a slower block rate.
    #[arg(long, default_value_t = 3, env = "KASPA_PQ_ATTEST_POLL_SECS")]
    attest_poll_secs: u64,

    /// Fee in sompi for each attestation-shard transaction. Default: a mass-based estimate from the
    /// network's mass params (the shard carries a 4627-byte ML-DSA-87 signature, so the flat floor
    /// is far below the mempool minimum — ≈ 232 600 sompi on devnet). Pass an explicit value to
    /// override (e.g. bump under congestion); like `bond`/`unbond`, omit it to auto-size.
    #[arg(long, env = "KASPA_PQ_ATTEST_FEE")]
    fee: Option<u64>,

    /// Logging level {off, error, warn, info, debug, trace}.
    #[arg(long, default_value = "info", env = "KASPA_PQ_LOG_LEVEL")]
    log_level: String,
}

#[derive(Parser, Debug)]
struct KeygenArgs {
    /// Output path for the validator key (32-byte seed as hex; written with mode 0600 on unix).
    #[arg(long)]
    out: String,

    /// Network for the printed funding address {mainnet, testnet, devnet, simnet}.
    /// Reads KASPA_PQ_NETWORK so `misaka validator keygen` inherits the unified CLI's
    /// --network-id (an explicit --network/--network-id still wins).
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK", default_value = "mainnet")]
    network: String,
}

#[derive(Parser, Debug)]
struct StatusArgs {
    /// Local node wRPC (borsh) endpoint, host:port.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Network id; used to auto-resolve the node endpoint when --node-wrpc-borsh is omitted.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Stake-bond outpoint to report, "txid_hex:index".
    #[arg(long, env = "KASPA_PQ_STAKE_BOND")]
    stake_bond: Option<String>,
}

#[derive(Parser, Debug)]
struct PalwStatusArgs {
    /// Local node wRPC (borsh) endpoint, host:port.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Expected network id; also resolves the default loopback endpoint.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// One PALW batch id (64-byte Hash64 hex) to resolve in the node's fork-local sink view.
    #[arg(long)]
    batch_id: Option<String>,

    /// One PALW provider-bond outpoint (`txid_hex:index`) to resolve in the selected-chain registry.
    #[arg(long)]
    provider_bond: Option<String>,
}

#[derive(Parser, Debug)]
struct BondArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Path to the ML-DSA-87 validator signing key (32-byte seed, hex). The bond is staked
    /// from a UTXO at this key's own funding address and binds this key as the validator.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// Amount to stake. Becomes the bond's locked output-0. Covered by aggregating up
    /// to 20 of the LARGEST mature funding UTXOs at this key's address (amount + fee); no manual
    /// consolidation needed unless the 20 largest still fall short.
    /// Accepts `10MSK` / `10.5MSK` / `10KAS` (1 MSK = 100_000_000 sompi, up to 8 decimals),
    /// or a bare integer / `<n>sompi` for raw sompi.
    #[arg(long, value_parser = parse_amount_sompi)]
    amount: u64,

    /// First DAA score at which the bond's attestations count. 0 = active as soon as accepted.
    #[arg(long, default_value_t = 0)]
    activation_daa_score: u64,

    /// Per-bond unbonding window in blocks. Must be >= the network's
    /// `unbonding_period_blocks` floor (devnet harness = 700).
    #[arg(long, default_value_t = 700)]
    unbonding_period_blocks: u64,

    /// Fee in sompi for the bond transaction. Default: a mass-based estimate from the network's
    /// mass params (the StakeBond payload carries the 2592-byte pubkey, so the flat attestation
    /// floor is too low to relay). Pass an explicit value to override (e.g. bump under congestion).
    #[arg(long)]
    fee: Option<u64>,

    /// Expected node network id; refuse on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct UnbondArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Path to the ML-DSA-87 validator signing key (32-byte seed, hex). Must be the key that
    /// owns the bond (its derived `validator_id` == the bond's `owner_pubkey_hash`), otherwise
    /// the node rejects the unauthorized request.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// The bond to unbond, "txid:index" — the `bond_outpoint` that `bond` printed (i.e. `<txid>:0`).
    #[arg(long)]
    stake_bond: String,

    /// Fee in sompi for the unbond transaction. Default: a mass-based estimate from the network's
    /// mass params (the unbond payload carries the 2592-byte pubkey + 4627-byte sig, so the flat
    /// attestation floor is too low to relay). Pass an explicit value to override.
    #[arg(long)]
    fee: Option<u64>,

    /// Expected node network id; refuse on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct SpamArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
    /// ML-DSA validator key (32-byte seed, hex) whose funding address holds the coins to spam.
    /// Mine to its `funding_address` first (e.g. `misaminer --wallet <addr>`).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,
    /// Outputs per split tx (fan-out). Each becomes a fresh spendable UTXO, so a chain of these
    /// grows the UTXO set and the tx rate. 2-4 is a good sustained load.
    #[arg(long, default_value_t = 3)]
    fanout: usize,
    /// Flat fee (sompi) per tx; must cover the tx's mass at the relay rate.
    #[arg(long, default_value_t = 50_000)]
    fee: u64,
    /// Max txs to submit per round (per UTXO-set scan).
    #[arg(long, default_value_t = 300)]
    max_per_round: usize,
    /// Milliseconds to sleep between rounds.
    #[arg(long, default_value_t = 200)]
    interval_ms: u64,
    /// Skip UTXOs smaller than this (sompi) — keeps splits above the dust floor.
    #[arg(long, default_value_t = 1_000_000)]
    min_utxo: u64,
    /// Expected node network id; refuse on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct ClaimArgs {
    /// Local node wRPC (borsh) endpoint, host:port. Run against a MINING node.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
    /// Network id; used to auto-resolve the node endpoint when --node-wrpc-borsh is omitted.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
    /// The EVM_DEPOSIT_LOCK outpoint to claim, `txid_hex:index` (the deposit-lock command printed it).
    #[arg(long)]
    outpoint: String,
}

#[derive(Parser, Debug)]
struct BalanceArgs {
    /// Node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
    /// Address to query, e.g. `misakatest:q...`. Repeat --address for several (one RPC call).
    #[arg(long, required = true)]
    address: Vec<String>,
    /// Expected node network id (e.g. `testnet-200`); refuse on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct DepositLockArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
    /// ML-DSA key (32-byte seed, hex) whose funding address pays the deposit. Its own
    /// funding P2PKH becomes the lock's refund script (reclaimable after the timeout).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,
    /// The EVM address to credit, 20-byte hex (optional 0x prefix).
    #[arg(long)]
    evm_address: String,
    /// Deposit amount in sompi (locked into the EVM_DEPOSIT_LOCK output-0).
    #[arg(long)]
    amount: u64,
    /// Claim-inclusion tip (sompi, ≤ amount) paid to the accepting block's EVM coinbase —
    /// the §9.2 incentive for a producer to include the claim.
    #[arg(long, default_value_t = 0)]
    claim_tip: u64,
    /// Refund timeout as a DAA-score DELTA from the current sink (the lock is claimable
    /// strictly before sink_daa + delta; refundable to the funding key after).
    #[arg(long, default_value_t = 1_000_000)]
    timeout_daa_delta: u64,
    /// Fee in sompi. Default: a mass-based estimate (each ML-DSA input is ~7 KB).
    #[arg(long)]
    fee: Option<u64>,
    /// Expected node network id; refuse on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Run(args) => {
            kaspa_core::log::init_logger(None, &args.log_level);
            run_daemon(args).await
        }
        Command::Keygen(args) => keygen(args),
        Command::Status(args) => status(args).await,
        Command::GetBlock(args) => get_block(args).await,
        Command::FindRewardSettlement(args) => find_reward_settlement(args).await,
        Command::Bond(args) => {
            kaspa_core::log::init_logger(None, "info");
            bond(args).await
        }
        Command::Unbond(args) => {
            kaspa_core::log::init_logger(None, "info");
            unbond(args).await
        }
        Command::Spam(args) => {
            kaspa_core::log::init_logger(None, "info");
            spam(args).await
        }
        Command::DepositLock(args) => {
            kaspa_core::log::init_logger(None, "info");
            deposit_lock(args).await
        }
        Command::Claim(args) => {
            kaspa_core::log::init_logger(None, "info");
            claim(args).await
        }
        Command::Balance(args) => {
            kaspa_core::log::init_logger(None, "info");
            balance(args).await
        }
        Command::PalwSubmit(args) => {
            kaspa_core::log::init_logger(None, "info");
            palw_submit::palw_submit(args).await
        }
        Command::PalwPayload(args) => palw_payload::palw_payload(args).await,
        Command::PalwProviderUnbond(args) => {
            kaspa_core::log::init_logger(None, "info");
            palw_provider_unbond::palw_provider_unbond(args).await
        }
        Command::PalwStatus(args) => palw_status(args).await,
        Command::PalwDaAutoRespond(args) => {
            kaspa_core::log::init_logger(None, "info");
            palw_da_auto_respond::palw_da_auto_respond(args).await
        }
        Command::PalwSearchRetrieval(args) => {
            kaspa_core::log::init_logger(None, "info");
            palw_search_retrieval_cmd(args).await
        }
        Command::PalwSearchAssign(args) => {
            kaspa_core::log::init_logger(None, "info");
            palw_search_assign_cmd(args).await
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[{VALIDATOR}] error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Generate a fresh ML-DSA-87 validator key, write the seed to `--out`, and print the
/// derived overlay identity + funding address. The owner / withdrawal key is NOT produced
/// here (ADR-0011 key-separation policy: validator key on the host, owner key off it).
fn keygen(args: KeygenArgs) -> Result<(), String> {
    let prefix = parse_prefix(&args.network)?;
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    rand::thread_rng().fill_bytes(&mut seed);
    let key = ValidatorKey::from_seed(seed);

    let mut hex_buf = [0u8; VALIDATOR_SEED_LEN * 2];
    faster_hex::hex_encode(&seed, &mut hex_buf).map_err(|e| format!("hex encode failed: {e}"))?;
    let hex = std::str::from_utf8(&hex_buf).expect("hex is valid utf-8");

    // Create the key file atomically and refuse to clobber an existing one. `create_new`
    // (O_CREAT|O_EXCL) both prevents silently destroying a funded validator's key on a mistyped path
    // and rejects following a pre-planted symlink; `.mode(0600)` sets owner-only perms at creation, so
    // there is never the group/world-readable window a write-then-chmod sequence leaves open.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&args.out)
            .map_err(|e| format!("cannot create key file '{}' (it must not already exist): {e}", args.out))?;
        f.write_all(hex.as_bytes()).map_err(|e| format!("cannot write key to '{}': {e}", args.out))?;
        f.sync_all().map_err(|e| format!("cannot fsync key file '{}': {e}", args.out))?;
    }
    #[cfg(not(unix))]
    {
        if std::path::Path::new(&args.out).exists() {
            return Err(format!("refusing to overwrite existing key file '{}'", args.out));
        }
        std::fs::write(&args.out, hex).map_err(|e| format!("cannot write key to '{}': {e}", args.out))?;
    }

    // Best-effort scrub of the in-memory seed/hex material (black_box discourages dead-store removal).
    seed.fill(0);
    hex_buf.fill(0);
    std::hint::black_box(&seed);
    std::hint::black_box(&hex_buf);

    println!("validator key written to {} (keep it secret; back it up; do NOT run it on a second host)", args.out);
    println!("validator_id:    {}", key.validator_id);
    println!("funding_address: {}", key.funding_address(prefix));
    Ok(())
}

/// One-shot status report: connect, print the node's network/sync state, and (if a bond is
/// given) the bond's effective status. Useful for `systemctl`-free health checks.
async fn status(args: StatusArgs) -> Result<(), String> {
    kaspa_core::log::init_logger(None, "warn");
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    println!("node_network: {}", server.network_id);
    println!("node_synced:  {}", server.is_synced);
    println!("node_version: {}", server.server_version);
    // Review §11.2: the node's OWN consensus identity, served from its live Config (SERVER-side
    // truth — includes runtime overrides like --palw-enable-algo4 and the params identity hash).
    // Falls back to an explicitly labeled client-side derivation for older nodes.
    match client.get_consensus_identity(GetConsensusIdentityRequest {}).await {
        Ok(identity) => {
            println!("node_genesis_hash: {} (server-reported)", identity.genesis_hash);
            println!("node_params_hash: {}", identity.consensus_params_hash);
            println!("node_header_version_effective: {}", identity.header_version_effective);
            println!("node_palw_algo4_accept: {}", identity.palw_algo4_accept_effective);
            println!("node_archival: {}", identity.is_archival);
            println!("node_utxoindex: {}", identity.is_utxo_indexed);
            println!(
                "node_git_commit: {}",
                if identity.git_commit.is_empty() { "unknown (built outside git)" } else { &identity.git_commit }
            );
            // Cross-check: the CLI's own preset derivation for the same network id. A mismatch means
            // the CLI and node builds carry DIFFERENT presets for one network name — preset drift.
            let local_genesis = Params::from(server.network_id).genesis.hash.to_string();
            if local_genesis != identity.genesis_hash {
                println!(
                    "WARN preset_drift: this CLI derives genesis {local_genesis} for '{}' but the node enforces {} — CLI and node builds disagree about the network preset",
                    server.network_id, identity.genesis_hash
                );
            }
        }
        Err(_) => {
            // Older node without getConsensusIdentity: the only available value is the CLI's own
            // preset derivation — label it as such (it proves the CLI's preset, not the node's).
            println!(
                "node_genesis_hash: {} (CLI-derived from network id; node predates getConsensusIdentity)",
                Params::from(server.network_id).genesis.hash
            );
        }
    }
    if let Some(bond) = &args.stake_bond {
        match client.get_stake_bond(GetStakeBondRequest { bond_outpoint: bond.clone() }).await {
            Ok(b) if b.available => {
                println!("bond:         {bond}");
                println!("bond_status:  {}", b.effective_status);
                println!("bond_amount:  {}", b.amount);
                println!("validator_id: {}", b.validator_id);
            }
            Ok(_) => println!("bond:         {bond} (not found in the registry)"),
            Err(e) => println!("bond:         query failed: {e} (does the node configure the overlay?)"),
        }
    }
    match client.get_dns_confirmation().await {
        Ok(d) if d.available => {
            let health = match d.health {
                0 => "DisabledBeforeActivation",
                1 => "Active",
                2 => "DegradedStakeQualityLow",
                3 => "DegradedCertificateCensored",
                _ => "Unknown",
            };
            println!("dns_confirmed: {}", d.dns_confirmed);
            println!("pow_confirmed: {}", d.pow_confirmed);
            println!("work_depth:    {}/{}", d.work_depth, d.required_work_depth);
            println!("stake_depth:   {}/{}", d.stake_depth, d.required_stake_depth);
            println!("dns_health:    {health}");
            println!("dns_anchor:    {} (daa {})", d.last_dns_confirmed_anchor, d.last_dns_confirmed_anchor_daa_score);
        }
        Ok(_) => println!("dns:          overlay not active on this node"),
        Err(e) => println!("dns:          query failed: {e}"),
    }
    let _ = client.disconnect().await;
    Ok(())
}

#[derive(Parser, Debug)]
struct GetBlockArgs {
    /// Block hash (64-hex) to fetch.
    #[arg(long)]
    hash: String,

    /// Local node wRPC (borsh) endpoint, host:port. Bind the node's RPC to 127.0.0.1 only.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Network id; used to auto-resolve the node endpoint when --node-wrpc-borsh is omitted.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

/// One-shot: fetch a block (with transactions) by hash and print its coinbase facts as parseable
/// `key: value` lines: `coinbase_subsidy_sompi` (block subsidy S, decoded from the coinbase
/// payload — layout `blue_score:u64 LE ‖ subsidy:u64 LE ‖ …`), `coinbase_blue_score`, and every
/// `coinbase_output_<i>: value=<sompi> spk=<hex>` (SPK = version big-endian 4-hex ‖ script hex, the
/// same human-readable encoding consensus uses). The harness (start-palw-miner.sh) parses these to
/// record PALW_ALGO4_SUBSIDY_SOMPI after a mint. IMPORTANT: an algo-4 block's OWN coinbase pays its
/// mergeset (the algo-3 base blocks it merges), NOT its providers — the provider A/B payouts appear
/// only in the coinbase of a LATER block that merges this block as a blue ReplicaPalw source.
async fn get_block(args: GetBlockArgs) -> Result<(), String> {
    kaspa_core::log::init_logger(None, "warn");
    let hash = RpcHash::from_str(args.hash.trim()).map_err(|e| format!("--hash '{}' is not a valid block hash: {e}", args.hash))?;
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let block = client.get_block(hash, true).await.map_err(|e| format!("getBlock({hash}) failed: {e}"))?;
    let _ = client.disconnect().await;

    println!("block_hash: {hash}");
    println!("block_daa_score: {}", block.header.daa_score);
    // Selected-parent-chain membership at the answering node's CURRENT virtual.
    // This is what a reorg changes: a displaced branch's blocks remain fetchable
    // (stored forever) but flip to is_chain_block=false. The G7 reorg-parity case
    // asserts this on BOTH nodes for the orphaned and the winning tip.
    match block.verbose_data.as_ref() {
        Some(vd) => println!("block_is_chain_block: {}", vd.is_chain_block),
        None => println!("block_is_chain_block: unavailable (node returned no verbose data)"),
    }
    let cb = block.transactions.first().ok_or_else(|| format!("block {hash} has no coinbase transaction"))?;
    if cb.payload.len() >= 16 {
        // Coinbase payload: blue_score:u64 LE ‖ subsidy:u64 LE ‖ spk_version:u16 LE ‖ spk_len:u8 ‖ spk.
        let blue_score = u64::from_le_bytes(cb.payload[0..8].try_into().expect("8 bytes"));
        let subsidy = u64::from_le_bytes(cb.payload[8..16].try_into().expect("8 bytes"));
        println!("coinbase_blue_score: {blue_score}");
        println!("coinbase_subsidy_sompi: {subsidy}");
    } else {
        println!("coinbase_subsidy_sompi: unavailable (coinbase payload is {} bytes, < 16)", cb.payload.len());
    }
    println!("coinbase_output_count: {}", cb.outputs.len());
    for (i, o) in cb.outputs.iter().enumerate() {
        let spk_hex = spk_to_hex(&o.script_public_key)?;
        println!("coinbase_output_{i}: value={} spk={}", o.value, spk_hex);
    }
    Ok(())
}

/// SPK -> the human-readable hex encoding the harness compares (version big-endian 4-hex ‖ script hex).
pub(crate) fn spk_to_hex(spk: &kaspa_consensus_core::tx::ScriptPublicKey) -> Result<String, String> {
    let script = spk.script();
    let mut out = vec![0u8; script.len() * 2 + 4];
    faster_hex::hex_encode(&spk.version().to_be_bytes(), &mut out[..4]).map_err(|e| format!("hex encode failed: {e}"))?;
    faster_hex::hex_encode(script, &mut out[4..]).map_err(|e| format!("hex encode failed: {e}"))?;
    String::from_utf8(out).map_err(|e| format!("spk hex is not utf-8: {e}"))
}

#[derive(Parser, Debug)]
struct FindRewardSettlementArgs {
    /// The minted algo-4 (ReplicaPalw) block hash (128-hex) whose provider settlement to locate.
    #[arg(long)]
    source_block: String,

    /// Expected provider-A reward SPK (version 4-hex ‖ script hex — the encoding get-block prints).
    /// When both SPKs are given the verdict is exact-verified; without them it is PARTIAL.
    #[arg(long)]
    provider_a_spk: Option<String>,

    /// Expected provider-B reward SPK (same encoding).
    #[arg(long)]
    provider_b_spk: Option<String>,

    /// Replica premium π in bps. Consensus currently pins the premium NEUTRAL (10000) via
    /// `palw_premium_at_window`; override only if a future fork activates a dynamic premium.
    #[arg(long, default_value_t = 10_000)]
    premium_pi_bps: u32,

    /// Maximum DAG blocks to walk above the source while searching for the merging chain block.
    #[arg(long, default_value_t = 512)]
    max_walk: usize,

    /// Local node wRPC (borsh) endpoint, host:port. Bind the node's RPC to 127.0.0.1 only.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Network id; used to auto-resolve the node endpoint when --node-wrpc-borsh is omitted.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

/// Review §8 — descendant-reward-settlement verifier. An algo-4 block's provider payouts appear only
/// in the coinbase of the first CHAIN block that merges it (blue ⇒ paid per the lane split; red ⇒
/// 0, ADR-0039 §17.4). This walks the children DAG to that merging chain block, cross-checks the
/// node's own `getCurrentBlockColor`, derives the EXACT expected values with the REAL consensus
/// split functions (`split_block_subsidy` over the lane params + `premium_split`), and compares the
/// merging coinbase's outputs value+SPK-exactly. Honest caveats printed with the verdict: a blue
/// source can still legitimately pay 0 under halted-epoch / duplicate-work / unbacked-collateral
/// classifications, which this RPC surface cannot distinguish — reported as such, never as PASS.
async fn find_reward_settlement(args: FindRewardSettlementArgs) -> Result<(), String> {
    use kaspa_consensus_core::{dns_finality::split_block_subsidy, palw_premium::premium_split, pow_layer0::POW_ALGO_ID_PALW_REPLICA};
    kaspa_core::log::init_logger(None, "warn");
    let source = RpcHash::from_str(args.source_block.trim()).map_err(|e| format!("--source-block is not a valid block hash: {e}"))?;
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;

    // 1. The source block: must be an algo-4 (ReplicaPalw) header; capture its subsidy S.
    let src = client.get_block(source, true).await.map_err(|e| format!("getBlock(source) failed: {e}"))?;
    println!("settlement.source_block: {source}");
    if src.header.pow_algo_id != POW_ALGO_ID_PALW_REPLICA {
        let _ = client.disconnect().await;
        return Err(format!(
            "source block's pow_algo_id is {} (expected {POW_ALGO_ID_PALW_REPLICA}/ReplicaPalw) — not an algo-4 block",
            src.header.pow_algo_id
        ));
    }
    println!("settlement.source_batch_id: {}", src.header.palw_batch_id);
    println!("settlement.source_leaf_index: {}", src.header.palw_leaf_index);
    let cb = src.transactions.first().ok_or("source block carries no coinbase")?;
    if cb.payload.len() < 16 {
        let _ = client.disconnect().await;
        return Err(format!("source coinbase payload is {} bytes (< 16) — cannot decode subsidy", cb.payload.len()));
    }
    let subsidy = u64::from_le_bytes(cb.payload[8..16].try_into().expect("8 bytes"));
    println!("settlement.source_subsidy_sompi: {subsidy}");

    // 2. Walk the children DAG (BFS) to the FIRST chain block whose mergeset contains the source.
    let src_verbose = src.verbose_data.as_ref().ok_or("node returned no verbose data for the source block")?;
    let mut frontier: std::collections::VecDeque<RpcHash> = src_verbose.children_hashes.iter().copied().collect();
    let mut seen: std::collections::HashSet<RpcHash> = frontier.iter().copied().collect();
    let mut merging: Option<(RpcHash, kaspa_rpc_core::RpcBlock, &'static str)> = None;
    let mut walked = 0usize;
    while let Some(next) = frontier.pop_front() {
        if walked >= args.max_walk {
            break;
        }
        walked += 1;
        let block = client.get_block(next, true).await.map_err(|e| format!("getBlock({next}) failed during walk: {e}"))?;
        let Some(verbose) = block.verbose_data.clone() else { continue };
        if verbose.is_chain_block {
            if verbose.merge_set_blues_hashes.contains(&source) {
                merging = Some((next, block, "blue"));
                break;
            }
            if verbose.merge_set_reds_hashes.contains(&source) {
                merging = Some((next, block, "red"));
                break;
            }
            // A chain block above the source that does NOT merge it: the source is on a pruned-off
            // side branch relative to this walk — keep exploring siblings via the queue.
        }
        for child in verbose.children_hashes {
            if seen.insert(child) {
                frontier.push_back(child);
            }
        }
    }

    // Cross-check with the node's own merging-block BFS (authoritative; errors while unmerged).
    match client.get_current_block_color(source).await {
        Ok(color) => println!("settlement.node_color_crosscheck: blue={}", color.blue),
        Err(e) => println!("settlement.node_color_crosscheck: unavailable ({e})"),
    }

    let Some((merger_hash, merger, classification)) = merging else {
        println!("settlement.classification: not-yet-merged (walked {walked} blocks, cap {})", args.max_walk);
        println!("settlement.verdict: NOT_SETTLED — no chain block merging the source found yet; mine on and re-run");
        let _ = client.disconnect().await;
        std::process::exit(2);
    };
    println!("settlement.block: {merger_hash}");
    println!("settlement.classification: {classification}");

    // 3. EXACT expected values via the real consensus split fns, with the fee split the network's
    //    dns_params selects at the MERGING block's daa score (bootstrap vs full split).
    let params = Params::from(server.network_id);
    let merger_daa = merger.header.daa_score;
    let fee_split = params
        .dns_params
        .as_ref()
        .and_then(|dns| dns.reward_fee_split(merger_daa))
        .ok_or("this network has no dns_params/fee split — PALW reward split is undefined here")?;
    let lane = fee_split.palw_lane();
    let split = split_block_subsidy(subsidy, &lane);
    let (a_expected, b_expected_base, b_remainder) = premium_split(split.worker_base_sompi, 1, args.premium_pi_bps);
    let b_expected = b_expected_base + b_remainder;
    println!("settlement.expected_inclusion_sompi: {}", split.worker_inclusion_sompi);
    println!("settlement.expected_validator_sompi: {}", split.validator_sompi);
    println!("settlement.expected_base_sompi: {}", split.worker_base_sompi);
    println!("settlement.expected_provider_a_sompi: {a_expected}");
    println!("settlement.expected_provider_b_sompi: {b_expected}");

    // 4. Compare against the merging block's coinbase outputs.
    let merger_cb = merger.transactions.first().ok_or("merging block carries no coinbase")?;
    println!("settlement.merging_coinbase_outputs: {}", merger_cb.outputs.len());
    let mut outputs: Vec<(u64, String)> = Vec::with_capacity(merger_cb.outputs.len());
    for o in &merger_cb.outputs {
        outputs.push((o.value, spk_to_hex(&o.script_public_key)?));
    }
    for (i, (value, spk)) in outputs.iter().enumerate() {
        println!("settlement.output_{i}: value={value} spk={spk}");
    }

    let _ = client.disconnect().await;

    if classification == "red" {
        // A red merge pays 0 by design (ADR-0039 §17.4, weight-0 fork): with the expected SPKs we
        // can assert the ABSENCE of provider payouts; that is a verified red settlement.
        match (args.provider_a_spk.as_deref(), args.provider_b_spk.as_deref()) {
            (Some(spk_a), Some(spk_b)) => {
                let paid_a = outputs.iter().any(|(_, spk)| spk.eq_ignore_ascii_case(spk_a));
                let paid_b = outputs.iter().any(|(_, spk)| spk.eq_ignore_ascii_case(spk_b));
                if paid_a || paid_b {
                    println!(
                        "settlement.verdict: FAIL — RED merge but a provider SPK was still paid (paid_a={paid_a} paid_b={paid_b})"
                    );
                    std::process::exit(1);
                }
                println!("settlement.verdict: PASS_RED — red merge, provider payouts correctly absent (0 by design)");
                return Ok(());
            }
            _ => {
                println!(
                    "settlement.verdict: PARTIAL — red merge (pays 0 by design); pass --provider-a-spk/--provider-b-spk to assert the payout absence exactly"
                );
                std::process::exit(3);
            }
        }
    }

    match (args.provider_a_spk.as_deref(), args.provider_b_spk.as_deref()) {
        (Some(spk_a), Some(spk_b)) => {
            // Multiset match by (value, spk): other mergeset sources contribute their own outputs.
            let hit_a = outputs.iter().any(|(value, spk)| *value == a_expected && spk.eq_ignore_ascii_case(spk_a));
            let hit_b = outputs.iter().any(|(value, spk)| *value == b_expected && spk.eq_ignore_ascii_case(spk_b));
            if hit_a && hit_b {
                println!(
                    "settlement.verdict: PASS — blue merge pays provider A {a_expected} and provider B {b_expected} sompi to the exact expected SPKs"
                );
                Ok(())
            } else {
                println!(
                    "settlement.verdict: FAIL — blue merge but expected payouts not found (provider_a_matched={hit_a} provider_b_matched={hit_b}). \
Honest caveat: a blue source still pays 0 under halted-epoch / duplicate-work / unbacked-collateral / DA-challenged classifications, \
which this RPC surface cannot distinguish — inspect the node logs for the source block's reward class before treating this as a split bug"
                );
                std::process::exit(1);
            }
        }
        _ => {
            println!(
                "settlement.verdict: PARTIAL — blue merge located and expected values derived; pass --provider-a-spk/--provider-b-spk for the exact-SPK assertion"
            );
            std::process::exit(3);
        }
    }
}

/// Inspect the selected-chain provider registry and the batch surfaces used by ticket resolution. The
/// response is pinned to a named sink so two operators can compare like-for-like; batch fields remain
/// raw-carried-view/global-blob diagnostics, not selected-chain acceptance proof.
async fn palw_status(args: PalwStatusArgs) -> Result<(), String> {
    // A selector-less call is allowed since RPC wire v3: it reports the sink facts + the lagged
    // activation signal (review §6.4) without enumerating anything.
    kaspa_core::log::init_logger(None, "warn");
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|error| format!("getServerInfo failed: {error}"))?;
    if let Some(expected) = args.network.as_deref()
        && server.network_id.to_string() != expected
    {
        return Err(format!("network mismatch: node is '{}' but --network is '{expected}'", server.network_id));
    }
    let response = client
        .get_palw_state(GetPalwStateRequest { batch_id: args.batch_id.clone(), provider_bond_outpoint: args.provider_bond.clone(), pcpb_anchor_epoch: None, pcpb_a_commit: None })
        .await
        .map_err(|error| format!("getPalwState failed: {error}"))?;

    println!("node_network: {}", server.network_id);
    println!("node_synced: {}", server.is_synced);
    println!("palw_enabled: {}", response.enabled);
    println!("sink: {}", response.sink);
    println!("sink_daa_score: {}", response.sink_daa_score);
    println!("overlay_view_available: {}", response.overlay_view_available);
    println!("overlay_view_coordinate: past-relative (excludes sink body)");
    if args.batch_id.is_some() {
        println!("batch.provenance_scope: raw-carried-view + global-blob-availability (not selected-chain acceptance proof)");
    }
    if let Some(batch) = response.batch {
        println!("batch.in_sink_view: true");
        println!("batch.id: {}", batch.batch_id);
        println!("batch.status: {}", batch.status);
        println!(
            "batch.epochs: registration={} activation_not_before={} expiry={}",
            batch.registration_epoch, batch.activation_not_before_epoch, batch.expiry_epoch
        );
        println!("batch.chunks: {}/{}", batch.chunks_present_count, batch.chunk_count);
        println!("batch.leaf_blobs: {}/{}", batch.leaf_blobs_present, batch.leaf_count);
        println!("batch.leaf_scan_complete: {}", batch.leaf_scan_complete);
        println!("batch.manifest_present: {}", batch.manifest_present);
        println!("batch.manifest_hash: {}", batch.manifest_hash.as_deref().unwrap_or("none"));
        println!("batch.leaf_root: {}", batch.leaf_root);
        println!("batch.certificate_hash: {}", batch.certificate_hash.as_deref().unwrap_or("none"));
        println!("batch.certificate_blob_present: {}", batch.certificate_blob_present);
        println!(
            "batch.first_certificate_daa_score: {}",
            batch.first_certificate_daa_score.map_or_else(|| "none".to_string(), |value| value.to_string())
        );
        println!(
            "batch.revoked_from_daa_score: {}",
            batch.revoked_from_daa_score.map_or_else(|| "none".to_string(), |value| value.to_string())
        );
    } else if args.batch_id.is_some() {
        println!("batch.in_sink_view: false");
    }
    if let Some(provider) = response.provider_bond {
        println!("provider.in_registry: true");
        println!("provider.bond_outpoint: {}", provider.bond_outpoint);
        println!("provider.status: {}", provider.effective_status);
        println!("provider.amount_sompi: {}", provider.amount_sompi);
        println!("provider.owner_pubkey_hash: {}", provider.owner_pubkey_hash);
        println!("provider.operator_group_id: {}", provider.operator_group_id);
        println!("provider.runtime_classes: {}", provider.runtime_classes.join(","));
        println!(
            "provider.capacity_by_shape: {}",
            provider.capacity_by_shape.iter().map(|(shape, capacity)| format!("{shape}:{capacity}")).collect::<Vec<_>>().join(",")
        );
        println!("provider.reward_key_root: {}", provider.reward_key_root);
        println!("provider.unbond_delay_epochs: {}", provider.unbond_delay_epochs);
        println!("provider.activation_daa_score: {}", provider.activation_daa_score);
        println!(
            "provider.unbond_request_daa_score: {}",
            provider.unbond_request_daa_score.map_or_else(|| "none".to_string(), |value| value.to_string())
        );
        println!(
            "provider.release_daa_score: {}",
            provider.release_daa_score.map_or_else(|| "none".to_string(), |value| value.to_string())
        );
        println!(
            "provider.slashed_at_daa_score: {}",
            provider.slashed_at_daa_score.map_or_else(|| "none".to_string(), |value| value.to_string())
        );
    } else if args.provider_bond.is_some() {
        println!("provider.in_registry: false");
    }
    // Open availability challenges on the requested bond (empty without --provider-bond). One
    // line per challenge, key=value, so a soak/responder script can parse them directly.
    println!("da_challenges.open: {}", response.da_challenges.len());
    for challenge in &response.da_challenges {
        println!(
            "da_challenge: id={} object_root={} chunk={} deadline_daa={}",
            challenge.challenge_id, challenge.object_root, challenge.chunk_index, challenge.response_deadline_daa_score
        );
    }
    println!("search_challenges.open: {}", response.search_challenges.len());
    for challenge in &response.search_challenges {
        println!(
            "search_challenge: object_root={} scheduler_bond={} chunk={} deadline_daa={} availability_deadline_daa={}",
            challenge.object_root,
            challenge.scheduler_bond,
            challenge.chunk_index,
            challenge.response_deadline_daa_score,
            challenge.availability_deadline_daa_score
        );
    }
    // DA obligations for the requested batch, all statuses (empty without --batch-id, or from a node
    // older than wire v5). One key=value line per obligation so a lifecycle script can read the
    // obligation ids + sampled chunk indices it must challenge to satisfy the certificate DA gate.
    println!("da_obligations.count: {}", response.da_obligations.len());
    for obligation in &response.da_obligations {
        let status = match obligation.status {
            0 => "pending",
            1 => "challenged",
            2 => "satisfied",
            3 => "timedout",
            _ => "unknown",
        };
        println!(
            "da_obligation: id={} provider_bond={} object_root={} chunk={} status={} beacon_epoch={} retention_until_daa={}",
            obligation.obligation_id,
            obligation.provider_bond,
            obligation.object_root,
            obligation.chunk_index,
            status,
            obligation.beacon_epoch,
            obligation.retention_until_daa_score
        );
    }
    // §6.4 — the lagged Certified→Active gate signal, derived server-side with the EXACT consensus
    // walk `advance_epoch_gated` consumes. `none` from an older node (wire < v3) or a preset
    // without PALW/dns_params.
    match response.activation {
        Some(activation) => {
            println!("activation.open: {}", activation.activation_open);
            println!(
                "activation.newest_sample: epoch={} seed={}",
                activation.newest_sample_epoch.map_or_else(|| "none".to_string(), |value| value.to_string()),
                if activation.newest_sample_seed.is_empty() { "none" } else { &activation.newest_sample_seed }
            );
            println!(
                "activation.previous_sample: epoch={} seed={}",
                activation.previous_sample_epoch.map_or_else(|| "none".to_string(), |value| value.to_string()),
                if activation.previous_sample_seed.is_empty() { "none" } else { &activation.previous_sample_seed }
            );
            println!("activation.buried_sample_count: {}", activation.buried_sample_count);
            println!("activation.buried_carry_run: {} (lane open iff <= grace)", activation.buried_carry_run);
            println!("activation.anchor_hash: {}", if activation.anchor_hash.is_empty() { "none" } else { &activation.anchor_hash });
            println!("activation.current_epoch: {}", activation.current_epoch);
            println!("activation.grace_epochs: {}", activation.grace_epochs);
            println!(
                "activation.derived_mode: {} (sink's own per-block beacon state; distinct from the lagged signal)",
                if activation.derived_mode.is_empty() { "none" } else { &activation.derived_mode }
            );
            println!(
                "activation.derived_degraded_epochs: {}",
                activation.derived_degraded_epochs.map_or_else(|| "none".to_string(), |value| value.to_string())
            );
        }
        None => println!("activation: none (node predates wire v3, or PALW/dns_params absent)"),
    }
    let _ = client.disconnect().await;
    Ok(())
}

/// Stake mined coins into a new bond: load the validator key, find a funding UTXO at its own
/// address, build a signed `StakeBond` tx (locked output-0 == amount, change back to self),
/// submit it, and print the resulting `bond_outpoint` (`txid:0`) to pass to `run --stake-bond`.
async fn bond(args: BondArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let mass_calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );
    info!("[{VALIDATOR}] staking {} sompi as validator_id={} (funding {})", args.amount, key.validator_id, funding_addr);

    // Aggregate enough MATURE funding UTXOs to cover amount + fee. Mining pays the funding address
    // as many ~subsidy-sized coinbase fragments, so a single UTXO rarely covers a bond; sum the
    // largest mature ones (`build_funded_stake_bond_tx_multi`). A coinbase UTXO is unspendable
    // until `coinbase_maturity` blocks pass (consensus rule); a miner still paying this address
    // mints a fresh immature coinbase every block, so filter by maturity (else an immature pick
    // gets the bond tx rejected "spends an immature UTXO"). Mass-based fee unless overridden — the
    // StakeBond payload carries the 2592-byte pubkey and each ML-DSA-87 input is ~7 KB, so the fee
    // grows with the input count and is re-estimated as UTXOs are added.
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    // Largest-first greedy selection. Cap the input count so the bond tx stays within the block
    // mass limit (each ML-DSA-87 input is ~7 KB); 20 comfortably fits a reasonable testnet bond.
    const MAX_BOND_INPUTS: usize = 20;
    let (mature, mature_seen) =
        top_mature_funding_paged(&client, &funding_addr, virtual_daa, coinbase_maturity, MAX_BOND_INPUTS, None).await?;
    let mut selected = Vec::new();
    let mut sum: u64 = 0;
    let mut fee = match args.fee {
        Some(f) => f,
        None => key.estimate_bond_fee_for_inputs(&mass_calc, prefix, 1),
    };
    for e in mature.into_iter() {
        if selected.len() >= MAX_BOND_INPUTS {
            break;
        }
        sum = sum.saturating_add(e.entry.amount);
        selected.push(e);
        if args.fee.is_none() {
            fee = key.estimate_bond_fee_for_inputs(&mass_calc, prefix, selected.len());
        }
        if sum >= args.amount.saturating_add(fee) {
            break;
        }
    }
    let needed = args.amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
    if selected.is_empty() || sum < needed {
        return Err(format!(
            "not enough MATURE funding at {funding_addr}: have {sum} sompi across {} selected UTXO(s) \
             ({} mature scanned, cap {MAX_BOND_INPUTS}), \
             need {needed} sompi (amount {} + fee {fee}). Mine more to this address and wait for coinbase maturity \
             ({coinbase_maturity} blocks), or lower --amount.",
            selected.len(),
            mature_seen,
            args.amount
        ));
    }
    info!(
        "[{VALIDATOR}] funding bond from {} mature UTXO(s) totalling {sum} sompi (fee {fee} sompi{})",
        selected.len(),
        if args.fee.is_some() { "" } else { ", mass-based" }
    );
    let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = selected.into_iter().map(|e| (e.outpoint, e.entry)).collect();

    let tx = key.build_funded_stake_bond_tx_multi(
        args.amount,
        args.activation_daa_score,
        args.unbonding_period_blocks,
        key.reward_spk_payload(),
        &fundings,
        fee,
    )?;

    let txid =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
    info!("[{VALIDATOR}] submitted stake-bond tx (txid={txid})");
    // The bond outpoint is always output-0 of the bond tx.
    println!("bond_outpoint: {txid}:0");
    println!(
        "(once accepted + activation_daa_score reached, run: {VALIDATOR} run --validator-key <key> --stake-bond {txid}:0 --signed-epoch-db <db>)"
    );
    let _ = client.disconnect().await;
    Ok(())
}

/// kaspa-pq EVM Lane (§7.2): create an EVM_DEPOSIT_LOCK output — the UTXO side of a bridge
/// deposit. Mirrors `bond`'s mature-UTXO aggregation; output-0 is the lock binding the EVM
/// credit address / refund timeout / claim tip, refund script = this key's own funding P2PKH.
async fn deposit_lock(args: DepositLockArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    // Audit F4: refuse to create a deposit-lock on a network where the EVM lane is inert
    // (mainnet/simnet, or before activation). The claim path can never run there, so such a lock
    // could only be REFUNDED after its timeout (and a near-u64::MAX timeout would strand the funds
    // effectively forever). This is a CLI-side guard only — non-consensus, and the refund path
    // itself stays open so any lock that does exist remains recoverable.
    if !params.is_evm_active(server.virtual_daa_score) {
        return Err(format!(
            "EVM lane is not active on '{node_network}' (evm_activation_daa_score not reached; mainnet/simnet are inert) — \
             a deposit-lock here could only be refunded after the timeout, never claimed. Refusing to create it."
        ));
    }
    let mass_calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );

    // 20-byte EVM address (optional 0x). The deposit CREDITS this address on claim and a typo is
    // UNRECOVERABLE (the lock is consumed, no refund), so enforce EIP-55 + dangerous-target guards
    // here — the CLI boundary of MISAKA EVM Wallet Profile v1 (docs/misaka-evm-wallet-profile-v1.md).
    // Consensus serialization of `EvmAddress` is unchanged.
    let evm_hex = args.evm_address.strip_prefix("0x").or_else(|| args.evm_address.strip_prefix("0X")).unwrap_or(&args.evm_address);
    if evm_hex.len() != 40 || !evm_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("--evm-address must be 40 hex chars (20 bytes), got '{}'", args.evm_address));
    }
    let mut evm_address = [0u8; 20];
    faster_hex::hex_decode(evm_hex.to_ascii_lowercase().as_bytes(), &mut evm_address)
        .map_err(|e| format!("malformed --evm-address: {e}"))?;
    let checksummed = eip55_checksum(&evm_address);
    let has_upper = evm_hex.bytes().any(|b| b.is_ascii_uppercase());
    let has_lower = evm_hex.bytes().any(|b| b.is_ascii_lowercase());
    if has_upper && has_lower {
        // Mixed-case ⇒ an EIP-55 checksummed address: verify it (typo guard).
        if checksummed != format!("0x{evm_hex}") {
            return Err(format!(
                "--evm-address EIP-55 checksum INVALID — likely a typo. You entered 0x{evm_hex}; the checksum for those bytes is {checksummed}. Re-check the address (a wrong address is unrecoverable after the claim)."
            ));
        }
    } else {
        warn!(
            "[{VALIDATOR}] --evm-address has no EIP-55 checksum (single-case), so typos can't be detected — its checksummed form is {checksummed}. Prefer pasting the EIP-55 address."
        );
    }
    // A deposit credits a BALANCE (it does NOT call the contract), so these are almost always a
    // mistake: zero ⇒ refuse; system (F001/F002/F003) + EVM precompiles (0x01..0x09) ⇒ strong warn.
    if evm_address == [0u8; 20] {
        return Err("--evm-address is the ZERO address (0x000…000) — refusing (the credit would be unrecoverable).".to_string());
    }
    if evm_address[..16].iter().all(|&b| b == 0) {
        let tail = u32::from_be_bytes([evm_address[16], evm_address[17], evm_address[18], evm_address[19]]);
        if tail == 0xF001 || tail == 0xF002 || tail == 0xF003 || (1..=9).contains(&tail) {
            warn!(
                "[{VALIDATOR}] --evm-address {checksummed} is a SYSTEM/precompile address — depositing there is almost certainly a mistake (no normal account holds a balance there)."
            );
        }
    }

    let timeout_daa_score = server.virtual_daa_score.saturating_add(args.timeout_daa_delta);
    info!(
        "[{VALIDATOR}] depositing {} sompi to EVM {checksummed} (tip {}, refund timeout daa {timeout_daa_score}, funding {funding_addr})",
        args.amount, args.claim_tip
    );

    // Same mature-UTXO aggregation as `bond`.
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    const MAX_DEPOSIT_INPUTS: usize = 20;
    let (mature, mature_seen) =
        top_mature_funding_paged(&client, &funding_addr, virtual_daa, coinbase_maturity, MAX_DEPOSIT_INPUTS, None).await?;
    let mut selected = Vec::new();
    let mut sum: u64 = 0;
    let mut fee = match args.fee {
        Some(f) => f,
        None => key.estimate_deposit_lock_fee_for_inputs(&mass_calc, prefix, 1),
    };
    for e in mature.into_iter() {
        if selected.len() >= MAX_DEPOSIT_INPUTS {
            break;
        }
        sum = sum.saturating_add(e.entry.amount);
        selected.push(e);
        if args.fee.is_none() {
            fee = key.estimate_deposit_lock_fee_for_inputs(&mass_calc, prefix, selected.len());
        }
        if sum >= args.amount.saturating_add(fee) {
            break;
        }
    }
    let needed = args.amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
    if selected.is_empty() || sum < needed {
        return Err(format!(
            "not enough MATURE funding at {funding_addr}: have {sum} sompi across {} selected UTXO(s) \
             ({} mature scanned, cap {MAX_DEPOSIT_INPUTS}), \
             need {needed} sompi (amount {} + fee {fee}).",
            selected.len(),
            mature_seen,
            args.amount
        ));
    }
    info!(
        "[{VALIDATOR}] funding deposit from {} mature UTXO(s) totalling {sum} sompi (fee {fee} sompi{})",
        selected.len(),
        if args.fee.is_some() { "" } else { ", mass-based" }
    );
    let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = selected.into_iter().map(|e| (e.outpoint, e.entry)).collect();

    let tx = key.build_funded_deposit_lock_tx_multi(args.amount, evm_address, timeout_daa_score, args.claim_tip, &fundings, fee)?;
    let txid =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
    info!("[{VALIDATOR}] submitted deposit-lock tx (txid={txid})");
    println!("deposit_lock_outpoint: {txid}:0");
    println!(
        "(once accepted, claim on a MINING node: submitEvmDepositClaim {txid} 0 — the claim then executes in an accepting chain block and credits the EVM address)"
    );
    let _ = client.disconnect().await;
    Ok(())
}

/// Begin unbonding a `StakeBond`: load the validator key, find a single MATURE funding UTXO at its
/// funding address (NOT the bond's own locked output-0), build a signed `StakeUnbondRequest` for
/// `--stake-bond`, submit it, and print the result. After acceptance the bond enters `Unbonding`;
/// its locked stake becomes spendable once `unbonding_period_blocks` further blocks elapse.
async fn unbond(args: UnbondArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let bond_outpoint = parse_stake_bond_ref(&args.stake_bond)?;
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let mass_calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );
    // Mass-based fee unless overridden (the unbond payload carries the 2592-byte pubkey + 4627-byte sig).
    let fee = match args.fee {
        Some(f) => f,
        None => key.estimate_unbond_fee(&mass_calc, prefix),
    };
    info!(
        "[{VALIDATOR}] unbonding {bond_outpoint} (fee {fee} sompi{}) for validator_id={} (funding {funding_addr})",
        if args.fee.is_some() { "" } else { ", mass-based" },
        key.validator_id
    );

    // Need a single MATURE UTXO that covers the fee — and it must NOT be the bond's own locked
    // output-0: the consensus bond-spend-gate keeps that locked until release, so trying to pay the
    // fee from it would be rejected. Coinbase maturity is filtered for the same reason as `bond`
    // (a miner still paying this address mints a fresh immature coinbase every block).
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    let (mut candidates, mature_seen) =
        top_mature_funding_paged(&client, &funding_addr, virtual_daa, coinbase_maturity, 1, Some(bond_outpoint)).await?;
    let funding = candidates.pop().filter(|e| e.entry.amount > fee).ok_or_else(|| {
        format!(
            "no single MATURE funding UTXO > {} sompi (fee) at {funding_addr} other than the bond itself \
            ({} mature scanned); \
            send funds there and wait for coinbase maturity ({coinbase_maturity} blocks)",
            fee, mature_seen
        )
    })?;
    let funding_outpoint = funding.outpoint;
    let funding_entry = funding.entry;

    // audit M-04: bind the unbond authorization to this network's genesis hash (prevents replay
    // of the signed authorization on another network).
    let tx = key.build_funded_unbond_tx(params.genesis.hash.as_byte_slice(), bond_outpoint, funding_outpoint, &funding_entry, fee)?;

    let txid =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
    info!("[{VALIDATOR}] submitted unbond request (txid={txid}) for bond {bond_outpoint}");
    println!("unbond_request_txid: {txid}");
    println!("(once accepted the bond enters Unbonding; its locked stake is spendable after unbonding_period_blocks more blocks)");
    let _ = client.disconnect().await;
    Ok(())
}

/// Load generator (devnet stress): continuously scan mature UTXOs at the key's funding address
/// and spend each into a fan-out NATIVE transfer back to self, flooding the node's mempool with
/// valid ML-DSA transactions. Each fan-out output becomes a fresh spendable UTXO, so the UTXO
/// set (and the tx rate) grows until the mempool saturates. Submit errors (mempool full, already
/// spent, orphan) are expected under load and ignored. Runs until killed.
async fn spam(args: SpamArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let coinbase_maturity = params.coinbase_maturity();
    let storage_mass_parameter = params.storage_mass_parameter;
    info!(
        "[{VALIDATOR}] SPAM: flooding {node_network} from {funding_addr} (fanout={}, fee={}, interval={}ms). Fund it via `misaminer --wallet {funding_addr}`.",
        args.fanout, args.fee, args.interval_ms
    );

    let mut total: u64 = 0;
    loop {
        let virtual_daa = client.get_server_info().await.map(|s| s.virtual_daa_score).unwrap_or(0);
        let spendable =
            match top_mature_funding_paged(&client, &funding_addr, virtual_daa, coinbase_maturity, args.max_per_round, None).await {
                Ok((u, _)) => u,
                Err(e) => {
                    warn!("[{VALIDATOR}] SPAM: getUtxosByAddressPage failed: {e}");
                    tokio::time::sleep(Duration::from_millis(args.interval_ms)).await;
                    continue;
                }
            };

        let mut round = 0u64;
        for e in spendable.into_iter().filter(|e| e.entry.amount >= args.min_utxo).take(args.max_per_round) {
            let funding_outpoint = e.outpoint;
            let funding_entry = e.entry;
            let Ok(tx) = key.build_funded_split_tx(funding_outpoint, &funding_entry, args.fee, args.fanout, storage_mass_parameter)
            else {
                continue;
            };
            if client.submit_transaction(RpcTransaction::from(&tx), false).await.is_ok() {
                round += 1;
                total += 1;
            }
        }
        if round > 0 {
            info!("[{VALIDATOR}] SPAM: +{round} txs this round (total {total}, vDAA {virtual_daa})");
        }
        tokio::time::sleep(Duration::from_millis(args.interval_ms)).await;
    }
}

/// EIP-55 mixed-case checksum of a 20-byte address → `0x` + 40 case-encoded hex chars
/// (typo guard for deposit destinations; see docs/misaka-evm-wallet-profile-v1.md).
fn eip55_checksum(addr: &[u8; 20]) -> String {
    use sha3::{Digest, Keccak256};
    let lower: String = addr.iter().map(|b| format!("{b:02x}")).collect();
    let hash = Keccak256::digest(lower.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, c) in lower.chars().enumerate() {
        // Uppercase a hex letter iff the corresponding Keccak-256 nibble is ≥ 8 (EIP-55).
        if c.is_ascii_alphabetic() && ((hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0x0f) >= 8 {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// kaspa-pq EVM Lane (§9.2): submit a deposit-claim for an EVM_DEPOSIT_LOCK outpoint via
/// the node's `submitEvmDepositClaim` RPC (queues it for inclusion on this mining node).
async fn claim(args: ClaimArgs) -> Result<(), String> {
    let (txid, index_str) =
        args.outpoint.split_once(':').ok_or_else(|| format!("--outpoint must be 'txid_hex:index', got '{}'", args.outpoint))?;
    let index: u32 = index_str.parse().map_err(|e| format!("bad outpoint index '{index_str}': {e}"))?;
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let resp =
        client.submit_evm_deposit_claim(txid.to_string(), index).await.map_err(|e| format!("submitEvmDepositClaim failed: {e}"))?;
    info!("[{VALIDATOR}] submitted deposit-claim for {txid}:{index} -> {resp:?}");
    let _ = client.disconnect().await;
    Ok(())
}

/// kaspa-pq: one-shot headless balance. Resolves each address, queries the node's
/// `getBalancesByAddresses` (requires --utxoindex), and prints `address <sompi> <MSK> MSK` to
/// STDOUT — one tab-separated line per address (plus a TOTAL line for several) — then exits.
/// Connection / sync notes go to the log so STDOUT stays clean for scripting
/// (e.g. `kaspa-pq-validator balance --address misakatest:q... | awk '{print $2}'`).
async fn balance(args: BalanceArgs) -> Result<(), String> {
    let mut addrs = Vec::with_capacity(args.address.len());
    for a in &args.address {
        addrs.push(Address::try_from(a.as_str()).map_err(|e| format!("invalid address '{a}': {e}"))?);
    }
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        let _ = client.disconnect().await;
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    if !server.has_utxo_index {
        let _ = client.disconnect().await;
        return Err(format!("node '{node_network}' has no UTXO index — restart kaspad with --utxoindex"));
    }
    if !server.is_synced {
        info!("[{VALIDATOR}] WARNING: node '{node_network}' is NOT fully synced — balance may be stale");
    }
    let entries = client.get_balances_by_addresses(addrs.clone()).await.map_err(|e| format!("getBalancesByAddresses failed: {e}"))?;
    // Map the response back by address string (response order is not guaranteed).
    let found: std::collections::HashMap<String, u64> =
        entries.into_iter().map(|e| (e.address.to_string(), e.balance.unwrap_or(0))).collect();
    let mut total = 0u64;
    for a in &addrs {
        let bal = found.get(&a.to_string()).copied().unwrap_or(0);
        total = total.saturating_add(bal);
        println!("{a}\t{bal}\t{} MSK", format_msk(bal));
    }
    if addrs.len() > 1 {
        println!("TOTAL\t{total}\t{} MSK", format_msk(total));
    }
    let _ = client.disconnect().await;
    Ok(())
}

/// Format a sompi amount as MSK for display (L1 = 8 decimals; 1 MSK = 100_000_000 sompi).
fn format_msk(sompi: u64) -> String {
    format!("{}.{:08}", sompi / 100_000_000, sompi % 100_000_000)
}

/// On a connection failure, hint when the endpoint port looks like a DIFFERENT service
/// than the wRPC Borsh the validator needs (design §10.1) — the most common operator
/// mistake is pointing the validator at node gRPC / wRPC JSON / the EVM RPC.
fn port_kind_hint(node_rpc: &str) -> Option<&'static str> {
    let port: u16 = node_rpc.rsplit_once(':').and_then(|(_, p)| p.parse().ok())?;
    match port {
        26110 | 26210 | 26510 | 26610 => Some(
            "that port is node gRPC; the validator needs wRPC Borsh (testnet-200: 27210, devnet: 27610). Pass --node-wrpc-borsh <host:borsh-port>.",
        ),
        28110 | 28210 | 28510 | 28610 => Some(
            "that port is node wRPC JSON; the validator needs wRPC Borsh (testnet-200: 27210). Pass --node-wrpc-borsh <host:borsh-port>.",
        ),
        8545 => Some("that port is the EVM JSON-RPC; the validator does not use it. Pass --node-wrpc-borsh <host:borsh-port>."),
        _ => None,
    }
}

/// Resolve the node wRPC Borsh endpoint (design §7.3): an explicit `--node-wrpc-borsh`
/// / `KASPA_PQ_NODE_RPC` wins; else the local endpoint registry the node wrote
/// (`~/.misaka/<network-id>/endpoints.json`, which carries the actual bound port even if
/// non-standard); else the network-id default loopback. So a bare `--network-id testnet-200`
/// finds a co-located node with no port typed. The result is still re-verified against the
/// node's own network-id after connecting (the existing post-connect guard).
fn resolve_node_rpc(network: &Option<String>, explicit: &Option<String>) -> String {
    if let Some(e) = explicit {
        return e.clone();
    }
    if let Some(net) = network
        && let Ok(nid) = NetworkId::from_str(net)
    {
        return misaka_endpoints::resolve(
            &nid,
            EndpointKind::NodeWrpcBorsh,
            None,
            misaka_endpoints::EndpointRegistry::load(net).as_ref(),
        );
    }
    "127.0.0.1:27210".to_string()
}

async fn connect(node_rpc: &str) -> Result<KaspaRpcClient, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| format!("failed to build wRPC client: {e}"))?;
    // ConnectStrategy::Retry keeps the wRPC client's reconnection loop alive, so a node restart
    // (or any transient WebSocket drop) is recovered AUTOMATICALLY: the validator resumes attesting
    // once the node is back, instead of getting wedged in "WebSocket is not connected; retrying"
    // forever (Fallback tears the reconnect loop down on the first failure). `block_async_connect`
    // still waits for the FIRST connection so the network-id guard + first attestation run against a
    // live node. Combined with run_loop's per-round retry, this makes the validator survive node
    // restarts unattended — important on every network (a node bounce no longer silently stops
    // attestation, which would otherwise degrade DNS finality until a manual restart).
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Retry,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| {
        let mut msg = format!("failed to connect to node {url}: {e}");
        if let Some(hint) = port_kind_hint(node_rpc) {
            msg.push_str(&format!("\nhint: {hint}"));
        }
        msg
    })?;
    Ok(client)
}

async fn run_daemon(args: RunArgs) -> Result<(), String> {
    let node_rpc = resolve_node_rpc(&args.network, &args.node_rpc);
    info!("[{VALIDATOR}] connecting to local node at ws://{} (dry_run={})", node_rpc, args.dry_run);
    let client = connect(&node_rpc).await?;

    // Network-id guard (ADR-0011 §"Same network"): never attest against the wrong net.
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    match args.network.as_deref() {
        Some(expected) if node_network != expected => {
            return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
        }
        _ => {}
    }
    let prefix = prefix_for(server.network_id.network_type);
    let params = Params::from(server.network_id);
    let coinbase_maturity = params.coinbase_maturity();
    let mass_calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );
    info!("[{VALIDATOR}] connected: network={node_network} synced={} version={}", server.is_synced, server.server_version);

    // Load the signing identity if fully configured (key + bond + state DB); else observe.
    // Pin this network's genesis hash (C-01): the sidecar recomputes the canonical
    // attestation digest from it rather than trusting the RPC-supplied message.
    let attestor = Attestor::load(
        &args,
        prefix,
        coinbase_maturity,
        &mass_calc,
        params.storage_mass_parameter,
        params.max_block_mass,
        params.genesis.hash.as_byte_slice().to_vec(),
    )?;
    match &attestor {
        Some(a) => info!(
            "[{VALIDATOR}] attesting as validator_id={} (funding {}, fee {} sompi{})",
            a.key.validator_id,
            a.key.funding_address(prefix),
            a.attestation_fee,
            if args.fee.is_some() { "" } else { ", mass-based" }
        ),
        None => info!("[{VALIDATOR}] observe-only (need --validator-key + --stake-bond + --signed-epoch-db to attest)"),
    }

    // ADR-0011 §"Auto-startup ordering": tolerate every "not yet" state, loop until shutdown.
    let result = tokio::select! {
        r = run_loop(&client, &args, attestor) => r,
        _ = tokio::signal::ctrl_c() => {
            info!("[{VALIDATOR}] shutdown signal received");
            Ok(())
        }
    };
    let _ = client.disconnect().await;
    result
}

/// The ML-DSA-87 signing identity + equivocation guard, present only when fully
/// configured. Shares its primitives with the in-process service via
/// `kaspa-pq-validator-core`.
struct Attestor {
    key: ValidatorKey,
    bond_outpoint: TransactionOutpoint,
    signed_store: SignedEpochStore,
    prefix: Prefix,
    /// This network's genesis hash bytes — the attestation network discriminator
    /// (`stake_attestation_message` Addendum A.3). Pinned at load from the node's
    /// network id; audit C-01: the sidecar recomputes the canonical signing digest
    /// from this (never the RPC-supplied digest), so a malicious/desynced node
    /// cannot make it sign a non-canonical message.
    genesis_hash: Vec<u8>,
    /// Network coinbase-maturity (blocks); a coinbase funding UTXO younger than this cannot be
    /// spent for the attestation tx. Captured once at load from the node's network id.
    coinbase_maturity: u64,
    /// Attestation-shard fee (sompi), fixed once at load: the explicit `--fee` if given, else a
    /// mass-based estimate from the network's mass params (the shard tx shape is fixed, so the fee
    /// is constant across epochs). Either way it is far above the flat floor, which is below the
    /// kaspa-pq mempool minimum for this payload-heavy tx.
    attestation_fee: u64,
    /// KIP-0009 storage-mass parameter (`C`) for this network, used to keep the funding chain
    /// from producing an unminable tx: each chained hop pays the fee, so the change output shrinks
    /// every epoch, and a small output raises the tx's storage mass. Below a safe fraction of
    /// [`Self::max_tx_mass`] we re-seed from a fresh confirmed UTXO instead of chaining.
    storage_mass_parameter: u64,
    /// Network `max_block_mass` — the ceiling a tx's mass must stay under to be minable. We re-seed
    /// the funding chain well before the change shrinks enough to push storage mass past this.
    max_tx_mass: u64,
    /// The last epoch this PROCESS has already attested (submitted a shard for). Lets a short
    /// `--attest-poll-secs` revisit the same canonical-ready epoch cheaply without re-signing or
    /// rebroadcasting (which would burn a funding UTXO each poll). Reset on restart, so the
    /// persistent `SignedEpochStore` still drives a single crash-recovery rebroadcast.
    last_attested_epoch: Option<u64>,
    /// Head of the local funding chain: the change output (index 0, change back to self) of the
    /// most recently submitted attestation tx. The node's utxoindex keeps listing a just-spent
    /// funding UTXO as available until our tx is mined, so re-querying it each epoch re-selects an
    /// outpoint our own in-flight tx already spent → "output … already spent … in the mempool"
    /// rejection. Spending this change directly chains one funded hop per epoch across the
    /// unconfirmed window instead. In-memory only (reset on restart, which simply reselects a
    /// confirmed UTXO and starts a fresh chain).
    pending_change: Option<(TransactionOutpoint, UtxoEntry)>,
    /// Funding outpoints we have already spent in submitted (not-yet-mined) txs, mapped to the id of
    /// the tx that spent them, so the paginated fallback never re-selects one. Pruned (no
    /// full-UTXO-set scan) only once the spending tx has DEFINITIVELY left the mempool — a cheap
    /// per-txid `get_mempool_entry` lookup. NOT a fixed-age TTL: RPC-submitted txs are high-priority
    /// and never expire from the mempool, so a stuck spend must keep its exclusion until it actually
    /// mines or is dropped, else the fallback could re-pick a still-spent outpoint
    /// (RejectDoubleSpendInMempool → repeated failed attestations).
    inflight_spent: HashMap<TransactionOutpoint, TransactionId>,
    /// The tx id of the attestation that produced the current `pending_change` chain head. `None`
    /// when there is no in-flight chain. Used to detect head confirmation with a cheap per-txid
    /// `get_mempool_entry` lookup instead of fetching the whole funding address's UTXO set.
    chain_head_txid: Option<TransactionId>,
    /// kaspa-pq DNS-v3 hardening (Fix B): the epoch whose attestation produced the current
    /// `pending_change` chain head. `None` when there is no in-flight chain. Used to count
    /// distinct epochs the head has gone unconfirmed.
    chain_head_epoch: Option<u64>,
    /// kaspa-pq DNS-v3 hardening (Fix B): consecutive served epochs the funding-chain head has
    /// stayed in the mempool without confirming. Reset to 0 whenever the head leaves the mempool
    /// (mined or dropped) or the local pending chain is cleared. A present head is NOT abandoned:
    /// during congestion, re-funding from confirmed UTXOs creates parallel funding chains and
    /// amplifies the flood.
    stalled_epochs: u64,
}

impl Attestor {
    /// Load the signing identity iff `--validator-key`, `--stake-bond` and
    /// `--signed-epoch-db` are all provided. The state file is rejected if it belongs to a
    /// different validator/bond (cross-key equivocation guard).
    fn load(
        args: &RunArgs,
        prefix: Prefix,
        coinbase_maturity: u64,
        mass_calc: &MassCalculator,
        storage_mass_parameter: u64,
        max_tx_mass: u64,
        genesis_hash: Vec<u8>,
    ) -> Result<Option<Self>, String> {
        let (Some(key_path), Some(bond_ref), Some(db)) = (&args.validator_key, &args.stake_bond, &args.signed_epoch_db) else {
            return Ok(None);
        };
        let key = ValidatorKey::from_seed(load_validator_seed(key_path)?);
        let bond_outpoint = parse_stake_bond_ref(bond_ref)?;
        let signed_store = SignedEpochStore::load_or_empty(db.into(), key.validator_id, bond_outpoint)?;
        // Mass-based fee unless overridden (mirrors `bond`/`unbond`): an explicit `--fee` wins, else
        // size it from the network mass params (≈ 290 000 sompi for the shard's 4627-byte signature).
        let attestation_fee = args.fee.unwrap_or_else(|| key.estimate_attestation_fee(mass_calc, prefix));
        Ok(Some(Self {
            key,
            bond_outpoint,
            signed_store,
            prefix,
            genesis_hash,
            coinbase_maturity,
            attestation_fee,
            storage_mass_parameter,
            max_tx_mass,
            last_attested_epoch: None,
            pending_change: None,
            inflight_spent: HashMap::new(),
            chain_head_txid: None,
            chain_head_epoch: None,
            stalled_epochs: 0,
        }))
    }

    /// Sign the attestation `target` under the equivocation guard and (unless `dry_run`)
    /// fund + submit the `StakeAttestationShard` transaction. Returns `Err` only on a
    /// genuine failure (self-verify, funding, build, submit); the benign "already attested
    /// this epoch" path logs and returns `Ok`.
    async fn attest(
        &mut self,
        client: &KaspaRpcClient,
        target: &GetValidatorAttestationTargetResponse,
        dry_run: bool,
        virtual_daa: u64,
    ) -> Result<(), String> {
        let target_hash = parse_hash64(&target.target_hash)?;
        let vsc = parse_hash64(&target.validator_set_commitment)?;

        // C-01: recompute the canonical attestation digest LOCALLY from the
        // structured target + the pinned genesis hash + this validator's bond —
        // never trust the RPC-supplied digest. The RPC `message` is advisory: it
        // MUST equal the local recompute, else a malicious/desynced node is trying
        // to make us sign a non-canonical message → fail closed (no signature).
        let expected =
            stake_attestation_message(&self.genesis_hash, target.epoch, target_hash, target.target_daa_score, vsc, self.bond_outpoint);
        let digest = expected.as_bytes();
        let rpc_message = decode_message(&target.message)?;
        if rpc_message != digest {
            return Err(format!(
                "[{VALIDATOR}] attestation digest mismatch for epoch {}: the node's `message` does not equal the locally recomputed canonical digest; refusing to sign (possible malicious or desynced node)",
                target.epoch
            ));
        }

        // ADR-0011 equivocation guard + dry-run BEFORE signing (C-01: never sign
        // before the guard, never sign in a dry run). `check` is read-only and keys
        // on (epoch, target_hash, target_daa_score), so a placeholder fingerprint is
        // fine for the decision; the real fingerprint is stamped after signing.
        let mut record = SignedEpochRecord {
            epoch: target.epoch,
            target_hash,
            target_daa_score: target.target_daa_score,
            signature_fingerprint: Hash64::default(),
        };
        let outcome = self.signed_store.check(&record);
        match outcome {
            SignedEpochCheckOutcome::Block => {
                // One key signs at most one target per epoch; once it has committed to the
                // first anchor it saw this epoch, a later (moved-sink) target is refused.
                info!("[{VALIDATOR}] already attested epoch {} (target moved); skipping", target.epoch);
                return Ok(());
            }
            SignedEpochCheckOutcome::Allow | SignedEpochCheckOutcome::AllowRebroadcast => {}
        }

        if dry_run {
            info!(
                "[{VALIDATOR}] DRY-RUN epoch {} target={} (recomputed digest verified; not signing/submitting)",
                target.epoch, target.target_hash
            );
            return Ok(());
        }

        // Sign the LOCALLY-RECOMPUTED canonical digest (never the RPC bytes) + self-verify.
        let signature = self.key.sign_attestation(&digest);
        if !self.key.verify_attestation(&digest, &signature) {
            return Err("local attestation self-verify failed".to_string());
        }
        record.signature_fingerprint = signature_fingerprint(&signature);

        // Build the attestation shard.
        let att = StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: self.key.validator_id,
            bond_outpoint: self.bond_outpoint,
            epoch: target.epoch,
            target_hash,
            target_daa_score: target.target_daa_score,
            validator_set_commitment: vsc,
            signature: signature.to_vec(),
        };
        let shard = single_attestation_shard(att);

        // Fund the attestation at the validator's own P2PKH-ML-DSA address (needs node
        // --utxoindex). Funding model A: a small input pays the fee, change returns to self.
        let fee = self.attestation_fee;
        let funding_addr = self.key.funding_address(self.prefix);

        // kaspa-pq DNS-v3 + large-UTXO hardening: NEVER fetch the funding address's FULL UTXO set
        // per epoch. A miner that paid this address can pile up tens of thousands of coinbase UTXOs
        // (live-observed ~88k), turning the legacy `getUtxosByAddresses` into a multi-MiB response
        // every epoch that delays the attestation and starves the funding-chain tip. Instead: chain
        // off our own change output with NO node fetch in steady state; detect head confirmation
        // with a cheap per-txid mempool lookup; and only fall back to a BOUNDED, paginated
        // confirmed-UTXO search when the chain must be re-seeded.
        const STALL_WARN_EPOCHS: u64 = 3;

        // Self-heal the in-flight exclusion set WITHOUT a full-set scan: drop an outpoint only once
        // the tx that spent it has DEFINITIVELY left the mempool (mined ⇒ the outpoint is consumed
        // on-chain; dropped ⇒ the outpoint is freed and may be reused). Keep it on Present (still
        // spent) or Unknown (transient RPC error) — never free a still-spent outpoint for the
        // fallback to re-pick. NB RPC-submitted txs are high-priority and never expire, so a
        // time-based prune would be wrong; this is keyed off the spending tx's actual mempool state.
        // Cost bound (defense-in-depth): with the wRPC `Gone` fix above the set tracks
        // only the live funding chain and stays tiny, so this serial per-txid scan is
        // cheap. Should a future regression let it grow, cap the work per attest (the
        // remainder is revisited next round) and warn, rather than let one attest spend
        // seconds sweeping the set and fall behind the epoch.
        const INFLIGHT_SCAN_CAP: usize = 512;
        if self.inflight_spent.len() > INFLIGHT_SCAN_CAP {
            warn!(
                "[{VALIDATOR}] in-flight exclusion set is unexpectedly large ({}); mempool cleanup may be failing — bounding this round's scan to {}",
                self.inflight_spent.len(),
                INFLIGHT_SCAN_CAP
            );
        }
        let inflight_snapshot: Vec<(TransactionOutpoint, TransactionId)> =
            self.inflight_spent.iter().take(INFLIGHT_SCAN_CAP).map(|(op, txid)| (*op, *txid)).collect();
        for (op, spender) in inflight_snapshot {
            if let MempoolStatus::Gone = mempool_status(client, spender).await {
                self.inflight_spent.remove(&op);
            }
        }

        // Did the funding-chain head confirm? Ask the mempool for the head tx by id (one cheap
        // lookup) instead of scanning the whole address. Present ⇒ still pending, so keep chaining
        // from it and do NOT re-fund from a confirmed UTXO. Gone ⇒ mined (its change is now a
        // confirmed, chainable UTXO) OR dropped (the next chained spend fails to submit → the submit
        // handler clears the head and re-funds) — either way no longer stalled. Unknown (transient
        // RPC error) ⇒ make NO change to the counter, so a flaky lookup can neither falsely advance
        // nor falsely reset the stall.
        if self.pending_change.is_some() {
            let status = match self.chain_head_txid {
                Some(txid) => mempool_status(client, txid).await,
                None => MempoolStatus::Gone,
            };
            match status {
                MempoolStatus::Present => {
                    // attest() runs at most once per distinct epoch (the run loop short-circuits
                    // repeats via last_attested_epoch), so a changed target.epoch means a whole epoch
                    // elapsed without the head confirming.
                    if self.chain_head_epoch != Some(target.epoch) {
                        self.stalled_epochs = self.stalled_epochs.saturating_add(1);
                        self.chain_head_epoch = Some(target.epoch);
                    }
                    if self.stalled_epochs >= STALL_WARN_EPOCHS {
                        warn!(
                            "[{VALIDATOR}] funding-chain head still in mempool for {} epochs (now epoch {}); keeping pending chain, not re-funding",
                            self.stalled_epochs, target.epoch
                        );
                    }
                }
                MempoolStatus::Gone => {
                    self.stalled_epochs = 0;
                    self.chain_head_epoch = None;
                }
                MempoolStatus::Unknown => {}
            }
        } else {
            self.stalled_epochs = 0;
        }

        // Chain off our own change while it covers the fee (the mempool accepts a chained spend of an
        // unconfirmed parent) — NO node fetch. Otherwise page the funding address for a mature
        // confirmed UTXO (bounded scan; never the full 88k set).
        let (funding_outpoint, funding_entry) = match &self.pending_change {
            // Chain off our own change only while it (a) covers the fee, (b) is not already
            // in flight, and (c) would produce a tx whose KIP-0009 storage mass stays safely
            // under the block mass ceiling. The change shrinks by `fee` every epoch, so without
            // (c) the funding chain eventually builds a tx the mempool rejects with
            // `storage mass … > …` (~hourly), missing that epoch until the next tick re-seeds.
            Some((op, en))
                if en.amount > fee
                    && !self.inflight_spent.contains_key(op)
                    && chained_tx_storage_mass_safe(en, fee, self.storage_mass_parameter, self.max_tx_mass) =>
            {
                (*op, en.clone())
            }
            _ => {
                self.pending_change = None;
                self.chain_head_txid = None;
                select_funding_paged(
                    client,
                    &funding_addr,
                    &self.inflight_spent,
                    self.bond_outpoint,
                    fee,
                    virtual_daa,
                    self.coinbase_maturity,
                )
                .await?
            }
        };

        let tx = self.key.build_funded_shard_tx(&shard, funding_outpoint, &funding_entry, fee)?;

        // Persist the signing record BEFORE broadcasting, so a crash post-submit cannot lose
        // the record and let a restart sign a different target for this epoch.
        if outcome == SignedEpochCheckOutcome::Allow {
            self.signed_store.record_and_flush(record)?;
        }

        match client.submit_transaction(RpcTransaction::from(&tx), false).await {
            Ok(txid) => {
                info!("[{VALIDATOR}] submitted attestation shard for epoch {} (txid={txid})", target.epoch);
                // Advance the funding chain: this tx's change output (index 0, back to self) funds the
                // next epoch. The tx id excludes signature scripts, so it is stable post-sign and
                // matches the id the node assigns.
                self.inflight_spent.insert(funding_outpoint, tx.id());
                let change = UtxoEntry::new(funding_entry.amount - fee, funding_entry.script_public_key.clone(), virtual_daa, false);
                self.pending_change = Some((TransactionOutpoint::new(tx.id(), 0), change));
                // Record the head tx id (for the per-txid mempool confirmation lookup) and which
                // epoch produced it (so the stall counter advances once per unconfirmed epoch).
                self.chain_head_txid = Some(tx.id());
                self.chain_head_epoch = Some(target.epoch);
                Ok(())
            }
            Err(e) => {
                // Submit failed ⇒ no new change output exists. Drop the chain head so the next tick
                // re-funds (paginated); the in-flight set still excludes UTXOs our earlier (accepted)
                // txs spent, so the fallback won't re-pick a mempool-spent outpoint.
                self.pending_change = None;
                self.chain_head_txid = None;
                Err(format!("submitTransaction failed: {e}"))
            }
        }
    }
}

/// Residency of a tx in the node's normal (non-orphan) mempool, as a tri-state so a transient RPC
/// error is never confused with a definitive "not in the pool".
enum MempoolStatus {
    /// Still in the transaction pool (unconfirmed; its spends are live).
    Present,
    /// Definitively not in the pool — mined or dropped (`TransactionNotFound`).
    Gone,
    /// Could not be determined (transient RPC error); callers should make no state change.
    Unknown,
}

#[derive(Debug, Clone)]
struct FundingCandidate {
    outpoint: TransactionOutpoint,
    entry: UtxoEntry,
}

fn push_top_funding(top: &mut Vec<FundingCandidate>, candidate: FundingCandidate, max_candidates: usize) {
    if max_candidates == 0 {
        return;
    }
    if top.len() < max_candidates {
        top.push(candidate);
        return;
    }
    let Some((min_idx, min)) = top.iter().enumerate().min_by_key(|(_, e)| e.entry.amount) else {
        return;
    };
    if candidate.entry.amount > min.entry.amount {
        top[min_idx] = candidate;
    }
}

/// Scan the funding address through the paged UTXO API and keep only the largest mature
/// candidates needed by a command. This avoids the legacy unbounded `getUtxosByAddresses`
/// response on miner-contaminated addresses with 100k+ coinbase fragments.
async fn top_mature_funding_paged(
    client: &KaspaRpcClient,
    funding_addr: &Address,
    virtual_daa: u64,
    coinbase_maturity: u64,
    max_candidates: usize,
    exclude: Option<TransactionOutpoint>,
) -> Result<(Vec<FundingCandidate>, usize), String> {
    const PAGE_LIMIT: u64 = 1000;
    if max_candidates == 0 {
        return Ok((Vec::new(), 0));
    }
    let mut cursor = String::new();
    let mut top = Vec::new();
    let mut mature_seen = 0usize;
    loop {
        let page = client
            .get_utxos_by_address_page(funding_addr.clone(), cursor, PAGE_LIMIT)
            .await
            .map_err(|e| format!("getUtxosByAddressPage failed (does the node run --utxoindex?): {e}"))?;
        let next_cursor = page.next_cursor;
        for e in page.entries {
            let outpoint = TransactionOutpoint::from(e.outpoint);
            if exclude.as_ref().map(|x| *x == outpoint).unwrap_or(false) {
                continue;
            }
            let entry = UtxoEntry::from(e.utxo_entry);
            if !is_spendable(entry.is_coinbase, entry.block_daa_score, virtual_daa, coinbase_maturity) {
                continue;
            }
            mature_seen += 1;
            push_top_funding(&mut top, FundingCandidate { outpoint, entry }, max_candidates);
        }
        if next_cursor.is_empty() {
            break;
        }
        cursor = next_cursor;
    }
    top.sort_by(|a, b| b.entry.amount.cmp(&a.entry.amount));
    Ok((top, mature_seen))
}

/// Query whether `txid` is still resident in the node's normal mempool. Args
/// (include_orphan_pool=false, filter_transaction_pool=false) ⇒ the node queries TransactionsOnly;
/// NB it REJECTS (filter=true, orphan=false) as an inconsistent query, so `filter_transaction_pool`
/// MUST be false. One cheap per-txid lookup — never the whole funding address's UTXO set.
async fn mempool_status(client: &KaspaRpcClient, txid: TransactionId) -> MempoolStatus {
    match client.get_mempool_entry(txid, false, false).await {
        Ok(_) => MempoolStatus::Present,
        // Structured form (in-process / gRPC): the server's `TransactionNotFound`
        // (`#[error("Transaction {0} not found")]`, rpc/core/src/error.rs).
        Err(RpcError::TransactionNotFound(_)) => MempoolStatus::Gone,
        // wRPC stringifies EVERY server error to `RpcSubsystem(e.to_string())`
        // (rpc/macros/src/wrpc/client.rs), so over a wRPC connection the same
        // "not found" arrives as `RpcSubsystem("Transaction … not found")` and the
        // structured arm above never matches. Match the exact `TransactionNotFound`
        // Display shape (`"Transaction {txid} not found"`, rpc/core/src/error.rs) —
        // anchored at both ends so a generic transient error that merely contains the
        // words (e.g. "method not found") is NOT misread as Gone, which would free a
        // still-spent outpoint for re-selection (double-spend in the mempool). Without
        // this, a mined funding tx is judged `Unknown`, its outpoint is never dropped
        // from `inflight_spent`, and the exclusion set grows without bound until the
        // per-attest cleanup scan outlasts the epoch and every attestation misses.
        Err(RpcError::RpcSubsystem(msg)) if msg.starts_with("Transaction") && msg.ends_with("not found") => MempoolStatus::Gone,
        Err(_) => MempoolStatus::Unknown,
    }
}

/// Would a one-input/one-output funding-chain hop that spends `funding` and pays `fee` (leaving
/// `funding.amount - fee` as the change output) keep its KIP-0009 storage mass safely under the
/// block mass ceiling? The funding chain pays the fee from its own change every epoch, so the
/// change output shrinks over time, and a small output raises storage mass — eventually past the
/// limit, making the tx unminable. Re-seeding from a fresh (large) confirmed UTXO before that keeps
/// the chain minable. Leaves 20% headroom below `max_tx_mass`.
///
/// The estimate MUST match the node's: KIP-0009 storage mass scales with `plurality^2`, and a UTXO's
/// plurality is derived from its script-pubkey length (`utxo_plurality`), NOT 1. The validator's
/// funding address is a 69-byte ML-DSA-87 P2PKH, so its plurality is 2 (`ceil((95+69)/100)`), and the
/// change output reuses that same SPK. We take the input plurality straight from the funding
/// [`UtxoEntry`] via the same `UtxoCell::from` the node uses, and mirror it onto the change output —
/// hardcoding plurality 1 underestimates storage mass 4x and re-seeds far too late (the guarded tx is
/// then the very one the mempool rejects with `storage mass > …`).
fn chained_tx_storage_mass_safe(funding: &UtxoEntry, fee: u64, storage_mass_parameter: u64, max_tx_mass: u64) -> bool {
    let out_amount = funding.amount.saturating_sub(fee);
    if out_amount == 0 {
        return false;
    }
    let input_cell = UtxoCell::from(funding);
    let output_cell = UtxoCell { plurality: input_cell.plurality, amount: out_amount };
    let storage_mass = calc_storage_mass(false, std::iter::once(input_cell), std::iter::once(output_cell), storage_mass_parameter)
        .unwrap_or(u64::MAX);
    storage_mass <= max_tx_mass.saturating_mul(4) / 5
}

/// kaspa-pq large-UTXO hardening: find a mature CONFIRMED funding UTXO at the validator address via
/// the PAGINATED utxo index (op 160) instead of the legacy all-UTXO fetch — bounded to a few pages,
/// so a funding address contaminated with tens of thousands of coinbase UTXOs (a miner that paid
/// it) never forces a multi-MiB per-epoch response. Pages until it has seen a comfortably large
/// seed (one that funds a long change-chain) or hits a bounded page budget, then defers to the
/// shared, unit-tested `select_funding` to pick the largest qualifying UTXO from what it gathered.
async fn select_funding_paged(
    client: &KaspaRpcClient,
    funding_addr: &Address,
    inflight: &HashMap<TransactionOutpoint, TransactionId>,
    // kaspa-pq bond spend-gate hardening: the validator's own bond output-0, excluded from funding
    // candidates below. Its stake-lock is enforced solely by the consensus spend-gate, so spending it
    // gets the carrying block disqualified (a self-wedge). Mirrors the unbond path's exclusion.
    bond_outpoint: TransactionOutpoint,
    fee: u64,
    virtual_daa: u64,
    coinbase_maturity: u64,
) -> Result<(TransactionOutpoint, UtxoEntry), String> {
    const PAGE_LIMIT: u64 = 1000;
    const MAX_PAGES: usize = 16; // ≤16k UTXOs scanned even on a heavily-contaminated address
    // A seed > fee * this multiple funds a long change-chain, so once we see one we stop paging.
    const GOOD_ENOUGH_FEE_MULT: u64 = 64;
    let good_enough = fee.saturating_mul(GOOD_ENOUGH_FEE_MULT);

    let inflight_set: HashSet<TransactionOutpoint> = inflight.keys().copied().collect();
    let mut gathered: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::new();
    let mut cursor = String::new();
    for _ in 0..MAX_PAGES {
        let page = client
            .get_utxos_by_address_page(funding_addr.clone(), cursor, PAGE_LIMIT)
            .await
            .map_err(|e| format!("getUtxosByAddressPage failed (does the node run --utxoindex?): {e}"))?;
        let next_cursor = page.next_cursor;
        let mut seen_good = false;
        for e in page.entries {
            let op = TransactionOutpoint::from(e.outpoint);
            if op == bond_outpoint {
                continue; // never fund from our own locked bond output-0 (see signature note)
            }
            let en = UtxoEntry::from(e.utxo_entry);
            if en.amount > good_enough
                && is_spendable(en.is_coinbase, en.block_daa_score, virtual_daa, coinbase_maturity)
                && !inflight_set.contains(&op)
            {
                seen_good = true;
            }
            gathered.push((op, en));
        }
        if seen_good || next_cursor.is_empty() {
            break;
        }
        cursor = next_cursor;
    }
    // Reuse the shared, unit-tested selector: pending=None ⇒ it picks the largest mature, > fee,
    // not-in-flight UTXO from what we gathered (and errors with the same guidance if none qualify).
    select_funding(&None, &inflight_set, gathered, fee, virtual_daa, coinbase_maturity)
}

/// The ADR-0011 validator runtime loop. Returns `Err` only on the fatal `Slashed` state;
/// every other state sleeps and retries.
async fn run_loop(client: &KaspaRpcClient, args: &RunArgs, mut attestor: Option<Attestor>) -> Result<(), String> {
    loop {
        // 1. Sync guard (NodeNotSynced).
        let server = match client.get_server_info().await {
            Ok(s) => s,
            Err(e) => {
                warn!("[{VALIDATOR}] getServerInfo failed: {e}; retrying");
                sleep_secs(5).await;
                continue;
            }
        };
        if !server.is_synced {
            info!("[{VALIDATOR}] status=NodeNotSynced (virtual_daa={})", server.virtual_daa_score);
            sleep_secs(5).await;
            continue;
        }

        // 2. Bond configured?
        let Some(bond) = args.stake_bond.as_deref() else {
            info!("[{VALIDATOR}] status=Idle (no --stake-bond configured; observing only)");
            sleep_secs(30).await;
            continue;
        };

        // 3. Bond lifecycle (ADR-0011 state machine).
        let bond_resp = match client.get_stake_bond(GetStakeBondRequest { bond_outpoint: bond.to_owned() }).await {
            Ok(r) => r,
            Err(e) => {
                warn!("[{VALIDATOR}] getStakeBond failed: {e}; retrying");
                sleep_secs(15).await;
                continue;
            }
        };
        if !bond_resp.available {
            info!("[{VALIDATOR}] status=BondNotFound (bond {bond} not in the registry yet)");
            sleep_secs(30).await;
            continue;
        }
        match bond_resp.effective_status.as_str() {
            "pending" => {
                info!("[{VALIDATOR}] status=BondPending (activation_daa={})", bond_resp.activation_daa_score);
                sleep_secs(60).await;
            }
            "unbonding" => {
                warn!("[{VALIDATOR}] status=Unbonding; will stop attesting once finalised");
                sleep_secs(60).await;
            }
            "slashed" => {
                return Err(format!("status=Slashed: bond {bond} has been slashed (fatal)"));
            }
            "active" => {
                // ADR-0017: every active-bond validator attests. Fetch a BATCH of ready targets
                // from the first not-yet-attested epoch so a validator that briefly fell behind
                // catches up multiple epochs per poll — the singular target advances only one epoch
                // per poll, so a validator momentarily slower than the epoch cadence could never
                // catch up (this is the residual-miss issue behind the in-flight-set bug). Mirrors
                // the in-node ATTESTATION_CATCH_UP_LIMIT batch path.
                const CATCH_UP_LIMIT: u32 = 16;
                let from_epoch = attestor.as_ref().and_then(|a| a.last_attested_epoch).map(|e| e + 1).unwrap_or(0);
                match client
                    .get_validator_attestation_targets(GetValidatorAttestationTargetsRequest {
                        bond_outpoint: bond.to_owned(),
                        from_epoch,
                        limit: CATCH_UP_LIMIT,
                    })
                    .await
                {
                    Ok(resp) if resp.targets.is_empty() => {
                        info!("[{VALIDATOR}] status=ActiveIdle (no attestation target available this tick)")
                    }
                    Ok(resp) => {
                        if let Some(a) = attestor.as_mut() {
                            // Attest each ready target, then advance the per-process cursor to the
                            // MAX epoch attested this batch. The transport order is NOT guaranteed
                            // ascending — the consensus method returns the steady-state (all-
                            // certified) fallback newest-first — so advancing to the last-processed
                            // element would let the cursor regress and re-broadcast the whole tail
                            // every poll (burning funding UTXOs). Skip any epoch at/below the cursor
                            // (already attested this run or a prior run). Stop the batch on the first
                            // funding/submit failure (the chain is broken for this round; retry next poll).
                            let mut cursor = a.last_attested_epoch;
                            for rt in resp.targets {
                                // kaspa-pq DNS-v3 hardening (Fix A — anchor-deep start-gate): never
                                // attest an epoch whose canonical lagged anchor predates the bond's
                                // activation — such a shard is block-invalid (consensus §B.4) and
                                // would submit-OK but never mine, stalling the funding chain. Skip
                                // it and keep scanning other targets.
                                if rt.target_daa_score < bond_resp.activation_daa_score {
                                    info!(
                                        "[{VALIDATOR}] status=ActiveBelowActivation epoch={} target_daa={} < activation_daa={} (gating until anchor-deep)",
                                        rt.epoch, rt.target_daa_score, bond_resp.activation_daa_score
                                    );
                                    continue;
                                }
                                // Already attested this run (or earlier); skip cheaply, no re-sign /
                                // rebroadcast (a short poll revisits the same ready window).
                                if cursor.is_some_and(|c| rt.epoch <= c) {
                                    continue;
                                }
                                let target = GetValidatorAttestationTargetResponse {
                                    available: true,
                                    epoch: rt.epoch,
                                    target_hash: rt.target_hash,
                                    target_daa_score: rt.target_daa_score,
                                    validator_set_commitment: rt.validator_set_commitment,
                                    message: rt.message,
                                };
                                match a.attest(client, &target, args.dry_run, server.virtual_daa_score).await {
                                    Ok(()) => cursor = Some(cursor.map_or(target.epoch, |c| c.max(target.epoch))),
                                    Err(e) => {
                                        warn!("[{VALIDATOR}] attest failed for epoch {}: {e}; stopping this batch", target.epoch);
                                        break;
                                    }
                                }
                            }
                            a.last_attested_epoch = cursor;
                        } else {
                            let newest = resp.targets.last().expect("non-empty");
                            info!(
                                "[{VALIDATOR}] status=ActiveEligible epoch={} target={} ({} ready; observe-only, not signing)",
                                newest.epoch,
                                newest.target_hash,
                                resp.targets.len()
                            );
                        }
                    }
                    Err(e) => warn!("[{VALIDATOR}] getValidatorAttestationTargets failed: {e}"),
                }
                sleep_secs(args.attest_poll_secs).await;
            }
            other => {
                warn!("[{VALIDATOR}] unknown bond status '{other}'; retrying");
                sleep_secs(30).await;
            }
        }
    }
}

/// Map the node's `NetworkType` to the bech32 address `Prefix` (for the funding address).
fn prefix_for(network_type: NetworkType) -> Prefix {
    match network_type {
        NetworkType::Mainnet => Prefix::Mainnet,
        NetworkType::Testnet => Prefix::Testnet,
        NetworkType::Devnet => Prefix::Devnet,
        NetworkType::Simnet => Prefix::Simnet,
    }
}

/// Parse a network name {mainnet, testnet, devnet, simnet} to its address `Prefix`.
fn parse_prefix(s: &str) -> Result<Prefix, String> {
    // Accept both the NetworkType form ("testnet") and the NetworkId form
    // ("testnet-10") so the unified CLI's --network-id flows through keygen; the
    // testnet suffix only affects the P2P port, never the address prefix.
    let base = s.split('-').next().unwrap_or(s);
    match base.to_ascii_lowercase().as_str() {
        "mainnet" => Ok(Prefix::Mainnet),
        "testnet" => Ok(Prefix::Testnet),
        "devnet" => Ok(Prefix::Devnet),
        "simnet" => Ok(Prefix::Simnet),
        _ => Err(format!("unknown network '{s}' (expected mainnet/testnet/devnet/simnet, optionally with a -suffix)")),
    }
}

/// Decode the 32-byte ready-to-sign attestation message digest (hex).
fn decode_message(hex: &str) -> Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    faster_hex::hex_decode(hex.as_bytes(), &mut out).map_err(|e| format!("bad attestation message hex '{hex}': {e}"))?;
    Ok(out)
}

#[derive(Parser, Debug)]
struct PalwSearchRetrievalArgs {
    /// Local node wRPC (borsh) endpoint, host:port.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Expected network id; also resolves the default loopback endpoint.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Provider endpoint (operator-local SearXNG), e.g. http://127.0.0.1:8080.
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    endpoint: String,

    /// Search query exactly as assigned.
    #[arg(long)]
    query: String,

    /// Ruleset pin recorded in the snapshot.
    #[arg(long, default_value = "palw-search-v1")]
    ruleset_id: String,

    /// Scheduler assignment id (128-char Hash64 hex); omitted = zero (unassigned diagnostic).
    #[arg(long)]
    assignment_id: Option<String>,

    /// Provider policy document hash (128-char Hash64 hex); omitted = zero (unpinned draft).
    #[arg(long)]
    provider_policy_id: Option<String>,

    /// Provider language setting.
    #[arg(long, default_value = "ja-JP")]
    language: String,

    /// Region tag recorded in the snapshot.
    #[arg(long, default_value = "jp")]
    region: String,

    /// Safe-search level 0..=2.
    #[arg(long, default_value_t = 1)]
    safe_search: u8,

    /// Freshness window in milliseconds granted from retrieval time.
    #[arg(long, default_value_t = palw_search_retrieval::DEFAULT_FRESHNESS_WINDOW_MS)]
    freshness_ms: u64,

    /// Provider timeout in milliseconds.
    #[arg(long, default_value_t = palw_search_retrieval::DEFAULT_TIMEOUT_MS)]
    timeout_ms: u64,

    /// Maximum ranked results kept.
    #[arg(long, default_value_t = 16)]
    max_results: usize,

    /// Signed PalwSearchAssignmentV1 file (from palw-search-assign). Pins query/policy/freshness
    /// and sets the snapshot's assignment id; CLI flags they overlap are ignored.
    #[arg(long)]
    assignment: Option<String>,

    /// Scheduler ML-DSA-87 seed. With --assignment, signs the snapshot anchor and writes a
    /// complete verified PalwSearchJobSpecV1 next to the snapshot.
    #[arg(long)]
    scheduler_seed: Option<String>,

    /// Availability deadline (DAA score) promised in the signed anchor.
    #[arg(long)]
    availability_deadline_daa: Option<u64>,

    /// Output directory for the canonical object bytes + manifest.
    #[arg(long, default_value = "palw-search-snapshots")]
    out: String,
}

#[derive(Parser, Debug)]
struct PalwSearchAssignArgs {
    /// Local node wRPC (borsh) endpoint, host:port.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Expected network id; also resolves the default loopback endpoint.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Query to pin (normalized with the v1 rule before signing).
    #[arg(long)]
    query: String,

    /// Provider language / region / safe-search pins.
    #[arg(long, default_value = "ja-JP")]
    language: String,
    #[arg(long, default_value = "jp")]
    region: String,
    #[arg(long, default_value_t = 1)]
    safe_search: u8,

    /// Provider policy document hash (128-char Hash64 hex); omitted = zero (unpinned draft).
    #[arg(long)]
    provider_policy_id: Option<String>,

    /// Ruleset pin.
    #[arg(long, default_value = "palw-search-v1")]
    ruleset_id: String,

    /// Maximum ranked results the snapshot may carry.
    #[arg(long, default_value_t = 8)]
    max_results: u16,

    /// Freshness window in milliseconds.
    #[arg(long, default_value_t = palw_search_retrieval::DEFAULT_FRESHNESS_WINDOW_MS)]
    freshness_ms: u64,

    /// Validity window length in DAA scores, starting at the node's current virtual DAA.
    #[arg(long, default_value_t = 600)]
    valid_for_daa: u64,

    /// Scheduler's on-chain authority anchor: an active provider-bond outpoint (txid_hex:index)
    /// whose owner key must equal the scheduler key (bonded scheduler registry).
    #[arg(long)]
    scheduler_bond: String,

    /// Scheduler ML-DSA-87 seed path (signs the assignment).
    #[arg(long)]
    scheduler_seed: String,

    /// Output directory.
    #[arg(long, default_value = "palw-search-snapshots")]
    out: String,
}

fn mldsa_verify_adapter(public_key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    matches!(kaspa_txscript::verify_mldsa87_with_context(public_key, message, signature, context), Ok(true))
}

/// Build + sign + persist one search assignment against live node anchor facts.
async fn palw_search_assign_cmd(args: PalwSearchAssignArgs) -> Result<(), String> {
    use kaspa_consensus_core::palw::search_snapshot::{
        PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT, PALW_SEARCH_ASSIGNMENT_VERSION_V1, PalwSearchAssignmentV1, PalwSearchProviderPolicyV1,
        normalize_query_v1,
    };
    let node_rpc = resolve_node_rpc(&args.network, &args.node_rpc);
    let client = connect(&node_rpc).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let params = Params::from(server.network_id);
    let key = ValidatorKey::from_seed(load_validator_seed(&args.scheduler_seed)?);
    let mut assignment = PalwSearchAssignmentV1 {
        version: PALW_SEARCH_ASSIGNMENT_VERSION_V1,
        network_id: server.network_id.suffix.unwrap_or(0),
        genesis_hash: params.genesis.hash,
        ruleset_id: args.ruleset_id,
        normalized_query: normalize_query_v1(&args.query),
        provider: PalwSearchProviderPolicyV1 {
            provider_id: "searxng".into(),
            policy_id: match &args.provider_policy_id {
                Some(hex) => parse_hash64(hex)?,
                None => Hash64::from_bytes([0; 64]),
            },
            region: args.region,
            language: args.language,
            safe_search: args.safe_search.min(2),
        },
        max_results: args.max_results,
        freshness_window_millis: args.freshness_ms,
        valid_from_daa_score: server.virtual_daa_score,
        valid_until_daa_score: server.virtual_daa_score.saturating_add(args.valid_for_daa),
        scheduler_bond: parse_stake_bond_ref(&args.scheduler_bond)?,
        scheduler_public_key: key.public_key().to_vec(),
        signature: vec![],
    };
    let signing = assignment.signing_bytes().map_err(|e| format!("assignment signing bytes: {e}"))?;
    assignment.signature = key.sign_with_context(&signing, PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT).to_vec();
    assignment.verify_signature(mldsa_verify_adapter).map_err(|e| format!("self-check failed: {e}"))?;
    let assignment_id = assignment.assignment_id().map_err(|e| format!("assignment id: {e}"))?;
    let bytes = assignment.encode().map_err(|e| format!("assignment encode: {e}"))?;
    std::fs::create_dir_all(&args.out).map_err(|e| format!("cannot create {}: {e}", args.out))?;
    let id_hex = format!("{assignment_id}");
    let path = format!("{}/palw-search-assignment-{}.bin", args.out, &id_hex[..16]);
    std::fs::write(&path, &bytes).map_err(|e| format!("cannot write {path}: {e}"))?;
    let manifest = serde_json::json!({
        "schema": "misaka.palw.search-assignment-manifest.v1",
        "assignment_id": id_hex,
        "assignment_file": path,
        "normalized_query": assignment.normalized_query,
        "valid_from_daa_score": assignment.valid_from_daa_score,
        "valid_until_daa_score": assignment.valid_until_daa_score,
        "scheduler_key_id": format!("{}", kaspa_consensus_core::palw::search_snapshot::scheduler_key_id(key.public_key())),
        "next": "palw-search-retrieval --assignment <file> [--scheduler-seed <seed> --availability-deadline-daa <daa>]",
    });
    println!("{}", serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?);
    Ok(())
}

/// One-shot node-side retrieval: node-anchor facts over wRPC, egress-guarded provider search,
/// canonical snapshot bytes + manifest on disk. Admission stays a separate, explicit step
/// (`ConsensusApi::palw_admit_search_snapshot`) so operators can inspect exactly what will be
/// stored before the node commits to serving it.
async fn palw_search_retrieval_cmd(args: PalwSearchRetrievalArgs) -> Result<(), String> {
    let node_rpc = resolve_node_rpc(&args.network, &args.node_rpc);
    let client = connect(&node_rpc).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    if let Some(expected) = &args.network
        && let Ok(expected_id) = NetworkId::from_str(expected)
        && expected_id != server.network_id
    {
        return Err(format!("node network {} does not match --network {expected_id}", server.network_id));
    }
    let params = Params::from(server.network_id);
    let anchor = palw_search_retrieval::NodeAnchorV1 {
        network_id: server.network_id.suffix.unwrap_or(0),
        genesis_hash: params.genesis.hash,
        daa_score: server.virtual_daa_score,
        unix_millis: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("system clock before Unix epoch: {e}"))?
            .as_millis() as u64,
    };
    let assignment = match &args.assignment {
        Some(path) => {
            let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
            let assignment = kaspa_consensus_core::palw::search_snapshot::PalwSearchAssignmentV1::decode_strict(&bytes)
                .map_err(|e| format!("assignment decode: {e}"))?;
            assignment.verify_signature(mldsa_verify_adapter).map_err(|e| format!("assignment signature: {e}"))?;
            if anchor.daa_score < assignment.valid_from_daa_score || anchor.daa_score > assignment.valid_until_daa_score {
                return Err(format!(
                    "node DAA {} is outside the assignment validity window {}..={}",
                    anchor.daa_score, assignment.valid_from_daa_score, assignment.valid_until_daa_score
                ));
            }
            Some(assignment)
        }
        None => None,
    };
    let request = match &assignment {
        // The assignment pins the retrieval facts; overlapping CLI flags are ignored so the
        // snapshot can only be built the way the scheduler signed it.
        Some(assignment) => palw_search_retrieval::RetrievalRequestV1 {
            endpoint: args.endpoint,
            query: assignment.normalized_query.clone(),
            ruleset_id: assignment.ruleset_id.clone(),
            assignment_id: assignment.assignment_id().map_err(|e| format!("assignment id: {e}"))?,
            provider_policy_id: assignment.provider.policy_id,
            language: assignment.provider.language.clone(),
            region: assignment.provider.region.clone(),
            safe_search: assignment.provider.safe_search,
            freshness_window_millis: assignment.freshness_window_millis,
        },
        None => palw_search_retrieval::RetrievalRequestV1 {
            endpoint: args.endpoint,
            query: args.query,
            ruleset_id: args.ruleset_id,
            assignment_id: match &args.assignment_id {
                Some(hex) => parse_hash64(hex)?,
                None => Hash64::from_bytes([0; 64]),
            },
            provider_policy_id: match &args.provider_policy_id {
                Some(hex) => parse_hash64(hex)?,
                None => Hash64::from_bytes([0; 64]),
            },
            language: args.language,
            region: args.region,
            safe_search: args.safe_search,
            freshness_window_millis: args.freshness_ms,
        },
    };
    let policy =
        palw_search_retrieval::RetrievalPolicyV1 { timeout_ms: args.timeout_ms, max_results: args.max_results, ..Default::default() };
    let snapshot = tokio::task::spawn_blocking(move || palw_search_retrieval::retrieve_search_snapshot(&request, &anchor, &policy))
        .await
        .map_err(|e| format!("retrieval task join failure: {e}"))??;

    let bytes = snapshot.encode().map_err(|e| format!("snapshot encode: {e}"))?;
    let digest = snapshot.digest().map_err(|e| format!("snapshot digest: {e}"))?;
    let commitment = snapshot.da_commitment().map_err(|e| format!("snapshot commitment: {e}"))?;
    std::fs::create_dir_all(&args.out).map_err(|e| format!("cannot create {}: {e}", args.out))?;
    let root_hex = format!("{}", commitment.root);
    let object_path = format!("{}/palw-search-snapshot-{}.bin", args.out, &root_hex[..16]);
    let manifest_path = format!("{}/palw-search-snapshot-{}.manifest.json", args.out, &root_hex[..16]);
    std::fs::write(&object_path, &bytes).map_err(|e| format!("cannot write {object_path}: {e}"))?;
    let manifest = serde_json::json!({
        "schema": "misaka.palw.search-snapshot-manifest.v1",
        "object_root": root_hex,
        "snapshot_digest": format!("{digest}"),
        "object_len": commitment.object_len,
        "chunk_count": commitment.chunk_count,
        "network_id": snapshot.network_id,
        "genesis_hash": format!("{}", snapshot.genesis_hash),
        "retrieval_daa_score": snapshot.retrieval_daa_score,
        "retrieval_unix_millis": snapshot.retrieval_unix_millis,
        "freshness_deadline_millis": snapshot.freshness_deadline_millis,
        "outcome": snapshot.outcome,
        "sources": snapshot
            .results
            .iter()
            .map(|r| serde_json::json!({"rank": r.rank, "title": r.title, "url": r.url}))
            .collect::<Vec<_>>(),
        "object_file": object_path,
        "admission_api": "ConsensusApi::palw_admit_search_snapshot(object_bytes)",
        "consensus_weight": "none — assignment-id resolution is a StopShip gate before P2P/mint exposure",
    });
    let manifest_text = serde_json::to_string_pretty(&manifest).map_err(|e| format!("manifest encode: {e}"))?;
    std::fs::write(&manifest_path, format!("{manifest_text}\n")).map_err(|e| format!("cannot write {manifest_path}: {e}"))?;
    println!("{manifest_text}");

    // JobSpec assembly: sign the anchor over exactly the admitted-recomputable facts and
    // self-verify the complete spec before persisting it for workers.
    if let (Some(assignment), Some(seed)) = (assignment, &args.scheduler_seed) {
        use kaspa_consensus_core::palw::search_snapshot::{
            PALW_SEARCH_ANCHOR_MLDSA87_CONTEXT, PalwSearchJobSpecV1, PalwSearchSnapshotAnchorV1, PalwSignedSearchAnchorV1,
        };
        let deadline = args.availability_deadline_daa.ok_or("--availability-deadline-daa is required when signing a JobSpec")?;
        let key = ValidatorKey::from_seed(load_validator_seed(seed)?);
        if key.public_key() != assignment.scheduler_public_key.as_slice() {
            return Err("scheduler seed does not match the assignment's scheduler key".to_string());
        }
        let anchor = PalwSearchSnapshotAnchorV1 {
            assignment_id: assignment.assignment_id().map_err(|e| format!("assignment id: {e}"))?,
            snapshot_digest: digest,
            object_root: commitment.root,
            object_len: commitment.object_len,
            chunk_count: commitment.chunk_count,
            availability_deadline_daa_score: deadline,
        };
        let signature = key.sign_with_context(anchor.signing_hash().as_byte_slice(), PALW_SEARCH_ANCHOR_MLDSA87_CONTEXT).to_vec();
        let jobspec = PalwSearchJobSpecV1 {
            signed_anchor: PalwSignedSearchAnchorV1 { anchor, scheduler_public_key: key.public_key().to_vec(), signature },
            assignment,
        };
        jobspec.verify(mldsa_verify_adapter).map_err(|e| format!("JobSpec self-verification failed: {e}"))?;
        let jobspec_path = format!("{}/palw-search-jobspec-{}.json", args.out, &root_hex[..16]);
        let jobspec_text = serde_json::to_string_pretty(&jobspec).map_err(|e| format!("jobspec encode: {e}"))?;
        std::fs::write(&jobspec_path, format!("{jobspec_text}\n")).map_err(|e| format!("cannot write {jobspec_path}: {e}"))?;
        println!("{{\"jobspec_file\": \"{jobspec_path}\", \"verified\": true}}");
    }
    Ok(())
}

/// Parse a 64-byte Hash64 from hex (128 chars).
fn parse_hash64(hex: &str) -> Result<Hash64, String> {
    Hash64::from_str(hex).map_err(|e| format!("bad Hash64 hex '{hex}': {e}"))
}

async fn sleep_secs(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

/// Parse a stake/amount string into u64 sompi (design §13.3). Accepts:
///   - a bare integer or `<n>sompi` — already sompi (whole numbers only);
///   - `<n>MSK` / `<n>KAS` / `<n.m>MSK` — 1 MSK = 100_000_000 sompi, up to 8 decimals.
/// Integer math throughout (no f64 precision loss); rejects junk and u64 overflow.
fn parse_amount_sompi(s: &str) -> Result<u64, String> {
    const SOMPI_PER_MSK: u64 = 100_000_000;
    let s = s.trim();
    if s.is_empty() {
        return Err("amount is empty".to_string());
    }
    let lower = s.to_ascii_lowercase();
    let (num, is_coins) = if let Some(n) = lower.strip_suffix("sompi") {
        (n.trim(), false)
    } else if let Some(n) = lower.strip_suffix("msk") {
        (n.trim(), true)
    } else if let Some(n) = lower.strip_suffix("kas") {
        (n.trim(), true)
    } else {
        (lower.as_str(), false) // bare = sompi
    };
    if num.is_empty() {
        return Err(format!("missing number in amount '{s}'"));
    }
    if !is_coins {
        // sompi is indivisible — whole numbers only.
        return num.parse::<u64>().map_err(|_| format!("invalid sompi amount '{s}' (must be a whole number)"));
    }
    // MSK/KAS: fixed-point with up to 8 fractional digits, parsed as integers.
    let (int_part, frac_part) = num.split_once('.').unwrap_or((num, ""));
    if frac_part.len() > 8 {
        return Err(format!("too many decimals in '{s}' (max 8 for MSK/KAS)"));
    }
    if !int_part.chars().all(|c| c.is_ascii_digit()) || !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid amount '{s}'"));
    }
    let int_v: u64 = if int_part.is_empty() { 0 } else { int_part.parse().map_err(|_| format!("invalid amount '{s}'"))? };
    // Right-pad the fraction to 8 digits → its value is directly in sompi.
    let frac_v: u64 =
        if frac_part.is_empty() { 0 } else { format!("{frac_part:0<8}").parse().map_err(|_| format!("invalid fraction in '{s}'"))? };
    int_v
        .checked_mul(SOMPI_PER_MSK)
        .and_then(|whole| whole.checked_add(frac_v))
        .ok_or_else(|| format!("amount '{s}' overflows u64 sompi"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefix_known_and_unknown() {
        assert_eq!(parse_prefix("mainnet").unwrap(), Prefix::Mainnet);
        assert_eq!(parse_prefix("SIMNET").unwrap(), Prefix::Simnet);
        // NetworkId suffix form (the unified CLI's --network-id) resolves to the base prefix.
        assert_eq!(parse_prefix("testnet-10").unwrap(), Prefix::Testnet);
        assert_eq!(parse_prefix("testnet-11").unwrap(), Prefix::Testnet);
        assert!(parse_prefix("bogus").is_err());
    }

    #[test]
    fn prefix_for_maps_every_network() {
        assert_eq!(prefix_for(NetworkType::Mainnet), Prefix::Mainnet);
        assert_eq!(prefix_for(NetworkType::Testnet), Prefix::Testnet);
        assert_eq!(prefix_for(NetworkType::Devnet), Prefix::Devnet);
        assert_eq!(prefix_for(NetworkType::Simnet), Prefix::Simnet);
    }

    #[test]
    fn storage_mass_guard_uses_real_spk_plurality() {
        // The funding UTXO + its change are 69-byte ML-DSA-87 P2PKH scripts, whose KIP-0009
        // plurality is 2 (ceil((95+69)/100)). Storage mass scales with plurality^2, so the re-seed
        // guard MUST use the SPK's real plurality (via UtxoCell::from, exactly as the node does),
        // NOT a hardcoded 1 — else it underestimates 4x and re-seeds too late, letting the mempool
        // reject the very tx it approved.
        let spk = kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0u8; 69]);
        let entry = |amount| UtxoEntry::new(amount, spk.clone(), 0, false);
        let storm = 1_000_000_000_000u64; // STORAGE_MASS_PARAMETER (SOMPI_PER_KASPA * 10_000)
        let max_tx_mass = 500_000u64; // testnet max_block_mass; guard threshold = 4/5 = 400_000
        let threshold = max_tx_mass * 4 / 5;
        let fee = 250_000u64;

        // The guard derives plurality from the entry the same way the node's calc_contextual_masses
        // does — lock it to 2.
        let funding = entry(1_500_000);
        let in_cell = UtxoCell::from(&funding);
        assert_eq!(in_cell.plurality, 2, "69-byte SPK => plurality 2 (node-identical)");

        // At this change size the plurality-2 storage mass is over the threshold while a plurality-1
        // estimate is below it.
        let out = 1_500_000 - fee;
        let mass_p2 =
            calc_storage_mass(false, std::iter::once(in_cell), std::iter::once(UtxoCell { plurality: 2, amount: out }), storm)
                .unwrap();
        let mass_p1 = calc_storage_mass(
            false,
            std::iter::once(UtxoCell { plurality: 1, amount: 1_500_000 }),
            std::iter::once(UtxoCell { plurality: 1, amount: out }),
            storm,
        )
        .unwrap();
        assert!(
            mass_p1 <= threshold && mass_p2 > threshold,
            "test point must separate p=1 (safe) from p=2 (unsafe): p1={mass_p1} p2={mass_p2}"
        );

        // The guard must follow the node (plurality 2) and re-seed here…
        assert!(
            !chained_tx_storage_mass_safe(&funding, fee, storm, max_tx_mass),
            "must re-seed before the node's plurality-2 storage mass exceeds the ceiling"
        );
        // …and keep chaining off a large fresh seed.
        assert!(
            chained_tx_storage_mass_safe(&entry(1_000_000_000), fee, storm, max_tx_mass),
            "a large confirmed-UTXO seed is storage-mass-safe"
        );
    }

    #[test]
    fn decode_message_roundtrip_and_reject() {
        let bytes = [0xABu8; 32];
        let mut hex = [0u8; 64];
        faster_hex::hex_encode(&bytes, &mut hex).unwrap();
        let decoded = decode_message(std::str::from_utf8(&hex).unwrap()).unwrap();
        assert_eq!(decoded, bytes);
        assert!(decode_message("zz").is_err());
    }

    #[test]
    fn parse_amount_sompi_units() {
        // bare + explicit sompi
        assert_eq!(parse_amount_sompi("1000000000").unwrap(), 1_000_000_000);
        assert_eq!(parse_amount_sompi("1000000000sompi").unwrap(), 1_000_000_000);
        assert_eq!(parse_amount_sompi("0").unwrap(), 0);
        // MSK / KAS (1 coin = 1e8 sompi), incl. fractional + case-insensitive + spaces
        assert_eq!(parse_amount_sompi("10MSK").unwrap(), 10 * 100_000_000);
        assert_eq!(parse_amount_sompi("10kas").unwrap(), 10 * 100_000_000);
        assert_eq!(parse_amount_sompi("10.5MSK").unwrap(), 1_050_000_000);
        assert_eq!(parse_amount_sompi("0.00000001MSK").unwrap(), 1); // 1 sompi
        assert_eq!(parse_amount_sompi(".5MSK").unwrap(), 50_000_000);
        assert_eq!(parse_amount_sompi(" 2 MSK ").unwrap(), 2 * 100_000_000);
    }

    #[test]
    fn port_kind_hint_flags_wrong_services() {
        assert!(port_kind_hint("127.0.0.1:26210").unwrap().contains("gRPC")); // testnet gRPC
        assert!(port_kind_hint("127.0.0.1:28210").unwrap().contains("wRPC JSON"));
        assert!(port_kind_hint("127.0.0.1:8545").unwrap().contains("EVM"));
        assert!(port_kind_hint("127.0.0.1:27210").is_none(), "borsh port has no hint");
        assert!(port_kind_hint("127.0.0.1:27610").is_none(), "devnet borsh port has no hint");
        assert!(port_kind_hint("garbage").is_none());
    }

    #[test]
    fn resolve_node_rpc_explicit_and_fallback() {
        // explicit --node-wrpc-borsh / env wins over the network
        assert_eq!(resolve_node_rpc(&Some("testnet-10".into()), &Some("1.2.3.4:9".into())), "1.2.3.4:9");
        // no network + no explicit → the loopback fallback default
        assert_eq!(resolve_node_rpc(&None, &None), "127.0.0.1:27210");
        // an unparseable network-id with no explicit → fallback (never panics)
        assert_eq!(resolve_node_rpc(&Some("bogus-net".into()), &None), "127.0.0.1:27210");
        // (the network-default and registry branches are covered by misaka_endpoints::resolve tests)
    }

    #[test]
    fn parse_amount_sompi_rejects_junk() {
        assert!(parse_amount_sompi("").is_err());
        assert!(parse_amount_sompi("MSK").is_err());
        assert!(parse_amount_sompi("10.5").is_err(), "fractional sompi is invalid");
        assert!(parse_amount_sompi("10.123456789MSK").is_err(), "more than 8 decimals");
        assert!(parse_amount_sompi("abcMSK").is_err());
        assert!(parse_amount_sompi("99999999999999999999MSK").is_err(), "overflow");
    }
}

#[cfg(test)]
mod eip55_tests {
    use super::eip55_checksum;
    fn bytes(s: &str) -> [u8; 20] {
        let mut a = [0u8; 20];
        faster_hex::hex_decode(s.as_bytes(), &mut a).unwrap();
        a
    }
    #[test]
    fn matches_eip55_spec_vectors() {
        // Canonical vectors from EIP-55.
        assert_eq!(eip55_checksum(&bytes("5aaeb6053f3e94c9b9a09f33669435e7ef1beaed")), "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
        assert_eq!(eip55_checksum(&bytes("fb6916095ca1df60bb79ce92ce3ea74c37c5d359")), "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359");
        assert_eq!(eip55_checksum(&bytes("dbf03b407c01e7cd3cbea99509d93f8dddc8c6fb")), "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB");
        assert_eq!(eip55_checksum(&bytes("d1220a0cf47c7b9be7a2e6ba89f429762e7b9adb")), "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb");
    }
}
