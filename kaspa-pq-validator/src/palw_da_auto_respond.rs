//! Automatic PALW DA-challenge (0x3b) response: discovery + deadline-aware planning + submission.
//!
//! The recovery scheduler inside the node discovers open availability challenges and republishes
//! bytes, but it deliberately holds no provider owner keys and never signs an on-chain 0x3b response
//! (see `docs/palw-da-object-v2-operations.md`). Completing the operational gate needs an **off-node**,
//! owner-key-holding responder that discovers open challenges and answers them before their deadline.
//!
//! Discovery uses the read-only `getPalwState` RPC (extended to return open DA challenges for a
//! provider bond). The pure, security-relevant core is [`plan_responses`]: given the discovered
//! challenges and the current DAA score, it decides which to answer. It never signs; it only decides.
//! Three safety invariants are unit-tested:
//!
//! 1. **Ownership** — only answer a challenge whose provider bond the operator supplied an owner key
//!    for; a challenge on another bond is never signed. (Discovery already filters by the queried bond;
//!    the planner re-checks against the owned set as defense in depth.)
//! 2. **Liveness of the deadline** — never spend an owner signature on a challenge whose deadline has
//!    already passed; that response would be rejected and the bond is already exposed to a timeout
//!    slash. Surface it instead.
//! 3. **Bounded work** — answer the soonest-deadline challenges first and cap submissions per cycle.
//!
//! Signing + submission reuse the shipped `build_signed_da_response` and `palw-submit` primitives and
//! are gated behind an explicit `--enable-auto-response` opt-in; without it the tool only reports the
//! plan.

use clap::Parser;
use kaspa_consensus_core::Hash64;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Pure decision engine
// ---------------------------------------------------------------------------

/// One open-availability challenge discovered from the node, reduced to the fields the planner needs.
/// `bond_key` is the operator-facing canonical bond reference (`"txid:index"`) used for ownership
/// comparison. Discovery only surfaces `Open` challenges, so no status is carried here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaChallengeTarget {
    pub challenge_id: Hash64,
    pub bond_key: String,
    pub object_root: Hash64,
    pub chunk_index: u16,
    pub response_deadline_daa_score: u64,
}

/// Why a discovered challenge was not scheduled for a response this cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The bond is not one the operator supplied an owner key for.
    NotOwned,
    /// The response deadline is at or before the current DAA score.
    DeadlinePassed,
    /// Skipped only because the per-cycle response cap was already reached.
    OverCycleCap,
}

/// A challenge the operator should sign and submit a 0x3b response for, before its deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponsePlan {
    pub challenge_id: Hash64,
    pub bond_key: String,
    pub object_root: Hash64,
    pub chunk_index: u16,
    pub response_deadline_daa_score: u64,
    /// DAA scores remaining before the deadline (`deadline - current`, always > 0 here).
    pub daa_remaining: u64,
    /// The remaining window is at or below the configured safety margin — respond first.
    pub urgent: bool,
}

/// A discovered challenge that was not scheduled, with the reason (for logging/telemetry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedChallenge {
    pub challenge_id: Hash64,
    pub bond_key: String,
    pub reason: SkipReason,
}

/// The full decision for one discovery cycle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResponsePlanSet {
    /// Ordered soonest-deadline first, then by challenge id; capped at `max_responses_per_cycle`.
    pub responses: Vec<ResponsePlan>,
    /// Every challenge that was not scheduled, with its reason.
    pub skipped: Vec<SkippedChallenge>,
}

/// Operator-supplied policy for the responder.
#[derive(Clone, Debug)]
pub struct AutoResponderConfig {
    /// Canonical `"txid:index"` bond references the operator holds owner keys for.
    pub owned_bonds: BTreeSet<String>,
    /// A response is flagged `urgent` when its remaining window is at or below this many DAA.
    pub safety_margin_daa: u64,
    /// Maximum number of responses to schedule in a single discovery cycle.
    pub max_responses_per_cycle: usize,
}

