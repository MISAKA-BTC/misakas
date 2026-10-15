//! Fixed required-family routing. Only an accepted honest completion advances the
//! descriptor; speculative local completion cannot select another stage or new clock.
use kaspa_consensus_core::{
    Hash64,
    palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_staged_bundle_v1::{PalwStagedBundlePlanV1, PalwStagedFamilyV1},
    palw_staged_court_state_v1::{PalwStagedCertificateRefV1, PalwStagedOwnedCursorV1, PalwStagedOwnedProgressV1},
};

pub(super) enum BundleScheduleV1 {
    Control(PalwConstraintStreamMoveV1),
    Inventory { offset: u64, maximum: u64 },
    Family { family: PalwStagedFamilyV1, progress: PalwStagedOwnedProgressV1 },
    Fault(PalwStagedCertificateRefV1),
    Complete,
}

pub(super) fn honest_complete_v1(cursor: Option<&PalwStagedOwnedCursorV1>) -> bool {
    matches!(
        cursor,
        Some(
            PalwStagedOwnedCursorV1::Constraint { completed: Some(false), .. }
                | PalwStagedOwnedCursorV1::Tokenizer { completed: Some(false), .. }
                | PalwStagedOwnedCursorV1::Prefix { completed: Some(false), .. }
                | PalwStagedOwnedCursorV1::PublicPrefix { pending: None, completed: Some(false), .. }
        )
    )
}
fn cursor_family_v1(cursor: &PalwStagedOwnedCursorV1) -> PalwStagedFamilyV1 {
    match cursor {
        PalwStagedOwnedCursorV1::Constraint { .. } => PalwStagedFamilyV1::Constraint,
        PalwStagedOwnedCursorV1::Tokenizer { .. } => PalwStagedFamilyV1::Tokenizer,
        PalwStagedOwnedCursorV1::Prefix { .. } | PalwStagedOwnedCursorV1::PublicPrefix { .. } => PalwStagedFamilyV1::PublicPrefix,
    }
}
fn movement(
    progress: &PalwStagedOwnedProgressV1,
    seed_id: Hash64,
    action: PalwConstraintStreamActionV1,
) -> PalwConstraintStreamMoveV1 {
    PalwConstraintStreamMoveV1 {
        version: 1,
        claim: progress.claim,
        reserver: progress.reserver,
        reserved_daa: progress.reserved_daa,
        seed_id,
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action,
        signature: Vec::new(),
    }
}
pub(super) fn request_v1(
    progress: &PalwStagedOwnedProgressV1,
    unopened: Option<&PalwStagedBundlePlanV1>,
) -> Result<Option<BundleScheduleV1>, String> {
    if progress.seed_id.is_none() {
        let Some(plan) = unopened else { return Ok(None) };
        plan.check_v1()?;
        if progress.seed_id.is_some()
            || progress.cursor.is_some()
            || progress.now_daa >= plan.terms.open_until_daa
            || (progress.terms.accepted_daa, progress.terms.open_until_daa) != (plan.terms.accepted_daa, plan.terms.open_until_daa)
        {
            return Err("bundle open changes immutable accepted quote, owner slot or original admission clock".into());
        }
        return Ok(Some(BundleScheduleV1::Control(movement(
            progress,
            plan.id_v1(progress.claim),
            PalwConstraintStreamActionV1::BundleOpen { plan: plan.clone() },
        ))));
    }
    let Some(bundle) = progress.bundle.as_ref() else { return Ok(None) };
    bundle.plan.check_v1()?;
    if bundle.bundle_id != bundle.plan.id_v1(progress.claim)
        || progress.now_daa < progress.reserved_daa
        || progress.now_daa >= bundle.plan.terms.collect_until_daa
    {
        return Err("bundle continuation changes literal paid identity or frozen horizon".into());
    }
    if bundle.plan.inventory.is_some() != bundle.inventory.is_some() {
        return Err("bundle continuation loses its required authenticated inventory".into());
    }
    if let Some((offset, maximum, sealed, fault)) = bundle.inventory {
        if fault {
            if !sealed {
                return Err("inventory fault lacks accepted exact EOF".into());
            }
            return Ok(Some(BundleScheduleV1::Fault(PalwStagedCertificateRefV1 {
                claim: progress.claim,
                reserver: progress.reserver,
                reserved_daa: progress.reserved_daa,
                seed_id: bundle.bundle_id,
            })));
        }
        if !sealed {
            let plan = bundle.plan.inventory.as_ref().ok_or("accepted inventory lacks immutable paid descriptor")?;
            let until = bundle
                .plan
                .terms
                .open_until_daa
                .checked_add(plan.envelope.inclusion_daa)
                .ok_or("inventory original horizon overflow")?;
            if maximum != plan.maximum_bytes || offset > maximum || bundle.active_index.is_some() || progress.now_daa >= until {
                return Err("inventory continuation changes original maximum, activation or paid clock".into());
            }
            return Ok(Some(BundleScheduleV1::Inventory { offset, maximum }));
        }
    }
    let next = match bundle.active_index {
        None => {
            if progress.cursor.is_some() || progress.seed_id != Some(bundle.bundle_id) {
                return Err("unactivated bundle substitutes a child cursor or seed".into());
            }
            0
        }
        Some(index) => {
            let stage = bundle.plan.stages.get(index as usize).ok_or("bundle active index exceeds immutable descriptors")?;
            if progress.terms != stage.terms
                || progress.seed_id.is_none()
                || progress.cursor.is_none()
                || progress.cursor.as_ref().is_some_and(|cursor| cursor_family_v1(cursor) != stage.family)
            {
                return Err("bundle active child has another exact terms/seed/cursor".into());
            }
            if !honest_complete_v1(progress.cursor.as_ref()) {
                return Ok(Some(BundleScheduleV1::Family { family: stage.family, progress: progress.clone() }));
            }
            index as usize + 1
        }
    };
    let Some(stage) = bundle.plan.stages.get(next) else { return Ok(Some(BundleScheduleV1::Complete)) };
    if progress.now_daa >= stage.terms.open_until_daa {
        return Err("next required family missed original fixed acquisition deadline".into());
    }
    if let Some(seed) = bundle.pending_seed {
        return Ok(Some(BundleScheduleV1::Control(movement(
            progress,
            progress.seed_id.unwrap_or(bundle.bundle_id),
            PalwConstraintStreamActionV1::ActivateStage {
                previous_seed: progress.seed_id.unwrap_or(bundle.bundle_id),
                next_seed: seed,
            },
        ))));
    }
    let mut child = progress.clone();
    child.terms = stage.terms;
    child.seed_id = None;
    child.cursor = None;
    child.prefix_page_root = None;
    Ok(Some(BundleScheduleV1::Family { family: stage.family, progress: child }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_rendering_resources_v1::{PalwStagedPublicCourtConfigV1, PalwTokenizerResourcePlanV1},
        palw_staged_bundle_v1::PalwStagedBundleProgressV1,
        palw_state_v2::PalwBondKeyV2,
        tx::TransactionOutpoint,
    };
    #[test]
    fn stage_transition_requires_rooted_honest_and_never_restarts_original_deadline() {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let envelope = PalwTokenizerResourcePlanV1::derive_with_key_bound(128, 2, config.prices)
            .unwrap()
            .court_envelope(19, config.prices)
            .unwrap();
        let plan = PalwStagedBundlePlanV1::derive_v1(
            100,
            vec![(PalwStagedFamilyV1::Constraint, envelope), (PalwStagedFamilyV1::Tokenizer, envelope)],
            config,
        )
        .unwrap();
        let claim = Hash64::from_u64_word(1);
        let seed = Hash64::from_u64_word(2);
        let mut p = PalwStagedOwnedProgressV1 {
            snapshot_block: kaspa_consensus_core::BlockHash::default(),
            now_daa: 110,
            claim,
            reserver: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(9), 0)),
            reserved_daa: 105,
            terms: plan.stages[0].terms,
            prefix_custodians: None,
            prefix_page_root: None,
            seed_id: Some(seed),
            bundle: Some(PalwStagedBundleProgressV1 {
                bundle_id: plan.id_v1(claim),
                plan: plan.clone(),
                active_index: Some(0),
                pending_seed: None,
                inventory: None,
            }),
            cursor: Some(PalwStagedOwnedCursorV1::Constraint {
                phase: PalwConstraintStreamPhaseV1::Complete,
                eog_offset: None,
                rendering: None,
                next_rendering_id: None,
                completed: None,
            }),
        };
        assert!(matches!(
            request_v1(&p, None).unwrap(),
            Some(BundleScheduleV1::Family { family: PalwStagedFamilyV1::Constraint, .. })
        ));
        if let Some(PalwStagedOwnedCursorV1::Constraint { completed, .. }) = &mut p.cursor {
            *completed = Some(true)
        }
        assert!(matches!(
            request_v1(&p, None).unwrap(),
            Some(BundleScheduleV1::Family { family: PalwStagedFamilyV1::Constraint, .. })
        ));
        if let Some(PalwStagedOwnedCursorV1::Constraint { completed, .. }) = &mut p.cursor {
            *completed = Some(false)
        }
        match request_v1(&p, None).unwrap().unwrap() {
            BundleScheduleV1::Family { family: PalwStagedFamilyV1::Tokenizer, progress } => {
                assert_eq!(progress.terms, plan.stages[1].terms);
                assert!(progress.seed_id.is_none());
                assert!(progress.cursor.is_none());
            }
            _ => panic!("honest active completion must select next fixed descriptor"),
        }
        p.now_daa = plan.stages[1].terms.open_until_daa;
        assert!(request_v1(&p, None).is_err());
    }
}
