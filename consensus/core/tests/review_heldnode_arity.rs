//! **Regression (feat/t12-aheld-node review, residual 1): the dissection arity a node's root claim
//! declares is the one the acceptance layer derives.**
//!
//! The processor refuses any root claim whose arity is not
//! `palw_court_params_held_at_v2(bundle, kary_at(daa), held_context_at(daa))` (its
//! `palw_court_params_at`). The panel's DENSE root claim derived it with `palw_court_params_at_v2`,
//! never held-aware: on testnet-12 (the held regime from genesis) that derivation finds no arity at
//! all, so a non-held fused class registered there answered no dissection and was convicted by the
//! silence. The node now derives it the processor's way (kaspad's panel pins the call). This test
//! pins the numbers the two derivations give on every shipped preset: testnet-12 held-aware 4 (the
//! plain derivation none — the defect), testnet-11 2 on both sides of its regime's height (7,100) by
//! either derivation (so its node is unchanged), devnet 2 by either, mainnet not V2.

use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, palw_court_params_at_v2, palw_court_params_held_at_v2};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// `(plain, held-aware)` arities at `daa`: what the dense path used to derive, and what the
/// processor (and now the node) derives.
fn arities(p: &Params, daa: u64) -> (Result<u8, PalwCourtV2Error>, Result<u8, PalwCourtV2Error>) {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("a V2 network") };
    let kary = p.palw_kary_court_active_at(daa);
    let held = p.palw_held_context_active_at(daa);
    (
        palw_court_params_at_v2(bundle, kary).map(|c| c.dissection_arity()),
        palw_court_params_held_at_v2(bundle, kary, held).map(|c| c.dissection_arity()),
    )
}

#[test]
fn review_heldnode_the_nodes_dissection_arity_is_the_processors_on_every_preset() {
    let t12 = palw_t12_shipped_params();
    for daa in [0u64, 7_099, 7_100, 10_000, 1_000_000] {
        let (plain, held_aware) = arities(&t12, daa);
        assert_eq!(held_aware, Ok(4), "testnet-12 at {daa}: the processor's arity");
        assert!(
            matches!(plain, Err(PalwCourtV2Error::NoAdmissibleArity { window_court: 3_000 })),
            "testnet-12 at {daa}: the plain derivation finds none — the dense path's old spelling: {plain:?}"
        );
    }
    let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    assert!(!t11.palw_held_context_active_at(7_099) && t11.palw_held_context_active_at(7_100), "testnet-11's regime at 7,100");
    for daa in [0u64, 7_099, 7_100, 10_000, 1_000_000] {
        assert_eq!(arities(&t11, daa), (Ok(2), Ok(2)), "testnet-11 at {daa}: 2 by either derivation, either side of the regime");
    }
    let devnet = Params::from(NetworkId::new(NetworkType::Devnet));
    for daa in [0u64, 7_100, 1_000_000] {
        assert_eq!(arities(&devnet, daa), (Ok(2), Ok(2)), "devnet at {daa}");
    }
    let mainnet = Params::from(NetworkId::new(NetworkType::Mainnet));
    assert!(!matches!(mainnet.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)), "mainnet runs no V2 court");
}