/// Decide which discovered challenges to answer this cycle. Pure and deterministic.
///
/// A challenge is a response candidate only when it is on an owned bond and its deadline is strictly
/// in the future. Candidates are ordered soonest-deadline first (ties broken by challenge id, so the
/// order is stable), then truncated to `max_responses_per_cycle`; the overflow is reported as
/// `OverCycleCap` rather than silently dropped.
pub fn plan_responses(targets: &[DaChallengeTarget], current_daa: u64, config: &AutoResponderConfig) -> ResponsePlanSet {
    let mut candidates: Vec<ResponsePlan> = Vec::new();
    let mut skipped: Vec<SkippedChallenge> = Vec::new();

    for target in targets {
        let skip =
            |reason: SkipReason| SkippedChallenge { challenge_id: target.challenge_id, bond_key: target.bond_key.clone(), reason };
        if !config.owned_bonds.contains(&target.bond_key) {
            skipped.push(skip(SkipReason::NotOwned));
            continue;
        }
        let Some(daa_remaining) = target.response_deadline_daa_score.checked_sub(current_daa).filter(|remaining| *remaining > 0)
        else {
            skipped.push(skip(SkipReason::DeadlinePassed));
            continue;
        };
        candidates.push(ResponsePlan {
            challenge_id: target.challenge_id,
            bond_key: target.bond_key.clone(),
            object_root: target.object_root,
            chunk_index: target.chunk_index,
            response_deadline_daa_score: target.response_deadline_daa_score,
            daa_remaining,
            urgent: daa_remaining <= config.safety_margin_daa,
        });
    }

    // Soonest deadline first; stable tie-break on challenge id so a cycle is reproducible.
    candidates.sort_by(|a, b| {
        a.response_deadline_daa_score
            .cmp(&b.response_deadline_daa_score)
            .then_with(|| a.challenge_id.as_bytes().cmp(&b.challenge_id.as_bytes()))
    });

    if candidates.len() > config.max_responses_per_cycle {
        for overflow in candidates.split_off(config.max_responses_per_cycle) {
            skipped.push(SkippedChallenge {
                challenge_id: overflow.challenge_id,
                bond_key: overflow.bond_key,
                reason: SkipReason::OverCycleCap,
            });
        }
    }

    ResponsePlanSet { responses: candidates, skipped }
}

// ---------------------------------------------------------------------------
// CLI: discover -> plan -> (optionally) sign + submit
// ---------------------------------------------------------------------------

/// `palw-da-auto-respond`: discover open DA challenges on the operator's provider bonds and answer
/// them with an owner-signed 0x3b response before their deadline. Opt-in and off-node.
#[derive(Parser, Debug)]
pub struct PalwDaAutoRespondArgs {
    /// Local node wRPC (borsh) endpoint, host:port. Bind the node's RPC to 127.0.0.1 only.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
    /// Network id (e.g. testnet-110) used to resolve the node endpoint when one is not given.
    #[arg(long)]
    network: Option<String>,
    /// PALW network domain id (u32) stamped into the response.
    #[arg(long)]
    network_id: u32,
    /// Provider-bond owner ML-DSA-87 seed path. This key both signs the 0x3b response and funds the
    /// carrier transaction (the node enforces funder == provider owner).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    owner_key: String,
    /// Provider bond outpoint(s) this operator owns, "txid:index". Repeat per bond.
    #[arg(long = "provider-bond", required = true)]
    provider_bonds: Vec<String>,
    /// Directory holding served canonical object bytes named "<object_root>.palwda" (the DA spool
    /// archive layout).
    #[arg(long)]
    object_dir: PathBuf,
    /// Flag a response urgent when its remaining window is at or below this many DAA.
    #[arg(long, default_value_t = 100)]
    safety_margin_daa: u64,
    /// Maximum responses to submit per discovery cycle.
    #[arg(long, default_value_t = 8)]
    max_per_cycle: usize,
    /// Seconds between discovery cycles. 0 runs a single cycle and exits.
    #[arg(long, default_value_t = 0)]
    interval_seconds: u64,
    /// Actually sign and submit. Without this flag the tool only discovers and prints the plan.
    #[arg(long)]
    enable_auto_response: bool,
    /// Optional explicit fee (sompi) for the carrier transaction.
    #[arg(long)]
    fee: Option<u64>,
    /// Max funding inputs per carrier transaction.
    #[arg(long, default_value_t = 20)]
    max_inputs: usize,
    /// Do not wait for selected-chain inclusion after submit.
    #[arg(long)]
    no_wait: bool,
    /// Seconds to wait for selected-chain inclusion.
    #[arg(long, default_value_t = 120)]
    inclusion_timeout_secs: u64,
}

