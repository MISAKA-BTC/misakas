//! **Lane sink — the model sink binding (`Params::palw_model_sink_bound`, post-launch, 2026-09-26) is
//! dormant on every shipped preset, and arming it is a scheduled fence like any other.**
//!
//! testnet-12 launched from `0e8ec984e` with a model sink (`OP_RETURN "MSKMDL01" <line>`) valid in any
//! transaction, bound to an object or not: an unbound one burns its MSK with nothing recorded (the
//! 2026-09-25 Position review's #1). The fix ships after launch behind this fence, which the operator
//! arms at the common post-launch height with every other fix of the release. So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//!   release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`, the shipping re-pin `9c717c16d`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, so an armed node and a shipped node stay peers until the height (and a
//!   `Some(never())` collapses to absence — the fourth of the four places a Some-only fence needs);
//! * the fork id names the height: below it an armed build and the shipped build keep each other, and
//!   from it the armed build refuses the shipped one (a named partition at the flag day, not a silent
//!   fork) — which is why the height must not be one testnet-12 already schedules (1,000);
//! * `validate_palw_v2` refuses it off ConsensusV2, without the market it binds, and without the P-B1
//!   refund route (`palw_audit_2026_09_23`) at or below it.
//!
//! The rule itself: [`palw_model_sink_binding_refusal_v1`] below (every burn route the review listed),
//! the header-context door on both sides of the fence
//! (`tx_validation_in_isolation::…::a_model_sink_is_valid_only_bound_from_the_sink_fence`), a
//! processor chain that crosses it (`t12_model_sink_bound_fence`), and the mempool's standardness
//! refusal that needs no fence (`a_sink_is_relayed_only_where_the_network_declares_the_market`).

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_model_market_v1::{palw_model_sink_binding_refusal_v1, palw_model_sink_spk_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_LIFECYCLE};
use kaspa_consensus_core::tx::{Transaction, TransactionOutput};
use kaspa_hashes::Hash64;

/// testnet-12 as the release ships it (`0e8ec984e`; pinned by the shipping re-pin `9c717c16d` as
/// `palw_clock_lead_cap_is_t12_only::T12_WITH_THE_CAP`): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick after launch: the common post-launch height (500), a low one a
/// drill crosses, and a later one. Never 1,000 — `palw_bond_maturity`'s height on testnet-12, where a
/// second fence would be invisible to the fork id.
const HEIGHTS: [u64; 3] = [60, 500, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_model_sink_bound = Some(ForkActivation::new(height));
    p
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12", palw_t12_shipped_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its Some-only
/// writers and its collapse cost the live chain nothing until an operator arms it.
#[test]
fn the_binding_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_model_sink_bound, None, "{name}: lane sink's fence ships dormant");
        assert_eq!(p.palw_model_sink_bound_fence(), None, "{name}");
        assert!(!p.palw_model_sink_bound_active_at(0) && !p.palw_model_sink_bound_active_at(u64::MAX), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_model_sink_bound" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert!(t12.palw_model_market_active_at(0), "the market the binding binds is declared on testnet-12 from genesis");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — so an
/// armed node and a shipped node stay peers until the height (M1-6). Armed at genesis it is a rule in
/// force from block one, and the identity separates the two. A `Some(never())` is absence.
#[test]
fn arming_the_binding_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 with the binding at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule the operator log names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert_eq!(armed.palw_model_sink_bound_fence(), Some(ForkActivation::new(height)));
        assert!(armed.palw_model_sink_bound_active_at(height) && !armed.palw_model_sink_bound_active_at(height - 1));
    }
    // Some(never()): the normalised form of a scheduled fence — absence in the identity, or the collapse
    // in `normalize_values_a_scheduled_fence_drags_with_it` is gone (a-some-only-fence).
    let mut never = shipped.clone();
    never.palw_model_sink_bound = Some(ForkActivation::never());
    never.validate_palw_v2().expect("a never-armed fence is no fence");
    assert_eq!(never.palw_model_sink_bound_fence(), None, "a never-armed fence arms nothing");
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    // At genesis the rule is in force from block one: another identity (a regenesis, not this lane).
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height** (a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id): the
/// fence is on the gate, at its own height, and not at a height testnet-12 already schedules. Below it
/// an armed build and the shipped build keep each other in BOTH directions; from it the armed build
/// refuses the shipped one — the flag day is a named refusal, not a silent fork.
#[test]
fn an_armed_build_below_the_binding_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    println!("testnet-12 as shipped gates on {shipped_gate:?}");
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height, not one the release schedules");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            let armed_sees = evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next);
            let shipped_sees = evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next);
            assert!(
                !armed_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the armed node keeps the shipped one ({armed_sees:?})"
            );
            assert!(
                !shipped_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the shipped node keeps the armed one ({shipped_sees:?})"
            );
        }
        let s = fork_id_v1(&shipped, height);
        let past = evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next);
        println!("armed at {height}: at the height the armed node says {past:?} to a shipped peer");
        assert!(past.refuses(), "armed at {height}: from the height the armed node refuses a node that did not upgrade");
    }
}

