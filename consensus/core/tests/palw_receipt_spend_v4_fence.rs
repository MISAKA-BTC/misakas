//! **RFC-0009 stage C — `palw_receipt_spend_v4`** — dormant on every ruleset a node can run (NOT on the DAA-5,300 list), fingerprinted
//! where armed (its fee cap with it), invisible to the identity until it fires, named by the fork id, refused without its
//! prerequisites, and byte-identical below its height.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_receipt_spend_v4_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_receipt_v4::{
    PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS, PALW_COMMITMENT_MAX_BYTES_V4, palw_receipt_spend_v4_value_v1,
};

/// A height no testnet-12 fence uses.
const AT: u64 = 7_919;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: Option<ForkActivation>) -> Params {
    let mut p = palw_t12_release_v5_params();
    p.palw_receipt_spend_v4 = at;
    p
}

#[test]
fn it_is_in_no_release_and_dormant_on_every_ruleset() {
    assert!(PALW_T12_INT11_FENCES_V1.iter().all(|f| f.name != "palw_receipt_spend_v4"), "lane R9 is not in the DAA-5,300 flag day");
    for (name, p) in [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
    ] {
        assert_eq!(p.palw_receipt_spend_v4, None, "{name}");
        assert!(!p.palw_receipt_spend_v4_active_at(u64::MAX - 1), "{name}");
        assert!(p.palw_fences_v1().contains(&("palw_receipt_spend_v4", None)), "{name}: the exhaustive list names it");
        p.validate_palw_receipt_spend_v4().unwrap_or_else(|e| panic!("{name}: dormant validates: {e:?}"));
    }
    palw_t12_shipped_params().validate_palw_v2().expect("the shipped testnet-12 ruleset still validates");
}

#[test]
fn armed_it_moves_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_it() {
    let base = palw_t12_release_v5_params();
    let p = armed(Some(ForkActivation::new(AT)));
    p.validate_palw_v2().expect("testnet-12 with the redemption armed validates (both audit fences are genesis rules there)");
    let (a, b) = (ids(&p), ids(&base));
    assert_ne!(a.0, b.0, "the params id names it");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "two builds that differ only about a future height share an identity");
    assert!(p.palw_receipt_spend_v4_active_at(AT) && !p.palw_receipt_spend_v4_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&base, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses(), "gated from its height");
    let below = fork_id_v1(&base, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses(), "kept below it");
    // Set back: the dormant ruleset, to the id; `never()` collapses out of the identity.
    assert_eq!(ids(&armed(None)), b, "None is the dormant ruleset");
    let never = armed(Some(ForkActivation::never()));
    never.validate_palw_v2().expect("dormant");
    assert_eq!(ids(&never).1, b.1);
    assert!(!never.palw_receipt_spend_v4_active_at(u64::MAX - 1), "never() is not armed");
    assert_eq!(never.palw_receipt_spend_v4_fence(), None);
    // The fee cap is a companion value of the fence, hashed with it.
    assert_eq!(palw_receipt_spend_v4_value_v1(), [PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS as u64]);
    assert!(PALW_COMMITMENT_MAX_BYTES_V4 > kaspa_consensus_core::pow_layer0::PALW_COMMITMENT_MAX_BYTES);
}

#[test]
fn it_is_refused_without_its_prerequisites_and_off_consensus_v2() {
    for (what, strip) in [
        ("palw_audit_2026_09_11", (|p: &mut Params| p.palw_audit_2026_09_11 = None) as fn(&mut Params)),
        ("palw_audit_2026_09_23", |p| p.palw_audit_2026_09_23 = None),
        ("palw_audit_2026_09_11 later than the fence", |p| p.palw_audit_2026_09_11 = Some(ForkActivation::new(AT + 1))),
        ("palw_audit_2026_09_23 later than the fence", |p| p.palw_audit_2026_09_23 = Some(ForkActivation::new(AT + 1))),
    ] {
        let mut p = armed(Some(ForkActivation::new(AT)));
        strip(&mut p);
        let e = p.validate_palw_receipt_spend_v4().expect_err(what);
        assert!(format!("{e:?}").contains("palw_receipt_spend_v4"), "{what}: {e:?}");
        assert!(p.validate_palw_v2().is_err(), "{what}: validate_palw_v2 asks it too");
    }
    let mut v1 = TESTNET11_PARAMS;
    v1.palw_receipt_spend_v4 = Some(ForkActivation::new(AT));
    assert!(v1.validate_palw_receipt_spend_v4().is_err(), "a ruleset with no V2 bundle");
}
