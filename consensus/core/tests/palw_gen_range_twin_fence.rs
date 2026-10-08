//! **`palw_gen_range_twin_v1` is dormant, and it changes nothing but the sizing's own work** (RFC-0003 PALW-GEN-20 by the range twin).
//!
//! * every shipped preset leaves it `None` but testnet-12 as shipped, which arms it with the int-13 list at DAA 9,000
//!   (`palw_t12_flag_day_9000.rs`; the int-12 release is the dormant baseline here), so every other shipped ruleset's ids are what they
//!   were (pinned in `palw_tir_fences_are_dormant.rs`; this file adds that arming it moves them, Some-only);
//! * `Some(never())` collapses whole in the handshake identity, as if absent;
//! * it is refused off a generative fence below it, and its bundle mirror must agree;
//! * the admission reads the twin from the rules at the block (`PalwGenAdmissionRulesV1::twin`).
//!
//! That the range twin sizes every close of a pipeline exactly as the element twin is `palw_gen_close_price.rs`'s
//! `the_generative_range_twin_sizes_every_close_exactly_as_the_element_twin`.

use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_INT13_DAA, Params, palw_t12_release_v6_params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_gen_admission_v1::PalwGenAdmissionRulesV1;
use kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1;
use kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;

const GEN_AT: u64 = 1_100;
const TWIN_AT: u64 = 1_200;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn with_gen() -> Params {
    let mut p = palw_t12_release_v6_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(GEN_AT)));
    p.sync_palw_tir_v1();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(GEN_AT)));
    p.sync_palw_gen_v1();
    p.validate_palw_v2().expect("the generative fence at GEN_AT");
    p
}

#[test]
fn every_preset_leaves_the_fence_unarmed() {
    let mut nets: Vec<Params> = [NetworkType::Mainnet, NetworkType::Devnet, NetworkType::Simnet]
        .into_iter()
        .map(|t| Params::from(NetworkId::new(t)))
        .collect();
    for suffix in [10u32, 11] {
        nets.push(Params::from(NetworkId::with_suffix(NetworkType::Testnet, suffix)));
    }
    nets.push(palw_t12_release_v6_params());
    for p in nets {
        assert_eq!(p.palw_gen_range_twin_v1, None, "{}: dormant", p.net);
        assert!(!p.palw_gen_range_twin_active_at(u64::MAX));
        assert_eq!(p.palw_gen_close_twin_at(u64::MAX), PalwTirCloseTwinV1::Element);
    }
    // Testnet-12 as shipped arms it with the int-13 list, at 9,000: the range twin from there, the element twin below.
    let at = PALW_T12_INT13_DAA.expect("the int-13 flag day");
    for p in [Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)), palw_t12_shipped_params()] {
        assert_eq!(p.palw_gen_range_twin_v1, Some(ForkActivation::new(at)), "{}: the int-13 list arms it", p.net);
        assert_eq!(p.palw_gen_close_twin_at(at - 1), PalwTirCloseTwinV1::Element);
        assert_eq!(p.palw_gen_close_twin_at(at), PalwTirCloseTwinV1::Range);
    }
}

#[test]
fn arming_it_moves_the_ids_and_never_is_absent() {
    let base = with_gen();
    let mut armed = base.clone();
    armed.palw_gen_range_twin_v1 = Some(ForkActivation::new(TWIN_AT));
    armed.sync_palw_gen_range_twin_v1();
    armed.validate_palw_v2().expect("armed over the generative fence");
    let (b, a) = (ids(&base), ids(&armed));
    assert_ne!(b.0, a.0, "the params id names the fence when it is armed");
    assert_ne!(b.2, a.2, "the schedule id names its height");
    let mut never = base.clone();
    never.palw_gen_range_twin_v1 = Some(ForkActivation::never());
    never.sync_palw_gen_range_twin_v1();
    never.validate_palw_v2().expect("a never() fence is dormant");
    assert_eq!(ids(&never).1, b.1, "Some(never()) is absent in the handshake identity");
    assert_eq!(armed.palw_gen_close_twin_at(TWIN_AT - 1), PalwTirCloseTwinV1::Element);
    assert_eq!(armed.palw_gen_close_twin_at(TWIN_AT), PalwTirCloseTwinV1::Range);
    assert_eq!(PalwGenAdmissionRulesV1::at(&armed, TWIN_AT - 1).expect("gen in force").twin, PalwTirCloseTwinV1::Element);
    assert_eq!(PalwGenAdmissionRulesV1::at(&armed, TWIN_AT).expect("gen in force").twin, PalwTirCloseTwinV1::Range);
}

#[test]
fn it_is_refused_without_the_generative_fence_or_with_a_stale_mirror() {
    let mut p = palw_t12_release_v6_params();
    p.palw_gen_range_twin_v1 = Some(ForkActivation::new(TWIN_AT));
    p.sync_palw_gen_range_twin_v1();
    let e = p.validate_palw_v2().expect_err("no generative fence");
    assert!(e.to_string().contains("palw_gen_range_twin_v1 needs palw_gen_v1"), "{e}");
    let mut q = with_gen();
    q.palw_gen_range_twin_v1 = Some(ForkActivation::new(GEN_AT - 1));
    q.sync_palw_gen_range_twin_v1();
    assert!(q.validate_palw_v2().is_err(), "below the generative fence");
    let mut r = with_gen();
    r.palw_gen_range_twin_v1 = Some(ForkActivation::new(TWIN_AT));
    let e = r.validate_palw_v2().expect_err("the mirror was not written");
    assert!(e.to_string().contains("mirror"), "{e}");
}
