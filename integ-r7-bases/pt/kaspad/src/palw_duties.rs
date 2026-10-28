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
//!
//! **One bond, one process.** Because a duty has no off switch, every process that holds a bond's key
//! and outpoint runs that bond's duties — a second one (a standby, a registration run, a copy re-synced
//! into a fresh app dir) signs the same round permit twice and is slashed. A bonded node says so at
//! startup ([`palw_one_process_per_bond_line_v1`]; a WARN on a `--palw-register-class` run).
//!
//! **The plan and what started.** The summary line is printed before the services exist; a duty the
//! plan put ON whose key or bond then fails to load gets one `PALW duties NOT as planned` WARN line
//! ([`palw_duty_shortfall_lines_v1`]) once they do.

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
///
/// `fee_outpoint` is the owner's consent to spend (`--palw-fee-outpoint`), not a switch on the duty:
/// without it the seat still signs its receipts, replays and proofs, and carries nothing on chain
/// ("receipts only", the panel's own startup line) — the summary line says so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatIdentityV1 {
    pub key_path: String,
    pub bond: Option<String>,
    pub fee_outpoint: Option<String>,
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
        (true, Some(key_path), Some(bond)) => Ok(PalwSeatIdentityV1 {
            key_path: key_path.clone(),
            bond: Some(bond.clone()),
            fee_outpoint: args.palw_fee_outpoint.clone(),
        }),
        (true, Some(key_path), None) if args.palw_register_bond => {
            Ok(PalwSeatIdentityV1 { key_path: key_path.clone(), bond: None, fee_outpoint: args.palw_fee_outpoint.clone() })
        }
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
        Ok(PalwSeatIdentityV1 { bond: Some(bond), fee_outpoint: Some(_), .. }) => format!("panel seat duties ON as bond {bond}"),
        // The duty runs; only the SPENDING is off, by the owner's choice — said here, because this is
        // the line an operator (and the kit's `switch`) reads to learn what the seat does.
        Ok(PalwSeatIdentityV1 { bond: Some(bond), fee_outpoint: None, .. }) => format!(
            "panel seat duties ON as bond {bond} (receipts only: no --palw-fee-outpoint, so this seat carries nothing on \
             chain — no DA answer, filer or carrier)"
        ),
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

/// **One bond, one process** — the line a node that acts as a bond prints next to the summary, and
/// whether it is a WARN (`true`) rather than an INFO.
///
/// A duty with no off switch runs in EVERY process that holds the bond's key and outpoint. Two such
/// processes at once sign the same execution-lane permit over two different templates, and consensus
/// slashes the whole bond for it (`RoundPermitEquivocated`) — the round producer's restart record
/// (`palw-round-last-signed`) lives in each process's own app dir, so neither can know the other
/// signed. It is a WARN where the operator started this process to REGISTER something under a bond
/// (`--palw-register-class`): that is the run the runbooks used to start beside the bond's live node.
pub fn palw_one_process_per_bond_line_v1(plan: &PalwDutyPlanV1, args: &Args) -> Option<(bool, String)> {
    let bond = match (&plan.round_lane, &plan.panel) {
        (Ok(PalwBondIdentityV1 { bond, .. }), _) | (_, Ok(PalwSeatIdentityV1 { bond: Some(bond), .. })) => bond,
        _ => return None,
    };
    let lane = plan.round_lane.is_ok();
    let signs = if lane { "signs its execution-lane round permits and answers its seat duties" } else { "answers its seat duties" };
    let slash = if lane {
        "signs the same round permit twice and the chain slashes the bond (RoundPermitEquivocated)"
    } else {
        "answers the same duties twice and spends the same fee output"
    };
    let mut line = format!(
        "PALW bond {bond}: run it in exactly ONE process, and keep that process's app dir. This process {signs}; a second \
         process holding the same bond at the same time — a standby, a registration run, a copy on another host, or this node \
         re-synced into a fresh app dir while the old one still runs — {slash}."
    );
    let warn = args.palw_register_class.is_some();
    if warn {
        line.push_str(
            " --palw-register-class was given: this registration run IS a node for that bond and does not exit when the class \
             lands. If another node runs this bond now, stop one of them — register through the running node instead \
             (`misaka model add`, or restart THAT node with the flag and remove it once the class is on the chain).",
        );
    }
    Some((warn, line))
}

