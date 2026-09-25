//! **A node's protocol duties are on by construction** (user decision, 2026-09-25).
//!
//! What a node MUST do to take part in the protocol is not an operator switch, and has no off switch.
//! `--palw-panel` and `--palw-round-lane` were each "default off": a seat whose unit forgot the flag
//! held its bond, drew its duties and answered none of them, and a producer whose unit forgot the
//! round lane let every permit the chain's schedule granted its bond go unused. Nothing on the chain
//! can tell the difference between that and a node that is down.
//!
//! **The rule.** A duty runs whenever
//! 1. the network's params configure it (read through the `Params` accessors — never a list of
//!    networks), and
//! 2. the node holds the identity the duty acts as (a bond's key and outpoint).
//!
//! Each service then asks the params at the CURRENT DAA inside its own loop: the round producer reads
//! the lane's status per round and does nothing before the fence fires, and every panel duty is gated
//! by its own fence (`palw_rcore_plus_active_at`, `palw_held_context_active_at`, …) and, for what it
//! carries, by `--palw-fee-outpoint`. A node without the identity starts nothing and says so in ONE
//! INFO line ([`palw_duty_summary_line_v1`]); it never panics.
//!
//! **What stays a command-line choice:** identities and resources (keys, bond, pay / heartbeat
//! addresses, the fee outpoint, artifact paths, memory share and budget, cache sizes), whether to
//! PRODUCE (`--palw-produce`, `--palw-producer-class`, `--palw-canonical-claims` — producing is a
//! business choice, seat duties are not), whether to run the heartbeat miner (it needs an address),
//! the voluntary watchdog (`--palw-challenge`, which stakes the bond on every dispute it opens), and
//! every `*-devnet` / `--palw-drill-*` flag.
//!
//! **The old flags stay accepted** so existing units, kits and scripts keep starting, and each one
//! the operator names logs one WARN line ([`palw_deprecated_duty_flag_warnings_v1`]). Nothing reads
//! them to decide whether a duty runs.

use crate::args::{Args, palw_chain_classes_default};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// **Why a duty is not running on this node** — a fact about the network or about the node's
/// identity, never an operator's switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwDutyIdleV1 {
    /// The network's params configure no such duty.
    NotOnThisNetwork(&'static str),
    /// The node holds no identity the duty could act as.
    NoIdentity(&'static str),
}

impl PalwDutyIdleV1 {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::NotOnThisNetwork(why) | Self::NoIdentity(why) => why,
        }
    }
}

/// The seat identity the panel acts as. `bond` is `None` only for a node registering its first bond
/// (`--palw-register-bond`), which the panel service dispatches to its registration worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatIdentityV1 {
    pub key_path: String,
    pub bond: Option<String>,
}

/// The bond the execution lane's round blocks are signed for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondIdentityV1 {
    pub key_path: String,
    pub bond: String,
}

/// **What this node's duties are, decided once at startup from the params and the identity alone.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDutyPlanV1 {
    /// ADR-0042 Decision 7: the panel's seat duties (and everything R-core+ added inside the panel:
    /// SEAT-R replays, readiness proofs, DA answers, the automatic filers, the carriers).
    pub panel: Result<PalwSeatIdentityV1, PalwDutyIdleV1>,
    /// ADR-0125: the execution lane's round blocks.
    pub round_lane: Result<PalwBondIdentityV1, PalwDutyIdleV1>,
    /// ADR-0067: the chain-registered-class arm ([`Args::palw_chain_classes_for`]).
    pub chain_classes: bool,
}

