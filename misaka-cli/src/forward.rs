//! `misaka validator …` / `misaka miner …` — thin shell-out front-ends over the
//! existing `kaspa-pq-validator` / `kaspa-pq-miner` binaries (design §6, option A).
//!
//! The unified CLI does NOT re-implement bond / attestation / ML-DSA key handling; it
//! forwards the user's args verbatim and injects the global context. The validator's
//! flags are PER-SUBCOMMAND (e.g. `keygen --network-id`), so a top-level flag cannot
//! be prepended — instead the context flows through the validator's own env vars
//! (`KASPA_PQ_NETWORK`, `KASPA_PQ_NODE_RPC`), which an explicit flag still overrides.
//! The miner is a flat command, so its `--network-id` / optional `--node-grpc` are
//! injected as leading flags.
//! In both cases an operator-exported env var / explicit flag wins, the child inherits
//! stdio, and its exact exit code is propagated.

use crate::node::Ctx;
use crate::{CliError, CliResult, exit};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use std::path::PathBuf;
use std::str::FromStr;

/// True if `args` already carries one of `names` (either `--flag` or `--flag=value`),
/// so the corresponding default is not injected twice (clap rejects duplicates).
fn has_flag(args: &[String], names: &[&str]) -> bool {
    args.iter().any(|a| names.iter().any(|n| a == n || a.starts_with(&format!("{n}="))))
}

/// Env defaults to hand the validator (it reads `--network-id`/`--node-wrpc-borsh` from
/// these). Always carries the network; the Borsh endpoint only when `misaka` has one.
/// `exec` skips any that the operator already exported.
fn validator_envs(network: &str, rpc: &Option<String>) -> Vec<(&'static str, String)> {
    let mut envs = vec![("KASPA_PQ_NETWORK", network.to_string())];
    if let Some(rpc) = rpc {
        envs.push(("KASPA_PQ_NODE_RPC", rpc.clone()));
    }
    envs
}

/// The miner is a flat command, so inject `--network-id` and, when explicitly configured,
/// `--node-grpc` as leading flags unless the user already passed either. Leaving gRPC unset lets
/// the miner use its own env/endpoint-registry/network-default resolver.
fn miner_injection(network: &str, node_grpc: &Option<String>, args: &[String]) -> Vec<String> {
    let mut injected = Vec::new();
    if !has_flag(args, &["--network-id", "--network"]) {
        injected.extend(["--network-id".to_string(), network.to_string()]);
    }
    if let Some(node_grpc) = node_grpc
        && !has_flag(args, &["--node-grpc", "--rpc"])
    {
        injected.extend(["--node-grpc".to_string(), node_grpc.clone()]);
    }
    injected
}

/// Resolve the target binary: explicit `env_override` → a sibling next to the running
/// `misaka` (the common install layout) → the bare name on `$PATH`.
fn resolve(bin: &str, env_override: &str) -> PathBuf {
    if let Ok(p) = std::env::var(env_override)
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    if let Ok(cur) = std::env::current_exe()
        && let Some(sib) = cur.parent().map(|d| d.join(bin))
        && sib.is_file()
    {
        return sib;
    }
    PathBuf::from(bin) // let the OS resolve via $PATH
}

/// Exec the target binary with `env_defaults` (each set only when the operator did not
/// already export it) and `injected_args` ahead of `user_args`, inheriting stdio, then
/// propagate its exact exit code (never returns on success).
fn exec(bin: &str, env_override: &str, env_defaults: &[(&str, String)], injected_args: &[String], user_args: &[String]) -> CliResult {
    let exe = resolve(bin, env_override);
    let mut cmd = std::process::Command::new(&exe);
    for (k, v) in env_defaults {
        if std::env::var_os(k).is_none() {
            cmd.env(k, v);
        }
    }
    let status = cmd.args(injected_args).args(user_args).status().map_err(|e| {
        CliError::new(
            exit::GENERIC,
            format!(
                "failed to launch {bin} ({}): {e}; install it next to `misaka`, put it on $PATH, or set {env_override}=<path>",
                exe.display()
            ),
        )
    })?;
    std::process::exit(status.code().unwrap_or(1));
}

/// `misaka validator …` → `kaspa-pq-validator …` (context via env; explicit flags win).
pub fn validator(ctx: &Ctx, args: &[String]) -> CliResult {
    let envs = validator_envs(&ctx.network, &ctx.rpc);
    exec("kaspa-pq-validator", "MISAKA_VALIDATOR_BIN", &envs, &[], args)
}

