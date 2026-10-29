//! Source controls for the native carrier/fold. No execution result is implied.
use super::*;
use crate::palw_gdn_court_v2::{PalwGdnCourtAccusationV2, PalwGdnCourtLimitsV2};

#[test]
fn versioned_gdn_carrier_reaches_real_fold_and_existing_delta_reversal() {
    for wrong in [false, true] {
        let refutation = crate::palw_step_refute::memory_v2::tests::genesis_evidence(wrong);
        let (p, state, claim_id) = leaf_demand_setup(&refutation.step.binding, Hash64::default());
        let accusation = PalwGdnCourtAccusationV2 {
            version: 2,
            claim: claim_id,
            execution_root: refutation.step.binding.committed_execution_root,
            trace_root: h64(HELD_TRACE_ROOT),
            executor_bond: bond_key(1),
            accuser_bond: bond_key(2),
            leaf_index: refutation.step.output_opening.leaf_index,
            refutation,
            artifact_openings: vec![], // float GdnCore reads no artifact
            signature: vec![1; 8],     // actual acceptance verifies the registry key
        };
        let limit = accusation.encoded_bytes().unwrap();
        let object = PalwConsensusObjectV2::GdnStepAccusedV2 { accusation: Box::new(accusation) };
        assert_eq!(borsh::to_vec(&object).unwrap()[0], 243);
        assert!(
            held_apply(&state, &p, &ctx(4, 103, 4), std::slice::from_ref(&object), None, &held_extras()).is_err(),
            "old fold extras cannot read the new carrier"
        );
        let mut extras = PalwTransitionExtrasV1 {
            memory_routing_court: Some(PalwGdnCourtLimitsV2 { ladder: 1 << 26, max_close_bytes: limit }),
            ..held_extras()
        };
        let observer = crate::palw_court_observation_v1::CourtObservationGuardV1::enter(ctx(4, 103, 4).block);
        let (after, delta) = held_apply(&state, &p, &ctx(4, 103, 4), std::slice::from_ref(&object), None, &extras)
            .expect("the actual deterministic GDN terminal reaches settlement");
        let observed = observer.finish().expect("bounded successful native fold");
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].claim_id, claim_id);
        assert_eq!(observed[0].object_tag, 243);
        assert!(observed[0].adjudication_and_settlement_micros.is_some());
        assert_eq!(observed[0].route, "GdnStep");
        assert_eq!(observed[0].verdict, if wrong { "ExecutorGuilty" } else { "FalseAccusation" });
        let unobserved = held_apply(&state, &p, &ctx(4, 103, 4), std::slice::from_ref(&object), None, &extras).unwrap().0;
        assert_eq!(after.state_root(), unobserved.state_root(), "telemetry does not change consensus state");
        if wrong {
            assert!(matches!(
                after.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }
            ));
            assert!(after.bond(&bond_key(1)).unwrap().collateral < state.bond(&bond_key(1)).unwrap().collateral);
        } else {
            assert_eq!(
                after.claim(&claim_id).unwrap().phase,
                state.claim(&claim_id).unwrap().phase,
                "a match neither Finalizes nor approves the claim"
            );
            assert!(after.bond(&bond_key(2)).unwrap().collateral < state.bond(&bond_key(2)).unwrap().collateral);
        }
        assert_eq!(revert_delta_v2(&after, &delta, &p).unwrap().state_root(), state.state_root());
        assert_eq!(apply_delta_v2(&state, &delta, &p).unwrap().state_root(), after.state_root());
        // A successful inner court can still be reverted by its enclosing batch.
        // The builder restores telemetry with the exact same checkpoint as state.
        let observer = crate::palw_court_observation_v1::CourtObservationGuardV1::enter(ctx(4, 103, 4).block);
        let mut builder = TransitionBuilder::new(&state, &p, false, false, false, true, &extras);
        let saved = builder.try_checkpoint().unwrap();
        apply_object(&mut builder, &ctx(4, 103, 4), &object).unwrap();
        assert!(crate::palw_court_observation_v1::checkpoint_v1().unwrap().1 > 0);
        builder.restore(saved);
        assert_eq!(builder.state.state_root(), state.state_root());
        assert!(observer.finish().unwrap().is_empty(), "a restored successful inner verdict is not observable");
        drop(builder);
        extras.memory_routing_court.as_mut().unwrap().max_close_bytes = limit - 1;
        assert!(held_apply(&state, &p, &ctx(4, 103, 4), &[object], None, &extras).is_err());
    }
}
