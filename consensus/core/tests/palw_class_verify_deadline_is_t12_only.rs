//! **ADR-0152 §4-quater's class-verify-deadline fence is testnet-12's alone (T-D7, T-D7b).**
//!
//! `Params::palw_class_verify_deadline` changes which claims a block may carry (V2), when a licensed
//! claim may `Final` (V4) and what a licence locks (V5), so it is a consensus fence. It is `Some(0)` on
//! testnet-12 and `None` everywhere else, hashed Some-only in every writer, with its measured rows
//! hashed only when non-empty — so testnet-11, devnet and mainnet fingerprint byte-identically to the
//! build before the field existed (the numbers `palw_offence_attribution_is_t12_only` pins, repeated
//! here), and on testnet-11's own bundle every rule it gates answers exactly what it answered before.
//!
//! Run: cargo test -p kaspa-consensus-core --test palw_class_verify_deadline_is_t12_only

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    ForkActivation, Params, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::{
    PalwDerivedProfileV1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, PalwModelWorkV1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwChainStateV2, PalwStateCarriageV2, PalwStateParamsV2, palw_class_needs_measured_row_v1, palw_panel_holds_to_final_v1,
};

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` of the presets that
/// do not arm the fence — `palw_offence_attribution_is_t12_only`'s `BEFORE_THE_ATTRIBUTION`, which the
/// parent of this fence (`c3fe99cd`) produces.
const BEFORE_THE_DEADLINE: &[(&str, &str, &str, &str)] = &[
    (
        "testnet-11",
        "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "devnet",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "mainnet",
        "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1",
        "00d98599bd45867f4a7b3bef1043431ccb8ef0d41cedbc552a87926b5e3b8af5",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
];

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        other => panic!("no such preset {other}"),
    }
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// Armed on testnet-12 from genesis with no measured row, mirrored into its bundle; dormant, unrowed
/// and unmirrored everywhere else.
#[test]
fn the_fence_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_class_verify_deadline, Some(ForkActivation::always()));
    assert!(t12.palw_class_verify_rows.is_empty(), "the 2M row is closed at launch (U-D1)");
    t12.validate_palw_v2().expect("testnet-12 validates with the fence");
    for (name, _, _, _) in BEFORE_THE_DEADLINE {
        let p = shipped(name);
        assert_eq!(p.palw_class_verify_deadline, None, "{name}: dormant");
        assert!(p.palw_class_verify_rows.is_empty(), "{name}: no rows");
        assert_eq!(p.palw_class_verify_deadline_fence(), None, "{name}");
        for daa in [0, 8_160, u64::MAX] {
            assert!(!p.palw_class_verify_deadline_active_at(daa), "{name}: never in force ({daa})");
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert_eq!(bundle.state.class_verify_deadline_from_daa(), None, "{name}: no mirror");
            assert!(bundle.state.class_verify_rows().is_empty(), "{name}");
        }
    }
}

/// **testnet-11, devnet and mainnet are byte-identical to the build before the fence.**
#[test]
fn testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_fence() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in BEFORE_THE_DEADLINE {
        let now = ids(&shipped(name));
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the class-verify-deadline fence moved: {moved:?}");
}

/// **The fence moves testnet-12's three ids, and its twin validates.**
#[test]
fn the_fence_moves_testnet12s_fingerprint() {
    let t12 = palw_t12_shipped_params();
    let mut twin = t12.clone();
    twin.palw_class_verify_deadline = None;
    twin.sync_palw_class_verify_deadline();
    twin.validate_palw_v2().expect("the fence-off twin validates");
    println!("testnet-12 with the fence:    {:?}", ids(&t12));
    println!("testnet-12 without the fence: {:?}", ids(&twin));
    assert_ne!(t12.consensus_params_id(), twin.consensus_params_id(), "the ruleset a node announces names the fence");
    assert_ne!(t12.consensus_identity_id(), twin.consensus_identity_id(), "in force from block one: two identities");
    assert_ne!(t12.consensus_schedule_id(), twin.consensus_schedule_id(), "and the schedule names it");
}

/// **T-D7b on testnet-11's own bundle: every gated rule answers as before, even for a class past 24
/// spans.** testnet-11 arms §11.3 (at 8,160, over a 5-DAA lane span), not this fence: a 30-span
/// class's receipt window is §11.3's `30 × 5 = 150` floored at the global window, its `D` is that
/// product, no verification horizon exists at any height (so its licensed Final floor stays
/// `L + window_challenge_at(L)`), it is no NM class, and the panel room holds it to Final only if C7
/// does.
#[test]
fn testnet11s_rules_are_unmoved_for_a_class_past_24_spans() {
    let t11 = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t11.palw_consensus_mode else { panic!("testnet-11 is ConsensusV2") };
    let sp: &PalwStateParamsV2 = &bundle.state;
    let class = Hash64::from_u64_word(0x30);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work: PalwModelWorkV1 {
            verification_ccu: 29 * 1_200_000_000_000,
            economic_ccu_per_claim: 1,
            ops_supported: true,
            ..Default::default()
        },
        profile: PalwDerivedProfileV1 { verification_window_spans: 30, max_inflight_claims: 3, ..Default::default() },
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.model_lifecycles.insert(class, row);
    let s = carriage.into_state(sp, None).expect("a one-row state");
    let fence = sp.class_receipt_window_daa().expect("testnet-11 arms §11.3");
    let a = PalwClaimVerifyShapeV1::Attempt;
    let product = 30 * 5;
    assert_eq!(sp.claim_verify_daa_v1(&s, &class, a, fence), product, "§11.3's product, verbatim");
    assert_eq!(sp.receipt_window_for_claim_v1(&s, &class, a, fence), sp.window_receipt().max(product));
    assert_eq!(sp.receipt_window_for_claim_v1(&s, &class, a, fence - 1), sp.window_receipt(), "below §11.3, the global window");
    assert!(!palw_class_needs_measured_row_v1(sp, &s, &class), "150 DAA is inside the global window: not NM");
    assert!(!palw_panel_holds_to_final_v1(sp, &s, &class), "no long-D hold below the fence");
    // No verification horizon exists at any height, so the licensed Final floor, the licence-time lock
    // and the DA pause are the pre-fence rules (the builder-level twins are `palw_state_v2`'s
    // `class_verify_deadline` unit tests, at testnet-11's 5-DAA span).
    for daa in [0, fence, u64::MAX] {
        assert!(!sp.class_verify_deadline_active_at(daa), "no horizon at {daa}");
    }
}