/// `misaka miner …` → `kaspa-pq-miner [--network-id …] …`.
pub fn miner(ctx: &Ctx, args: &[String]) -> CliResult {
    let injected = miner_injection(&ctx.network, &ctx.node_grpc, args);
    exec("kaspa-pq-miner", "MISAKA_MINER_BIN", &[], &injected, args)
}

/// Map a network-id to kaspad's network-selection flags. Port-free: kaspad derives every
/// port from the network, so the operator never types one. Mainnet selects no flag (the
/// default); testnet adds `--netsuffix=N` when the id carries a suffix.
fn kaspad_net_flags(network: &str) -> Result<Vec<String>, CliError> {
    let nid =
        NetworkId::from_str(network).map_err(|e| CliError::new(exit::GENERIC, format!("invalid network-id '{network}': {e}")))?;
    Ok(match nid.network_type {
        NetworkType::Mainnet => vec![],
        NetworkType::Testnet => {
            let mut v = vec!["--testnet".to_string()];
            if let Some(s) = nid.suffix {
                v.push(format!("--netsuffix={s}"));
            }
            v
        }
        NetworkType::Devnet => vec!["--devnet".to_string()],
        NetworkType::Simnet => vec!["--simnet".to_string()],
    })
}

/// Compute kaspad's injected flags for a port-free node launch: the network-selection flags,
/// optional RPC profile, and optional operator node profile/resource guard flags. Injections are
/// skipped when the operator already passed the corresponding kaspad flag in trailing args.
fn node_injection(
    network: &str,
    profile: Option<&str>,
    node_profile: Option<&str>,
    vps_8gb: bool,
    min_disk_free_percent: Option<u8>,
    args: &[String],
) -> Result<Vec<String>, CliError> {
    let mut injected = Vec::new();
    if !has_flag(args, &["--testnet", "--devnet", "--simnet"]) {
        injected.extend(kaspad_net_flags(network)?);
    }
    if let Some(p) = profile
        && !has_flag(args, &["--profile"])
    {
        injected.push(format!("--profile={p}"));
    }
    if let Some(p) = node_profile
        && !has_flag(args, &["--node-profile"])
    {
        injected.push(format!("--node-profile={p}"));
    }
    if vps_8gb && !has_flag(args, &["--vps-8gb"]) {
        injected.push("--vps-8gb".to_string());
    }
    if let Some(percent) = min_disk_free_percent
        && !has_flag(args, &["--min-disk-free-percent"])
    {
        injected.push(format!("--min-disk-free-percent={percent}"));
    }
    Ok(injected)
}

/// `misaka node start` / `misaka join` → `kaspad <net-flags> [--profile=…] <user args>`.
/// `announce` prints a one-line "joining …" banner (the `join` front-end) naming the DNS seeds
/// that will be used for peer discovery, so a newcomer sees the bootstrap path before kaspad's
/// own startup summary. The child inherits stdio and its exit code is propagated.
pub fn node(
    ctx: &Ctx,
    profile: Option<&str>,
    node_profile: Option<&str>,
    vps_8gb: bool,
    min_disk_free_percent: Option<u8>,
    args: &[String],
    announce: bool,
) -> CliResult {
    let injected = node_injection(&ctx.network, profile, node_profile, vps_8gb, min_disk_free_percent, args)?;
    if announce {
        // Best-effort: never block the launch on a bad id (node_injection already validated it).
        if let Ok(nid) = NetworkId::from_str(&ctx.network) {
            let seeds = Params::from(nid).dns_seeders;
            if seeds.is_empty() {
                eprintln!("Joining {} — peer discovery via --addpeer/--connect only (no DNS seeds configured)", ctx.network);
            } else {
                eprintln!("Joining {} — discovering peers via {} DNS seed(s): {}", ctx.network, seeds.len(), seeds.join(", "));
            }
        }
    }
    exec("kaspad", "MISAKA_KASPAD_BIN", &[], &injected, args)
}