pub fn palw_duty_plan_v1(args: &Args, params: &Params) -> PalwDutyPlanV1 {
    let v2 = matches!(params.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_));
    let panel = match (v2, &args.palw_producer_key, &args.palw_producer_bond) {
        (false, _, _) => {
            Err(PalwDutyIdleV1::NotOnThisNetwork("this network declares no ConsensusV2 ruleset, so no panels exist here"))
        }
        (true, None, _) => Err(PalwDutyIdleV1::NoIdentity("this node holds no seat identity (no --palw-producer-key)")),
        (true, Some(key_path), Some(bond)) => Ok(PalwSeatIdentityV1 { key_path: key_path.clone(), bond: Some(bond.clone()) }),
        (true, Some(key_path), None) if args.palw_register_bond => Ok(PalwSeatIdentityV1 { key_path: key_path.clone(), bond: None }),
        (true, Some(_), None) => {
            Err(PalwDutyIdleV1::NoIdentity("this node holds a key but no bond (no --palw-producer-bond), so it holds no seat"))
        }
    };
    // `palw_execution_lane_fence` folds in the ConsensusV2 condition: no V2 bundle, no lane.
    let round_lane = match (params.palw_execution_lane_fence(), &args.palw_producer_key, &args.palw_producer_bond) {
        (None, _, _) => {
            Err(PalwDutyIdleV1::NotOnThisNetwork("this network's params configure no execution lane (`palw_execution_lane`)"))
        }
        (Some(_), Some(key_path), Some(bond)) => Ok(PalwBondIdentityV1 { key_path: key_path.clone(), bond: bond.clone() }),
        (Some(_), None, _) => Err(PalwDutyIdleV1::NoIdentity("this node holds no bond key (no --palw-producer-key)")),
        (Some(_), Some(_), None) => Err(PalwDutyIdleV1::NoIdentity("this node holds no bond (no --palw-producer-bond)")),
    };
    PalwDutyPlanV1 { panel, round_lane, chain_classes: args.palw_chain_classes_for(params) }
}

/// **The one INFO line a node prints about its duties** — which run, and for each that does not, why.
pub fn palw_duty_summary_line_v1(plan: &PalwDutyPlanV1, params: &Params) -> String {
    let panel = match &plan.panel {
        Ok(PalwSeatIdentityV1 { bond: Some(bond), .. }) => format!("panel seat duties ON as bond {bond}"),
        Ok(PalwSeatIdentityV1 { bond: None, .. }) => "panel ON to register this node's first bond (--palw-register-bond)".to_string(),
        Err(idle) => format!("panel idle — {}", idle.reason()),
    };
    let round_lane = match &plan.round_lane {
        Ok(PalwBondIdentityV1 { bond, .. }) => format!("execution-lane round blocks ON as bond {bond}"),
        Err(idle) => format!("execution lane idle — {}", idle.reason()),
    };
    let chain_classes = match (plan.chain_classes, palw_chain_classes_default(params)) {
        (true, true) => "chain-registered classes armed (this network's params: the model registry is in force from genesis)",
        (true, false) => "chain-registered classes armed (operator opt-in: --palw-chain-classes; this network's params leave it off)",
        (false, _) => "chain-registered classes off (this network's fence; --palw-chain-classes opts in)",
    };
    format!("PALW duties (on by construction; no flag turns one off): {panel} | {round_lane} | {chain_classes}")
}