pub async fn palw_da_auto_respond(args: PalwDaAutoRespondArgs) -> Result<(), String> {
    use kaspa_core::info;

    let owned_bonds: BTreeSet<String> = args.provider_bonds.iter().cloned().collect();
    let node_rpc = crate::resolve_node_rpc(&args.network, &args.node_rpc);
    let config =
        AutoResponderConfig { owned_bonds, safety_margin_daa: args.safety_margin_daa, max_responses_per_cycle: args.max_per_cycle };
    if !args.enable_auto_response {
        info!("[palw-da-auto-respond] plan-only mode; pass --enable-auto-response to sign and submit");
    }
    loop {
        if let Err(error) = run_cycle(&args, &node_rpc, &config).await {
            kaspa_core::warn!("[palw-da-auto-respond] cycle error: {error}");
        }
        if args.interval_seconds == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_secs(args.interval_seconds)).await;
    }
    Ok(())
}

async fn run_cycle(args: &PalwDaAutoRespondArgs, node_rpc: &str, config: &AutoResponderConfig) -> Result<(), String> {
    use kaspa_core::{info, warn};
    use kaspa_rpc_core::{GetPalwStateRequest, api::rpc::RpcApi};

    let client = crate::connect(node_rpc).await?;
    let mut targets: Vec<DaChallengeTarget> = Vec::new();
    let mut current_daa: u64 = 0;
    for bond in &config.owned_bonds {
        let response = client
            .get_palw_state(GetPalwStateRequest {
                batch_id: None,
                provider_bond_outpoint: Some(bond.clone()),
                pcpb_anchor_epoch: None,
                pcpb_a_commit: None,
            })
            .await
            .map_err(|error| format!("getPalwState({bond}) failed: {error}"))?;
        current_daa = current_daa.max(response.sink_daa_score);
        for challenge in response.da_challenges {
            let challenge_id = challenge
                .challenge_id
                .parse::<Hash64>()
                .map_err(|_| format!("node returned a malformed challenge id {}", challenge.challenge_id))?;
            let object_root = challenge
                .object_root
                .parse::<Hash64>()
                .map_err(|_| format!("node returned a malformed object root {}", challenge.object_root))?;
            targets.push(DaChallengeTarget {
                challenge_id,
                bond_key: challenge.provider_bond,
                object_root,
                chunk_index: challenge.chunk_index,
                response_deadline_daa_score: challenge.response_deadline_daa_score,
            });
        }
    }

    let plan = plan_responses(&targets, current_daa, config);
    for skipped in &plan.skipped {
        info!("[palw-da-auto-respond] skip challenge={} bond={} reason={:?}", skipped.challenge_id, skipped.bond_key, skipped.reason);
    }
    if plan.responses.is_empty() {
        info!("[palw-da-auto-respond] no responses due at daa {current_daa}");
        return Ok(());
    }
    for response in &plan.responses {
        info!(
            "[palw-da-auto-respond] due challenge={} bond={} chunk={} deadline={} remaining={} urgent={}",
            response.challenge_id,
            response.bond_key,
            response.chunk_index,
            response.response_deadline_daa_score,
            response.daa_remaining,
            response.urgent
        );
        if !args.enable_auto_response {
            continue;
        }
        match submit_response(args, node_rpc, response).await {
            Ok(()) => info!("[palw-da-auto-respond] answered challenge={}", response.challenge_id),
            Err(error) => warn!("[palw-da-auto-respond] failed to answer challenge={}: {error}", response.challenge_id),
        }
    }
    Ok(())
}

async fn submit_response(args: &PalwDaAutoRespondArgs, node_rpc: &str, plan: &ResponsePlan) -> Result<(), String> {
    use misaka_palw_miner::da::{build_signed_da_response, encode_da_response};

    let provider_bond = kaspa_pq_validator_core::parse_stake_bond_ref(&plan.bond_key)?;
    // The served canonical object bytes, named by their content root (the DA spool archive layout).
    let object_path = args.object_dir.join(format!("{}.palwda", plan.object_root));
    let object_bytes =
        std::fs::read(&object_path).map_err(|error| format!("cannot read served object {}: {error}", object_path.display()))?;
    let owner_key = load_owner_key(&args.owner_key)?;
    let response =
        build_signed_da_response(args.network_id, plan.challenge_id, provider_bond, &owner_key, &object_bytes, plan.chunk_index)
            .map_err(|error| error.to_string())?;
    let (_subnetwork, payload) = encode_da_response(&response).map_err(|error| error.to_string())?;

    let submit_args = crate::palw_submit::GeneratedPalwSubmitArgs {
        node_rpc: Some(node_rpc.to_string()),
        validator_key: args.owner_key.clone(),
        network: args.network.clone(),
        fee: args.fee,
        max_inputs: args.max_inputs,
        // Never spend the provider bond outpoint itself as fee funding.
        exclude_funding_outpoint: vec![plan.bond_key.clone()],
        no_wait: args.no_wait,
        inclusion_timeout_secs: args.inclusion_timeout_secs,
        dry_run: false,
    };
    crate::palw_submit::palw_submit_generated(submit_args, crate::palw_submit::PalwSubmitKind::DaResponse, payload).await
}