/// `misaka node db-stats` → `kaspad <net-flags> --db-stats[=count-rows]`.
///
/// Forwarded rather than reimplemented: reading the report needs RocksDB, and
/// linking it into `misaka` would put a C++ build into the lightweight operator
/// CLI for one diagnostic. `kaspad` already has it, is already installed next to
/// the database, and opens it read-only.
pub fn node_db_stats(ctx: &Ctx, count_rows: bool, args: &[String]) -> CliResult {
    let mut injected = node_injection(&ctx.network, None, None, false, None, args)?;
    if !has_flag(args, &["--db-stats"]) {
        injected.push(if count_rows { "--db-stats=count-rows".to_string() } else { "--db-stats=estimate".to_string() });
    }
    exec("kaspad", "MISAKA_KASPAD_BIN", &[], &injected, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn validator_envs_carry_network_and_borsh() {
        let e = validator_envs("testnet-10", &Some("127.0.0.1:27210".to_string()));
        assert_eq!(e, vec![("KASPA_PQ_NETWORK", "testnet-10".to_string()), ("KASPA_PQ_NODE_RPC", "127.0.0.1:27210".to_string())]);
    }

    #[test]
    fn validator_envs_skip_borsh_when_rpc_unset() {
        assert_eq!(validator_envs("simnet", &None), vec![("KASPA_PQ_NETWORK", "simnet".to_string())]);
    }

    #[test]
    fn miner_injects_network_unless_present() {
        assert_eq!(miner_injection("testnet-10", &None, &s(&["--blocks", "0"])), s(&["--network-id", "testnet-10"]));
        assert!(miner_injection("testnet-10", &None, &s(&["--network-id=devnet"])).is_empty());
        assert!(miner_injection("testnet-10", &None, &s(&["--network", "devnet"])).is_empty());
    }

    #[test]
    fn miner_injects_node_grpc_when_configured() {
        assert_eq!(
            miner_injection("testnet-10", &Some("127.0.0.1:26210".to_string()), &s(&["--blocks", "0"])),
            s(&["--network-id", "testnet-10", "--node-grpc", "127.0.0.1:26210"])
        );
        assert_eq!(
            miner_injection("testnet-10", &Some("127.0.0.1:26210".to_string()), &s(&["--rpc", "127.0.0.1:9999"])),
            s(&["--network-id", "testnet-10"])
        );
    }

    #[test]
    fn has_flag_matches_both_forms() {
        assert!(has_flag(&s(&["--network-id=devnet"]), &["--network-id"]));
        assert!(has_flag(&s(&["--network-id", "devnet"]), &["--network-id"]));
        assert!(!has_flag(&s(&["--blocks", "0"]), &["--network-id", "--network"]));
    }

    #[test]
    fn kaspad_net_flags_per_network() {
        assert_eq!(kaspad_net_flags("mainnet").unwrap(), Vec::<String>::new());
        assert_eq!(kaspad_net_flags("testnet-10").unwrap(), s(&["--testnet", "--netsuffix=10"]));
        assert_eq!(kaspad_net_flags("devnet").unwrap(), s(&["--devnet"]));
        assert_eq!(kaspad_net_flags("simnet").unwrap(), s(&["--simnet"]));
        assert!(kaspad_net_flags("not-a-net").is_err());
    }

    #[test]
    fn node_injection_net_and_profile() {
        // bare launch: derive net flags + the require-equals profile form
        assert_eq!(
            node_injection("testnet-10", Some("local-validator"), None, false, None, &[]).unwrap(),
            s(&["--testnet", "--netsuffix=10", "--profile=local-validator"])
        );
        // no profile requested → only net flags
        assert_eq!(node_injection("devnet", None, None, false, None, &[]).unwrap(), s(&["--devnet"]));
    }

    #[test]
    fn node_injection_adds_operator_profile_flags() {
        assert_eq!(
            node_injection("mainnet", None, Some("bootstrap-pruned"), true, Some(12), &[]).unwrap(),
            s(&["--node-profile=bootstrap-pruned", "--vps-8gb", "--min-disk-free-percent=12"])
        );
    }

    #[test]
    fn node_injection_respects_operator_overrides() {
        // operator chose a net → inject NO net flags (avoid kaspad's "only a single net" panic)
        assert_eq!(
            node_injection("testnet-10", Some("minimal"), None, false, None, &s(&["--devnet"])).unwrap(),
            s(&["--profile=minimal"])
        );
        // operator passed their own --profile → don't inject ours
        assert!(node_injection("mainnet", Some("local-full"), None, false, None, &s(&["--profile=minimal"])).unwrap().is_empty());
        assert!(
            node_injection(
                "mainnet",
                None,
                Some("bootstrap-pruned"),
                true,
                Some(15),
                &s(&["--node-profile=archive", "--vps-8gb", "--min-disk-free-percent=5"]),
            )
            .unwrap()
            .is_empty()
        );
    }
}