/// **One WARN line for each deprecated duty flag the operator named** (command line, environment or
/// config file). Named with `false` it says so: the value is ignored, it does not turn the duty off.
pub fn palw_deprecated_duty_flag_warnings_v1(args: &Args, params: &Params) -> Vec<String> {
    let mut lines = Vec::new();
    let ignored = |value: bool| if value { "" } else { " — its value false is ignored: it cannot turn the duty off" };
    if let Some(value) = args.palw_panel {
        lines.push(format!(
            "--palw-panel is deprecated and does nothing{}: the panel's seat duties are always on, on every node of a ConsensusV2 \
             network that holds a seat identity (--palw-producer-key and --palw-producer-bond). Remove it from the unit.",
            ignored(value)
        ));
    }
    if let Some(value) = args.palw_round_lane {
        lines.push(format!(
            "--palw-round-lane is deprecated and does nothing{}: the execution lane's round blocks are always on, on every node \
             that holds --palw-producer-key and --palw-producer-bond where the network's params configure `palw_execution_lane`. \
             Remove it from the unit.",
            ignored(value)
        ));
    }
    if let Some(value) = args.palw_chain_classes
        && palw_chain_classes_default(params)
    {
        lines.push(format!(
            "--palw-chain-classes is deprecated on {} and does nothing{}: this network's params put the model registry in force \
             from genesis, so the chain-registered-class arm is always armed here. Remove it from the unit.",
            params.net,
            ignored(value)
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};

    fn parse(extra: &[&str]) -> Args {
        let mut argv = vec!["kaspad"];
        argv.extend_from_slice(extra);
        Args::parse(argv).expect("args parse")
    }

    fn params_of(args: &Args) -> Params {
        args.network().into()
    }

    const T12: [&str; 2] = ["--testnet", "--netsuffix=12"];
    const KEY: &str = "--palw-producer-key=/nonexistent/bond.key";
    const BOND: &str = "--palw-producer-bond=aa:0";

    fn t12(extra: &[&str]) -> Args {
        let mut argv = T12.to_vec();
        argv.extend_from_slice(extra);
        parse(&argv)
    }

    /// **testnet-12 configures both duties** — the premise every test below leans on, read from the
    /// params rather than assumed.
    #[test]
    fn testnet_12_configures_the_panel_and_the_execution_lane() {
        let params = params_of(&t12(&[]));
        assert!(matches!(params.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
        assert!(params.palw_execution_lane_fence().is_some(), "testnet-12 opens the execution lane");
        assert!(palw_chain_classes_default(&params), "testnet-12's registry is in force from genesis");
    }

    /// **A node with a bond runs both duties with NO duty flag at all.**
    #[test]
    fn a_bonded_node_runs_every_duty_without_a_flag() {
        let args = t12(&[KEY, BOND]);
        assert_eq!((args.palw_panel, args.palw_round_lane), (None, None), "no duty flag was given");
        let plan = palw_duty_plan_v1(&args, &params_of(&args));
        assert_eq!(plan.panel, Ok(PalwSeatIdentityV1 { key_path: "/nonexistent/bond.key".into(), bond: Some("aa:0".into()) }));
        assert_eq!(plan.round_lane, Ok(PalwBondIdentityV1 { key_path: "/nonexistent/bond.key".into(), bond: "aa:0".into() }));
        assert!(plan.chain_classes);
        assert!(palw_deprecated_duty_flag_warnings_v1(&args, &params_of(&args)).is_empty(), "nothing named, nothing to warn");
    }

    /// **No flag, value or environment variable turns a duty off.** Every form of every old flag
    /// parses — so existing units keep starting — and the plan is the one a node without them gets.
    #[test]
    fn no_flag_can_turn_a_duty_off() {
        let baseline = t12(&[KEY, BOND]);
        let want = palw_duty_plan_v1(&baseline, &params_of(&baseline));
        for extra in [
            vec!["--palw-panel"],
            vec!["--palw-round-lane"],
            vec!["--palw-chain-classes"],
            vec!["--palw-chain-classes=true"],
            vec!["--palw-chain-classes=false"],
            vec!["--palw-panel", "--palw-round-lane", "--palw-chain-classes=false"],
        ] {
            let mut argv = vec![KEY, BOND];
            argv.extend_from_slice(&extra);
            let args = t12(&argv);
            assert_eq!(palw_duty_plan_v1(&args, &params_of(&args)), want, "{extra:?} changed a duty");
        }
    }

    /// **A named `false` — from a config file here, from `KASPAD_PALW_PANEL=false` in a unit — is
    /// recorded, warned about, and turns nothing off.** The config file is the path a test can take
    /// without touching the process environment (a `set_var` in a test races its neighbours); the
    /// environment twin reaches the same `Option<bool>` through `arg_match_named_flag`'s
    /// `ValueSource::EnvVariable` arm, and its existence and plain-flag action are pinned here.
    #[test]
    fn a_named_false_is_recorded_warned_and_ignored() {
        for (id, env) in [("palw-panel", "KASPAD_PALW_PANEL"), ("palw-round-lane", "KASPAD_PALW_ROUND_LANE")] {
            let arg = crate::args::cli().get_arguments().find(|a| a.get_id() == id).cloned().expect("the flag still parses");
            assert!(matches!(arg.get_action(), clap::ArgAction::SetTrue), "{id} stays a plain flag");
            assert_eq!(arg.get_env().map(|e| e.to_string_lossy().into_owned()).as_deref(), Some(env), "{id}'s environment twin");
        }
        let dir = std::env::temp_dir().join(format!("palw-duties-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("kaspad.toml");
        std::fs::write(&file, "palw-panel = false\npalw-round-lane = false\npalw-chain-classes = false\n").unwrap();
        let config = format!("--configfile={}", file.display());
        let named_false = t12(&[KEY, BOND, &config]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            (named_false.palw_panel, named_false.palw_round_lane, named_false.palw_chain_classes),
            (Some(false), Some(false), Some(false))
        );
        let baseline = t12(&[KEY, BOND]);
        assert_eq!(palw_duty_plan_v1(&named_false, &params_of(&named_false)), palw_duty_plan_v1(&baseline, &params_of(&baseline)));
        let warnings = palw_deprecated_duty_flag_warnings_v1(&named_false, &params_of(&named_false));
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert!(warnings.iter().all(|w| w.contains("its value false is ignored")), "{warnings:?}");
    }

    /// **Each old flag the operator names logs exactly one WARN line saying the duty is always on.**
    #[test]
    fn each_named_old_flag_warns_once() {
        let args = t12(&[KEY, BOND, "--palw-panel", "--palw-round-lane", "--palw-chain-classes"]);
        assert_eq!((args.palw_panel, args.palw_round_lane, args.palw_chain_classes), (Some(true), Some(true), Some(true)));
        let warnings = palw_deprecated_duty_flag_warnings_v1(&args, &params_of(&args));
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        for (flag, line) in ["--palw-panel", "--palw-round-lane", "--palw-chain-classes"].iter().zip(&warnings) {
            assert!(line.starts_with(flag) && line.contains("deprecated") && line.contains("does nothing"), "{line}");
            assert!(line.contains("always"), "the line says the duty is always on: {line}");
            assert!(!line.contains("ignored"), "a plain `{flag}` has no value to ignore: {line}");
        }
        // Only the named ones.
        let args = t12(&[KEY, BOND, "--palw-round-lane"]);
        let warnings = palw_deprecated_duty_flag_warnings_v1(&args, &params_of(&args));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("--palw-round-lane"));
    }

    /// **A node without the identity runs the duty as a clean no-op** — the seatless relay (pre-t12's
    /// t0), a public node with no key, a node with a key and no bond — whether or not its unit still
    /// names the old flags. The plan says why, and the summary is the one INFO line it prints.
    #[test]
    fn a_node_without_identity_idles_every_duty_with_one_line() {
        for (extra, panel_why, lane_why) in [
            (vec![], "no --palw-producer-key", "no --palw-producer-key"),
            (vec!["--palw-panel", "--palw-round-lane"], "no --palw-producer-key", "no --palw-producer-key"),
            (vec![KEY], "no --palw-producer-bond", "no --palw-producer-bond"),
            (vec![KEY, "--palw-panel", "--palw-round-lane"], "no --palw-producer-bond", "no --palw-producer-bond"),
        ] {
            let args = t12(&extra);
            let params = params_of(&args);
            let plan = palw_duty_plan_v1(&args, &params);
            assert!(matches!(&plan.panel, Err(PalwDutyIdleV1::NoIdentity(why)) if why.contains(panel_why)), "{extra:?}: {plan:?}");
            assert!(matches!(&plan.round_lane, Err(PalwDutyIdleV1::NoIdentity(why)) if why.contains(lane_why)), "{extra:?}: {plan:?}");
            let line = palw_duty_summary_line_v1(&plan, &params);
            assert!(!line.contains('\n'), "one line: {line}");
            assert!(line.contains("panel idle") && line.contains("execution lane idle"), "{line}");
        }
    }

    /// **A node registering its first bond has a key and no bond**: the panel runs (its registration
    /// worker), the lane does not — there is no bond to sign a round for yet.
    #[test]
    fn a_first_bond_registration_runs_the_panel_and_not_the_lane() {
        let args = t12(&[KEY, "--palw-register-bond"]);
        let plan = palw_duty_plan_v1(&args, &params_of(&args));
        assert_eq!(plan.panel, Ok(PalwSeatIdentityV1 { key_path: "/nonexistent/bond.key".into(), bond: None }));
        assert!(matches!(plan.round_lane, Err(PalwDutyIdleV1::NoIdentity(_))));
    }

    /// **The params decide, not a list of networks.** A network whose params configure no ConsensusV2
    /// ruleset has no panels and no lane, whatever identity the node holds; testnet-11 has panels and
    /// its own lane fence, read from its params.
    #[test]
    fn the_params_decide_where_a_duty_exists() {
        let args = t12(&[KEY, BOND]);
        for net in [
            NetworkId::new(NetworkType::Mainnet),
            NetworkId::with_suffix(NetworkType::Testnet, 11),
            NetworkId::new(NetworkType::Devnet),
        ] {
            let params = Params::from(net);
            let plan = palw_duty_plan_v1(&args, &params);
            let v2 = matches!(params.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_));
            let lane = params.palw_execution_lane_fence().is_some();
            assert_eq!(plan.panel.is_ok(), v2, "{net}: the panel exists exactly where the params declare ConsensusV2");
            assert_eq!(plan.round_lane.is_ok(), lane, "{net}: the lane runs exactly where the params configure it");
            if !v2 {
                assert!(matches!(plan.panel, Err(PalwDutyIdleV1::NotOnThisNetwork(_))), "{net}");
            }
            if !lane {
                assert!(matches!(plan.round_lane, Err(PalwDutyIdleV1::NotOnThisNetwork(_))), "{net}");
            }
        }
    }

    /// **Where chain classes are NOT a duty (the fence: testnet-11, mainnet), the opt-in survives and
    /// the deprecation warning does not fire** — the flag still means something there.
    #[test]
    fn chain_classes_stay_an_opt_in_where_the_params_leave_them_off() {
        let t11 = parse(&["--testnet", "--netsuffix=11", "--palw-chain-classes"]);
        let params = params_of(&t11);
        assert!(!palw_chain_classes_default(&params));
        assert!(palw_duty_plan_v1(&t11, &params).chain_classes, "the operator's opt-in still arms it");
        assert!(palw_deprecated_duty_flag_warnings_v1(&t11, &params).is_empty(), "not deprecated where it is not a duty");
        let t11_off = parse(&["--testnet", "--netsuffix=11"]);
        assert!(!palw_duty_plan_v1(&t11_off, &params).chain_classes);
    }

    /// **The kit's duty check reads this token** (`contrib/t12-deploy-kit/lib.sh`): the launch script
    /// refuses a binary whose help does not mark each duty flag `ALWAYS-ON DUTY`, because the kit no
    /// longer passes them and an older binary would then run no panel and no lane.
    #[test]
    fn every_duty_flag_says_always_on_in_its_help() {
        for id in ["palw-panel", "palw-round-lane", "palw-chain-classes"] {
            let arg = crate::args::cli().get_arguments().find(|a| a.get_id() == id).cloned().expect("the flag still parses");
            let help = arg.get_help().expect("help").to_string();
            assert!(help.starts_with("ALWAYS-ON DUTY"), "{id}: {help}");
        }
    }
}
