//! **RFC-0009 stage C — `palw_receipt_spend_v4`** — dormant on EVERY ruleset a node can run, testnet-12 as shipped included (NOT on the
//! DAA-5,300 list; on the int-13 list, which is unscheduled — the user cancelled the DAA-9,000 flag day on 2026-10-08 and the list waits
//! for the full-activation release: `palw_t12_flag_day_9000.rs`), fingerprinted where armed (its fee cap with it), invisible to the
//! identity until it fires, named by the fork id, refused without its prerequisites, and byte-identical below its height.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_receipt_spend_v4_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FENCES_V1, PALW_T12_INT13_DAA, PALW_T12_INT13_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_arm_int13_flag_day_at_v1, palw_t12_release_v5_params,
    palw_t12_release_v6_params, palw_t12_shipped_params,
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
fn it_is_only_on_the_int13_list_and_dormant_on_every_shipped_ruleset() {
    assert!(PALW_T12_INT11_FENCES_V1.iter().all(|f| f.name != "palw_receipt_spend_v4"), "lane R9 is not in the DAA-5,300 flag day");
    assert!(PALW_T12_INT13_FENCES_V1.iter().any(|f| f.name == "palw_receipt_spend_v4"), "…it is on the int-13 list");
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
        ("palw_t12_release_v6_params", palw_t12_release_v6_params()),
    ] {
        assert_eq!(p.palw_receipt_spend_v4, None, "{name}");
        assert!(!p.palw_receipt_spend_v4_active_at(u64::MAX - 1), "{name}");
        assert!(p.palw_fences_v1().contains(&("palw_receipt_spend_v4", None)), "{name}: the exhaustive list names it");
        p.validate_palw_receipt_spend_v4().unwrap_or_else(|e| panic!("{name}: dormant validates: {e:?}"));
    }
    // Testnet-12 as shipped arms it NOWHERE (the int-13 list is unscheduled), and still validates.
    let shipped = palw_t12_shipped_params();
    assert_eq!(PALW_T12_INT13_DAA, None, "no DAA-9,000 flag day (user, 2026-10-08)");
    assert_eq!(shipped.palw_receipt_spend_v4, None, "the shipped ruleset does not arm it");
    assert!(!shipped.palw_receipt_spend_v4_active_at(u64::MAX - 1));
    assert!(shipped.palw_fences_v1().contains(&("palw_receipt_spend_v4", None)), "the exhaustive list names it, dormant");
    shipped.validate_palw_v2().expect("the shipped testnet-12 ruleset still validates");
    // Armed through the int-13 list at a test height (the way a drill or the future release arms it), it is in force from there only.
    let mut listed = shipped.clone();
    palw_t12_arm_int13_flag_day_at_v1(&mut listed, Some(AT));
    assert_eq!(listed.palw_receipt_spend_v4, Some(ForkActivation::new(AT)), "the list arms it at its one height");
    assert!(!listed.palw_receipt_spend_v4_active_at(AT - 1) && listed.palw_receipt_spend_v4_active_at(AT));
    listed.validate_palw_v2().expect("the list armed at a test height validates");
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

/// **RFC-0009 stage C: the signer-sidecar purpose for the executor's authorization is visible only under this fence.** Every older
/// purpose is offered everywhere (the fence is not theirs); `PalwReceiptAuthV4` is offered nowhere a preset ships it, and from its
/// height only where the fence is armed — the same `Params::palw_receipt_spend_v4_active_at` the chain's own header stage asks.
#[test]
fn the_signing_purpose_for_the_authorization_is_visible_only_under_the_fence() {
    use kaspa_consensus_core::dns_finality::SigningPurpose;
    let older = [
        SigningPurpose::Transaction,
        SigningPurpose::Attestation,
        SigningPurpose::TakeoverToken,
        SigningPurpose::Unbond,
        SigningPurpose::PalwAttemptV2,
        SigningPurpose::PalwFpCommitmentV3,
        SigningPurpose::PalwFpSpendV3,
        SigningPurpose::PalwDerivedArtifactV1,
    ];
    let dormant = armed(None);
    let live = armed(Some(ForkActivation::new(AT)));
    for daa in [0, AT - 1, AT, u64::MAX - 1] {
        for purpose in older {
            assert!(
                purpose.offered_by(&dormant, daa) && purpose.offered_by(&live, daa),
                "{purpose:?} is offered everywhere, as it always was"
            );
        }
        assert!(!SigningPurpose::PalwReceiptAuthV4.offered_by(&dormant, daa), "dormant: never offered (daa {daa})");
    }
    assert!(!SigningPurpose::PalwReceiptAuthV4.offered_by(&live, AT - 1), "below the height it is not offered");
    assert!(SigningPurpose::PalwReceiptAuthV4.offered_by(&live, AT), "from the height it is");
    // Every shipped ruleset, testnet-12 included, leaves it unoffered.
    for p in [palw_t12_shipped_params(), mainnet_shipped_params(), devnet_shipped_params(), palw_rc_shipped_params()] {
        assert!(!SigningPurpose::PalwReceiptAuthV4.offered_by(&p, u64::MAX - 1));
    }
    // `never()` is not armed.
    assert!(!SigningPurpose::PalwReceiptAuthV4.offered_by(&armed(Some(ForkActivation::never())), u64::MAX - 1));
    // And the new purpose changes no identity: the params/schedule ids are the dormant ruleset's.
    assert_eq!(ids(&dormant), ids(&armed(None)));
}
