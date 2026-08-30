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
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
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
/// **`misaka miner` no longer forwards** (ADR-0063 D4).
///
/// It forwarded to `kaspa-pq-miner`, which is not installed on the fleet hosts this was measured
/// on — and on a `ConsensusV2` network it could not have worked if it were: every block's proof of
/// work is a deterministic LLM inference under a registered bond, so a hash miner has no lane to
/// mine. A command that exists and cannot run teaches an operator that the tool is unreliable, in
/// the one place they most need to trust it, so it says what to run instead.
///
/// `MISAKA_MINER_BIN` still forces the old behaviour, and so does **any network that is not a PALW
/// network** — which is the important half, because it includes MAINNET.
///
/// The refusal is about the CONSENSUS, not about the command. Mainnet ships
/// `PalwConsensusMode::Disabled` and `pow_palw_activation: never()`: it is hash-only proof of work,
/// a hash miner is exactly the right tool, and the first version of this refusal told every mainnet
/// operator the opposite — that their blocks are LLM inferences and they should go register a bond.
/// Wrong advice in the one place an operator most needs the tool to be right, which is the same
/// sentence this doc uses to justify refusing at all. So ask the network the question instead of
/// assuming the answer: only a `ConsensusV2` network has no hash lane.
/// Does THIS network's consensus make blocks out of inference rather than hashes?
///
/// A network id this tree cannot parse is not one we can make a claim about, so it answers `false`
/// and the caller forwards — refusing on a parse failure would turn a typo into "your consensus has
/// no miner".
fn palw_is_the_consensus(network: &str) -> bool {
    NetworkId::from_str(network)
        .map(|nid| matches!(Params::from(nid).palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)))
        .unwrap_or(false)
}

pub fn miner(ctx: &Ctx, args: &[String]) -> CliResult {
    if std::env::var_os("MISAKA_MINER_BIN").is_some() {
        let injected = miner_injection(&ctx.network, &ctx.node_grpc, args);
        return exec("kaspa-pq-miner", "MISAKA_MINER_BIN", &[], &injected, args);
    }
    if !palw_is_the_consensus(&ctx.network) {
        let injected = miner_injection(&ctx.network, &ctx.node_grpc, args);
        return exec("kaspa-pq-miner", "MISAKA_MINER_BIN", &[], &injected, args);
    }
    Err(CliError::new(
        exit::GENERIC,
        format!(
            "there is no hash miner for {}: on this network every block's proof of work is a deterministic LLM inference made under a registered bond, so mining means running a producer, not a miner.\n\n  \
             1. `misaka key gen --out /etc/misaka/bond.key`   (or `key import` for an existing seed)\n  \
             2. fund that address, then register a bond with `kaspad --palw-register-bond`\n  \
             3. run the node with `--palw-produce` (see docs/testnet11-join-mining.md)\n\n\
             `misaka bond status` shows the bond once it is registered. If you do have a hash miner binary you want used anyway, set MISAKA_MINER_BIN to its path and this command forwards to it.",
            ctx.network
        ),
    ))
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

#[cfg(test)]
mod miner_refusal_tests {
    use super::palw_is_the_consensus;

    /// **`misaka miner` must not tell a mainnet operator their blocks are LLM inferences.**
    ///
    /// The refusal shipped unconditional, so every network got the PALW answer — including the
    /// hash-only ones, where a hash miner is exactly the right tool and the instructions it printed
    /// (generate a key, register a bond, run a producer) do not apply. Both positions, because a
    /// predicate that answered `true` everywhere would also pass an assertion that only checked the
    /// networks it is right about.
    #[test]
    fn only_a_palw_network_has_no_hash_miner() {
        assert!(!palw_is_the_consensus("mainnet"), "mainnet is hash-only PoW — the miner must still forward");
        assert!(!palw_is_the_consensus("testnet-10"), "a hash-only testnet forwards too");
        assert!(palw_is_the_consensus("testnet-11"), "testnet-11 IS a ConsensusV2 network, and there the refusal is the truth");
        assert!(!palw_is_the_consensus("not-a-network"), "an unparsable id is not a claim about consensus — forward, do not lecture");
    }
}