fn load_owner_key(path: &str) -> Result<kaspa_pq_validator_core::ValidatorKey, String> {
    let mut seed = kaspa_pq_validator_core::load_validator_seed(path)?;
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn target(id: u8, bond: &str, deadline: u64) -> DaChallengeTarget {
        DaChallengeTarget {
            challenge_id: h(id),
            bond_key: bond.to_string(),
            object_root: h(id.wrapping_add(100)),
            chunk_index: id as u16,
            response_deadline_daa_score: deadline,
        }
    }

    fn config(owned: &[&str], margin: u64, cap: usize) -> AutoResponderConfig {
        AutoResponderConfig {
            owned_bonds: owned.iter().map(|s| s.to_string()).collect(),
            safety_margin_daa: margin,
            max_responses_per_cycle: cap,
        }
    }

    #[test]
    fn only_owned_future_challenges_are_scheduled() {
        let targets = vec![
            target(1, "mine:0", 200),   // scheduled
            target(2, "theirs:0", 200), // NotOwned
            target(4, "mine:0", 100),   // DeadlinePassed (== current)
            target(5, "mine:0", 90),    // DeadlinePassed (< current)
        ];
        let plan = plan_responses(&targets, 100, &config(&["mine:0"], 10, 8));
        assert_eq!(plan.responses.iter().map(|r| r.challenge_id).collect::<Vec<_>>(), vec![h(1)]);
        let reasons: Vec<_> = plan.skipped.iter().map(|s| (s.challenge_id, s.reason)).collect();
        assert!(reasons.contains(&(h(2), SkipReason::NotOwned)));
        assert!(reasons.contains(&(h(4), SkipReason::DeadlinePassed)));
        assert!(reasons.contains(&(h(5), SkipReason::DeadlinePassed)));
    }

    #[test]
    fn responses_are_ordered_soonest_deadline_first() {
        let targets = vec![target(1, "mine:0", 300), target(2, "mine:0", 150), target(3, "mine:0", 220)];
        let plan = plan_responses(&targets, 100, &config(&["mine:0"], 10, 8));
        assert_eq!(plan.responses.iter().map(|r| r.response_deadline_daa_score).collect::<Vec<_>>(), vec![150, 220, 300]);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn per_cycle_cap_defers_the_latest_deadlines_as_over_cap() {
        let targets = vec![target(1, "mine:0", 300), target(2, "mine:0", 150), target(3, "mine:0", 220)];
        let plan = plan_responses(&targets, 100, &config(&["mine:0"], 10, 2));
        // The two soonest are scheduled; the latest deadline is deferred, not dropped silently.
        assert_eq!(plan.responses.iter().map(|r| r.response_deadline_daa_score).collect::<Vec<_>>(), vec![150, 220]);
        assert_eq!(
            plan.skipped,
            vec![SkippedChallenge { challenge_id: h(1), bond_key: "mine:0".into(), reason: SkipReason::OverCycleCap }]
        );
    }

    #[test]
    fn urgent_flag_tracks_the_safety_margin() {
        let targets = vec![
            target(1, "mine:0", 105), // remaining 5 <= margin 10 => urgent
            target(2, "mine:0", 130), // remaining 30 > margin 10 => not urgent
        ];
        let plan = plan_responses(&targets, 100, &config(&["mine:0"], 10, 8));
        let urgent: std::collections::BTreeMap<_, _> = plan.responses.iter().map(|r| (r.challenge_id, r.urgent)).collect();
        assert!(urgent[&h(1)]);
        assert!(!urgent[&h(2)]);
        assert_eq!(plan.responses[0].daa_remaining, 5);
    }

    #[test]
    fn ties_break_deterministically_on_challenge_id() {
        // Same deadline, submitted in reverse id order; the plan is still id-sorted.
        let targets = vec![target(9, "mine:0", 200), target(3, "mine:0", 200), target(7, "mine:0", 200)];
        let plan = plan_responses(&targets, 100, &config(&["mine:0"], 10, 8));
        assert_eq!(plan.responses.iter().map(|r| r.challenge_id).collect::<Vec<_>>(), vec![h(3), h(7), h(9)]);
    }
}
