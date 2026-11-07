//! ADR-0076 on the networks that ship: every registered class's attempt lane is seeded from its
//! own share and its own counted work, and no two classes of different cost share a seed.
//!
//! The unit tests in `palw_class_daa` hold the arithmetic. These hold the WIRING — that the
//! genesis assembly actually applies it, against the effective share table rather than the
//! declared one, on the preset a node boots. Relaunch 5d passed every arithmetic test in the tree
//! and still shipped one target for all three classes, because nothing asserted the assembled
//! bundle.

use kaspa_consensus_core::config::params::{Params, palw_rc_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_class_daa::attempt_target_seed_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2, apply_palw_transition_v2, palw_max_exposure_pwu_of_rule_v1,
};

/// `(class id, effective share‰, pwu_per_inference, seeded target)` for every class a bundle
/// registers — the share read from the table the registrations themselves produce, which is the
/// whole point: the floor DECLARES 1000‰ and HOLDS 22‰ once the tiers dilute it.
fn table(params: &Params) -> Vec<(String, u16, u64, u128)> {
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Vec::new();
    };
    let ctx = PalwBlockContextV2 { block: Default::default(), daa_score: 0, blue_score: 0, subsidy: 0 };
    let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &bundle.state, &ctx, &bundle.genesis_objects, None)
        .expect("a shipped bundle's genesis registrations apply");
    bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, .. } => Some((
                format!("{class_id}")[..8].to_string(),
                state.class_share_permille(class_id).expect("a registered class holds a share"),
                palw_max_exposure_pwu_of_rule_v1(pwu_rule),
                *initial_target,
            )),
            _ => None,
        })
        .collect()
}

fn shipped() -> Vec<(String, Params)> {
    let mut out = vec![("testnet-11 (RC)".to_string(), palw_rc_shipped_params())];
    for net in [NetworkId::new(NetworkType::Devnet), NetworkId::with_suffix(NetworkType::Testnet, 11)] {
        out.push((net.to_string(), net.into()));
    }
    out
}

#[test]
fn every_shipped_class_carries_the_seed_its_own_share_and_work_derive() {
    for (name, params) in shipped() {
        let rows = table(&params);
        assert!(!rows.is_empty(), "{name} registers no class");
        for (id, share, pwu, target) in rows {
            assert_eq!(
                target,
                attempt_target_seed_v1(share, pwu),
                "{name}: class {id} at {share}‰ and {pwu} pwu does not carry its derived seed"
            );
        }
    }
}

/// The regression that names the live failure: on testnet-11 the floor and the two model tiers
/// must not share a target. Relaunch 5d shipped `MAX/2` for all three and the floor produced 249
/// blocks in an hour against the tiers' zero.
#[test]
fn the_testnet11_tiers_are_not_priced_like_the_floor() {
    let rows = table(&palw_rc_shipped_params());
    assert_eq!(rows.len(), 3, "testnet-11 registers the floor and two model tiers");
    let floor = rows.iter().min_by_key(|(_, share, pwu, _)| (*share as u128) * (*pwu as u128)).expect("a row").clone();
    for (id, share, pwu, target) in rows.iter().filter(|r| r.0 != floor.0) {
        assert!(*target > floor.3, "tier {id} ({share}‰, {pwu} pwu) is not priced above the floor — this is the 5d table");
        // The floor draws hundreds of times faster per unit of share, so the gap is orders of
        // magnitude, not a nudge.
        assert!(*target / floor.3 > 100, "tier {id} sits only {}× above the floor", *target / floor.3);
    }
}
