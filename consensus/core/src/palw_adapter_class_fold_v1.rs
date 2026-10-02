//! **RFC-0001 §2.10 (ADR-0163) in the fold: `AdapterClassListed` (tag 94)** — a child module of `palw_state_v2`, as
//! the held-close fold is, so it reads the builder and writes the registry only through its one writer.
//!
//! The fold's half is the cheap, state-local second lock; the heavy half (the composite and family rules, every
//! terminal close carriable in the composite form) is the acceptance layer's
//! ([`PalwChainStateV2::adapter_class_acceptance_v1`], one a block). **Every refusal comes before the write.**
//!
//! What it writes is `improvement_composite_classes` (journaled `ImprovementCompositeClass`, delta 99) — the very
//! record a governed line's composite candidate writes: the only classes a composite opening may name, and the record
//! a seat proves possession of the adapter section against (RFC-0004 §6.7). Nothing else moves: the class was priced
//! and registered by its IR registration, and the listing adds no bond, no pool and no reward.

use super::*;
use crate::palw_adapter_class_v1::{PalwAdapterClassListingV1, palw_adapter_listing_shape_v1};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::AdapterClassRefused(why.into())
}

pub(super) fn apply_adapter_class_listed_v1(
    builder: &mut TransitionBuilder<'_>,
    _ctx: &PalwBlockContextV2,
    payload: &PalwAdapterClassListingV1,
    lister: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    palw_adapter_listing_shape_v1(payload).map_err(refused)?;
    if !matches!(builder.state.bond(lister).map(|b| &b.status), Some(PalwBondStatusV2::Active)) {
        return Err(refused("the lister is not an Active bond"));
    }
    let class_root = builder.state.class(&payload.class_id).ok_or_else(|| refused("the listed class is not registered"))?.artifact_root;
    if builder.state.tir_class_v1(&payload.class_id).is_none() {
        return Err(refused("the listed class is not an IR class"));
    }
    if class_root != payload.artifact.artifact_root() {
        return Err(refused("the composite reference is not the listed class's registered artifact"));
    }
    let parent = payload.artifact.parent_class;
    let parent_root = builder.state.class(&parent).ok_or_else(|| refused("the parent is not a registered class"))?.artifact_root;
    if builder.state.tir_class_v1(&parent).is_none() {
        return Err(refused("the parent is not an IR class"));
    }
    if parent_root != payload.artifact.parent_root {
        return Err(refused("the reference's parent root is not the parent's registered artifact root"));
    }
    if builder.state.improvement_composite_class(&payload.class_id).is_some() {
        return Err(refused("the class is already listed as a composite"));
    }
    builder.write_improvement_composite_class(payload.class_id, Some(payload.artifact));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_improve_composite_v1::PalwTirCompositeRefV1;
    use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
    use crate::tx::TransactionOutpoint;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn bond(b: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: h(b), index: 0 })
    }

    const PARENT: u8 = 0x10;
    const CHILD: u8 = 0x11;
    const LISTER: u8 = 3;

    fn reference() -> PalwTirCompositeRefV1 {
        PalwTirCompositeRefV1 { parent_class: h(PARENT), parent_root: h(0x21), adapter_root: h(0x22), p: 4 }
    }

    fn payload() -> PalwAdapterClassListingV1 {
        PalwAdapterClassListingV1 {
            class_id: h(CHILD),
            artifact: reference(),
            layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                version: 1,
                max_context: 8,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![],
                state_tiles: vec![],
            },
        }
    }

    fn params(armed: bool) -> PalwStateParamsV2 {
        let p = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
        bundle.state.clone().with_adapter_class_from_daa(armed.then_some(100))
    }

    /// A state with the parent and the child registered (the child's registered artifact is the composite's root).
    fn state() -> PalwChainStateV2 {
        let mut s = PalwChainStateV2::genesis();
        for class in [PARENT, CHILD] {
            s.tir_classes.insert(h(class), PalwTirClassRecordV1::test_row_v1(h(class)));
        }
        s.bonds.insert(
            bond(LISTER),
            palw_bond_state_from_registration_v2(&[LISTER], &[LISTER], 1_000_000_000_000_000, h(0x80), 0, Default::default()),
        );
        s
    }

    fn with_classes(mut s: PalwChainStateV2, parent_root: Hash64, child_root: Hash64) -> PalwChainStateV2 {
        for (class, root) in [(PARENT, parent_root), (CHILD, child_root)] {
            s.classes.insert(
                h(class),
                PalwClassStateV2 {
                    artifact_root: root,
                    slash_value_per_pwu: 3,
                    pwu_rule: PalwPwuRuleV2::MaxPerAttempt(100),
                    status: PalwClassStatusV2::Active,
                    registered_daa: 0,
                    registrant_bond: None,
                    fused_attention: false,
                },
            );
        }
        s
    }

    fn run(s: &PalwChainStateV2, p: &PalwStateParamsV2, payload: &PalwAdapterClassListingV1, lister: PalwBondKeyV2) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(s, p, false, false, false, false, &extras);
        let ctx = PalwBlockContextV2 { block: h(1), daa_score: 200, blue_score: 200, subsidy: 0 };
        apply_adapter_class_listed_v1(&mut b, &ctx, payload, &lister)?;
        Ok(b.checkpoint().0)
    }

    #[test]
    fn a_listing_records_the_composite_and_every_refusal_comes_before_the_write() {
        let p = params(true);
        let s = with_classes(state(), h(0x21), reference().artifact_root());
        let listed = run(&s, &p, &payload(), bond(LISTER)).expect("a registered composite is listed");
        assert_eq!(listed.improvement_composite_class(&h(CHILD)), Some(reference()));
        assert_eq!(listed.improvement_composite_classes_v1().count(), 1, "the registry the seats read");
        assert_ne!(listed.state_root(), s.state_root());
        assert!(run(&listed, &p, &payload(), bond(LISTER)).unwrap_err().to_string().contains("already listed"));
        assert!(run(&s, &p, &payload(), bond(9)).unwrap_err().to_string().contains("Active bond"), "a lister with no bond");
        let mut other_adapter = payload();
        other_adapter.artifact.adapter_root = h(0x99);
        assert!(run(&s, &p, &other_adapter, bond(LISTER)).unwrap_err().to_string().contains("registered artifact"), "not the class's artifact");
        let mut other_parent = payload();
        other_parent.artifact.parent_class = h(0x77);
        assert!(run(&s, &p, &other_parent, bond(LISTER)).is_err(), "a parent that is not registered");
        let wrong_root = with_classes(state(), h(0x55), reference().artifact_root());
        assert!(run(&wrong_root, &p, &payload(), bond(LISTER)).unwrap_err().to_string().contains("parent root"));
        let mut unregistered = payload();
        unregistered.class_id = h(0x66);
        assert!(run(&s, &p, &unregistered, bond(LISTER)).unwrap_err().to_string().contains("not registered"));
    }
}