/// **What `validate_palw_v2` refuses**: the binding off ConsensusV2, without the market it binds,
/// and without the P-B1 refund route (`palw_audit_2026_09_23`) at or below it. Any height is legal
/// on testnet-12 as shipped (both are armed at genesis there).
#[test]
fn the_binding_needs_the_market_and_the_refund_route_at_or_below_it() {
    let refused = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = armed_at(500);
        edit(&mut p);
        let why = p.validate_palw_model_sink_bound_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
    };
    refused(&|p| p.palw_model_market = None, "without palw_model_market declared");
    refused(&|p| p.palw_model_market = Some(ForkActivation::never()), "without palw_model_market declared");
    refused(&|p| p.palw_audit_2026_09_23 = Some(ForkActivation::new(501)), "without palw_audit_2026_09_23 at or below it");
    // At the same height is at or below it.
    let mut same = armed_at(500);
    same.palw_audit_2026_09_23 = Some(ForkActivation::new(500));
    assert_eq!(same.validate_palw_model_sink_bound_v1(), Ok(()));
    // A later market is legal: below it the header context refuses the sink form outright.
    let mut later = armed_at(500);
    later.palw_model_market = Some(ForkActivation::new(1_234));
    assert_eq!(later.validate_palw_model_sink_bound_v1(), Ok(()));
    // Off ConsensusV2 (mainnet's bundle-free const), the fence is refused by name.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_model_sink_bound = Some(ForkActivation::new(500));
    assert!(v1.validate_palw_model_sink_bound_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (None or never) is legal anywhere: the field costs a network that does not arm it nothing.
    for (name, mut p) in presets() {
        p.palw_model_sink_bound = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_model_sink_bound_v1(), Ok(()), "{name}: never() is dormant");
    }
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis market and audit fence, any height is legal");
    }
}

/// **The rule itself, on its own:** the honest buy and seed are bound; a transaction with no sink is
/// not asked; every burn route the review listed names its output and its reason.
#[test]
fn the_rule_names_every_unbound_sink() {
    let line = Hash64::from_u64_word(0x11E);
    let paid = 700_000_000u64;
    let change = TransactionOutput::new(5_000, kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[3u8; 64]));
    let sink = |value, line: Hash64| TransactionOutput::new(value, palw_model_sink_spk_v1(&line));
    let carrier = |object: Option<PalwConsensusObjectV2>, subnet, outputs: Vec<TransactionOutput>| {
        let payload = object
            .map(|object| borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap())
            .unwrap_or_default();
        Transaction::new(0, vec![], outputs, 0, subnet, 0, payload)
    };
    let buy = |msk_in, sink_index, line_id| {
        Some(PalwConsensusObjectV2::ModelBuy { line_id, holder: Hash64::from_u64_word(1), msk_in, min_units_out: 0, sink_index })
    };
    let seed = |msk_seed, sink_index| {
        Some(PalwConsensusObjectV2::ModelSeed { line_id: line, seeder: Hash64::from_u64_word(2), msk_seed, sink_index })
    };
    assert_eq!(
        palw_model_sink_binding_refusal_v1(&carrier(
            buy(paid, 1, line),
            SUBNETWORK_ID_PALW_LIFECYCLE,
            vec![change.clone(), sink(paid, line)]
        )),
        None
    );
    assert_eq!(
        palw_model_sink_binding_refusal_v1(&carrier(
            seed(paid, 1),
            SUBNETWORK_ID_PALW_LIFECYCLE,
            vec![change.clone(), sink(paid, line)]
        )),
        None
    );
    // The refund payee may sit anywhere in the carrier (P-B1 pays the first one).
    assert_eq!(
        palw_model_sink_binding_refusal_v1(&carrier(
            buy(paid, 0, line),
            SUBNETWORK_ID_PALW_LIFECYCLE,
            vec![sink(paid, line), change.clone()]
        )),
        None
    );
    assert_eq!(
        palw_model_sink_binding_refusal_v1(&carrier(None, SUBNETWORK_ID_NATIVE, vec![change.clone()])),
        None,
        "no sink, no question"
    );
    for (tx, index, why) in [
        (carrier(None, SUBNETWORK_ID_NATIVE, vec![sink(paid, line)]), 0, "rides only a lifecycle carrier"),
        (carrier(None, SUBNETWORK_ID_NATIVE, vec![change.clone(), sink(paid, line)]), 1, "rides only a lifecycle carrier"),
        (carrier(None, SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid, line)]), 1, "carries no ModelBuy or ModelSeed"),
        (carrier(buy(paid, 1, line), SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid - 1, line)]), 1, "another amount"),
        (carrier(seed(paid + 1, 1), SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid, line)]), 1, "another amount"),
        (
            carrier(buy(paid, 1, Hash64::from_u64_word(9)), SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid, line)]),
            1,
            "another line",
        ),
        (carrier(buy(paid, 7, line), SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid, line)]), 1, "is not the output"),
        (
            carrier(buy(paid, 1, line), SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid, line), sink(1, line)]),
            2,
            "is not the output",
        ),
        (carrier(buy(paid, 0, line), SUBNETWORK_ID_PALW_LIFECYCLE, vec![sink(paid, line)]), 0, "pays no P2PKH-ML-DSA-87 output"),
    ] {
        let (i, reason) = palw_model_sink_binding_refusal_v1(&tx).unwrap_or_else(|| panic!("{why}: expected a refusal"));
        assert_eq!(i, index, "{why}");
        assert!(reason.contains(why), "{why}: {reason}");
    }
    // An undecodable payload binds nothing: its sink is refused, not guessed.
    let mut garbled = carrier(buy(paid, 1, line), SUBNETWORK_ID_PALW_LIFECYCLE, vec![change.clone(), sink(paid, line)]);
    garbled.payload.truncate(3);
    assert!(palw_model_sink_binding_refusal_v1(&garbled).is_some_and(|(i, why)| i == 1 && why.contains("carries no ModelBuy")));
}