/// **When a planned duty did not start** — its key file would not load or its bond would not parse. The
/// summary line is the PLAN, printed before the services exist; this is the correction, one WARN line
/// per duty, printed once they do. It starts `PALW duties` so a reader of the last such line (the
/// deploy kit's `switch`) reads the correction and not the plan. A duty has no off switch, so a
/// shortfall is always a broken identity to fix, never a choice.
pub fn palw_duty_shortfall_lines_v1(plan: &PalwDutyPlanV1, panel_started: bool, round_lane_started: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if let (Ok(PalwSeatIdentityV1 { bond: Some(bond), key_path, .. }), false) = (&plan.panel, panel_started) {
        lines.push(format!(
            "PALW duties NOT as planned: panel seat duties are NOT running although this node was given bond {bond} — its key \
             ({key_path}) or bond did not load (the [palw-panel] warning above says which); the bond is still drawn for seats"
        ));
    }
    if let (Ok(PalwBondIdentityV1 { bond, key_path }), false) = (&plan.round_lane, round_lane_started) {
        lines.push(format!(
            "PALW duties NOT as planned: execution-lane round blocks are NOT running although this node was given bond {bond} \
             — its key ({key_path}) or bond did not load (the [palw-round-producer] warning above says which)"
        ));
    }
    lines
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
        assert_eq!(
            plan.panel,
            Ok(PalwSeatIdentityV1 { key_path: "/nonexistent/bond.key".into(), bond: Some("aa:0".into()), fee_outpoint: None })
        );
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
        assert_eq!(plan.panel, Ok(PalwSeatIdentityV1 { key_path: "/nonexistent/bond.key".into(), bond: None, fee_outpoint: None }));
        assert!(matches!(plan.round_lane, Err(PalwDutyIdleV1::NoIdentity(_))));
        assert_eq!(palw_one_process_per_bond_line_v1(&plan, &args), None, "no bond yet, nothing to run twice");
    }

    /// **`--palw-fee-outpoint` is the owner's consent to spend, not a switch on the duty**: without it
    /// the seat's duties still run and the summary line says the seat carries nothing on chain — the
    /// line the kit's `switch` reads.
    #[test]
    fn a_seat_without_a_fee_outpoint_is_on_and_says_receipts_only() {
        const FEE: &str = "--palw-fee-outpoint=bb:1";
        let bare = t12(&[KEY, BOND]);
        let params = params_of(&bare);
        let plan = palw_duty_plan_v1(&bare, &params);
        assert!(plan.panel.is_ok(), "the duty runs without the fee outpoint");
        let line = palw_duty_summary_line_v1(&plan, &params);
        assert!(line.contains("panel seat duties ON as bond aa:0 (receipts only: no --palw-fee-outpoint"), "{line}");
        let funded = t12(&[KEY, BOND, FEE]);
        let plan = palw_duty_plan_v1(&funded, &params);
        assert!(matches!(&plan.panel, Ok(PalwSeatIdentityV1 { fee_outpoint: Some(fee), .. }) if fee == "bb:1"), "{plan:?}");
        let line = palw_duty_summary_line_v1(&plan, &params);
        assert!(line.contains("panel seat duties ON as bond aa:0 |") && !line.contains("receipts only"), "{line}");
        // The kit's `switch` pattern holds for both.
        for args in [&bare, &funded] {
            let line = palw_duty_summary_line_v1(&palw_duty_plan_v1(args, &params), &params);
            let panel = line.find("panel seat duties ON").expect("panel ON");
            assert!(line[panel..].contains("round blocks ON"), "{line}");
        }
    }

    /// **One bond, one process** (the review of this change, 2026-09-25): a bonded node says it at
    /// startup, and a `--palw-register-class` run — the one the runbooks used to start BESIDE the
    /// bond's live node — says it as a WARN, naming the way to register through the running node.
    #[test]
    fn a_bonded_node_says_one_process_per_bond() {
        let seat = t12(&[KEY, BOND]);
        let (warn, line) = palw_one_process_per_bond_line_v1(&palw_duty_plan_v1(&seat, &params_of(&seat)), &seat).expect("bonded");
        assert!(!warn, "an ordinary seat is told, not warned");
        assert!(line.contains("aa:0") && line.contains("exactly ONE process") && line.contains("RoundPermitEquivocated"), "{line}");
        assert!(!line.contains('\n'), "one line: {line}");
        let reg = t12(&[KEY, BOND, "--palw-register-class=Qwen/Qwen2.5-1.5B/graph-v7@8192"]);
        let (warn, line) = palw_one_process_per_bond_line_v1(&palw_duty_plan_v1(&reg, &params_of(&reg)), &reg).expect("bonded");
        assert!(warn, "a registration run beside a live node is the slash the runbooks walked into");
        assert!(line.contains("--palw-register-class") && line.contains("misaka model add"), "{line}");
        for idle in [t12(&[]), t12(&[KEY])] {
            assert_eq!(palw_one_process_per_bond_line_v1(&palw_duty_plan_v1(&idle, &params_of(&idle)), &idle), None);
        }
        // Where there is no lane, the reason is the duplicated seat work, not a permit.
        let t11 = parse(&["--testnet", "--netsuffix=11", KEY, BOND]);
        let t11_params = params_of(&t11);
        if t11_params.palw_execution_lane_fence().is_none() {
            let (_, line) = palw_one_process_per_bond_line_v1(&palw_duty_plan_v1(&t11, &t11_params), &t11).expect("bonded");
            assert!(!line.contains("RoundPermitEquivocated"), "{line}");
        }
    }

    /// **The plan is corrected when a planned duty did not start** (the review of this change): one
    /// `PALW duties NOT as planned` WARN per duty whose key or bond failed to load, none when both
    /// started, none for a duty the plan never put ON — and each starts `PALW duties`, so the kit's
    /// `switch`, which reads the LAST such line, reads the correction.
    #[test]
    fn a_planned_duty_that_did_not_start_is_one_warn_line() {
        let seat = t12(&[KEY, BOND]);
        let plan = palw_duty_plan_v1(&seat, &params_of(&seat));
        assert!(palw_duty_shortfall_lines_v1(&plan, true, true).is_empty());
        let both = palw_duty_shortfall_lines_v1(&plan, false, false);
        assert_eq!(both.len(), 2, "{both:?}");
        assert!(both.iter().all(|l| l.starts_with("PALW duties NOT as planned") && !l.contains('\n')), "{both:?}");
        assert!(both[0].contains("panel seat duties are NOT running") && both[1].contains("round blocks are NOT running"));
        assert!(both.iter().all(|l| !l.contains("round blocks ON")), "never the kit's ON pattern: {both:?}");
        assert_eq!(palw_duty_shortfall_lines_v1(&plan, true, false).len(), 1);
        let idle = t12(&[]);
        assert!(palw_duty_shortfall_lines_v1(&palw_duty_plan_v1(&idle, &params_of(&idle)), false, false).is_empty());
        let first_bond = t12(&[KEY, "--palw-register-bond"]);
        assert!(
            palw_duty_shortfall_lines_v1(&palw_duty_plan_v1(&first_bond, &params_of(&first_bond)), false, false).is_empty(),
            "a first registration has no seat duties to fall short of"
        );
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
        // The open question this leaves (a NODE-side choice — no Params field, no fingerprint): the arm
        // reads the registry at GENESIS, and testnet-11's registry fence has already fired, so reading
        // it at the current DAA instead would make it a duty there too.
        assert!(params.palw_model_registry_at(u64::MAX), "testnet-11's registry fence has a height");
        assert!(!palw_chain_classes_default(&Params::from(NetworkId::new(NetworkType::Mainnet))), "mainnet: not from genesis");
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
