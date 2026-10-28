//! **Which staged-route forms a newer fence owns** (A-2 uniform).
//!
//! The staged public court's kinds (tags 165–167) are owned by `palw_staged_public_court_v1`.
//! Two later routes append forms INSIDE those kinds:
//!
//! * `palw_staged_float_constraint_v1` — the greedy Float32 source actions
//!   (`FloatSource*`) and the `FloatConstraint` auxiliary seed;
//! * `palw_staged_tir_route_v1` — the native-context (no binding) TIR selected-source actions
//!   (`NativeSelected*`). Its native seed is a kind of its own (tag 168,
//!   `PublicCourtStreamOpenedNativeV1`), owned through the new-kind table instead.
//!
//! A build that knows the staged court but not a route cannot decode the route's form. So, below
//! the route's own fence, this build reads the form exactly as such a build does — undecodable,
//! riding unjudged — through the lifecycle owner map
//! ([`crate::palw_lifecycle_objects_v2::palw_lifecycle_kind_owner_v1`]); past it, the form meets
//! its rules. The fold refuses it again as defence in depth.
use crate::palw_lifecycle_objects_v2::PalwLifecycleKindFenceV1;
use crate::palw_state_v2::PalwConsensusObjectV2;

/// The appended forms and their owning fences — the ledger the fence-ownership audit reads.
pub const PALW_STAGED_ROUTE_FORMS_V1: [(&str, PalwLifecycleKindFenceV1); 4] = [
    ("PalwConstraintStreamActionV1::FloatSource*", PalwLifecycleKindFenceV1::StagedFloatConstraintV1),
    ("PalwAuxiliaryPublicCourtSeedV1::FloatConstraint", PalwLifecycleKindFenceV1::StagedFloatConstraintV1),
    ("PalwConstraintStreamActionV1::NativeSelected*", PalwLifecycleKindFenceV1::StagedTirRouteV1),
    ("PalwConstraintStreamActionV1::NativeTirPrefix*", PalwLifecycleKindFenceV1::StagedTirRouteV1),
];

/// The newer fence that owns `object`'s appended staged-route form, if it carries one.
pub fn palw_staged_route_form_fence_v1(object: &PalwConsensusObjectV2) -> Option<PalwLifecycleKindFenceV1> {
    use crate::palw_constraint_stream_carriage_v1::{PalwAuxiliaryPublicCourtSeedV1 as Aux, PalwConstraintStreamActionV1 as A};
    match object {
        PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } => match &movement.action {
            A::FloatSourceOpen { .. } | A::FloatSourceAnswer(_) | A::FloatSourceNominate { .. } | A::FloatSourceCollect => {
                Some(PalwLifecycleKindFenceV1::StagedFloatConstraintV1)
            }
            A::NativeSelectedDemand { .. }
            | A::NativeSelectedAnswer(_)
            | A::NativeTirPrefixAnswer(_)
            | A::NativeTirPrefixCollect => Some(PalwLifecycleKindFenceV1::StagedTirRouteV1),
            _ => None,
        },
        PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed } => match &seed.auxiliary {
            Some(Aux::FloatConstraint { .. }) => Some(PalwLifecycleKindFenceV1::StagedFloatConstraintV1),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::ForkActivation;
    use crate::palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1 as A, PalwConstraintStreamMoveV1};
    use crate::palw_lifecycle_objects_v2::PalwLifecycleKindFencesV1;

    fn movement(action: A) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::PublicCourtStreamMoveV1 {
            movement: Box::new(PalwConstraintStreamMoveV1 {
                version: 1,
                claim: crate::Hash64::from_u64_word(1),
                reserver: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::Hash64::from_u64_word(2), 0)),
                reserved_daa: 3,
                seed_id: crate::Hash64::from_u64_word(4),
                phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt,
                action,
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            }),
        }
    }

    /// A-2: where only the staged court is armed, a build that knows the court but not the route
    /// cannot decode the route's form — so this build reads it the same way until the route's fence.
    #[test]
    fn a_route_form_rides_unjudged_until_its_own_fence_even_where_the_staged_court_is_armed() {
        let plain = movement(A::FlatSourceCollect);
        let float = movement(A::FloatSourceCollect);
        assert_eq!(palw_staged_route_form_fence_v1(&plain), None);
        assert_eq!(palw_staged_route_form_fence_v1(&float), Some(PalwLifecycleKindFenceV1::StagedFloatConstraintV1));
        let court = PalwLifecycleKindFencesV1::default().with(PalwLifecycleKindFenceV1::StagedPublicCourtV1, Some(ForkActivation::new(10)));
        assert!(court.kind_in_force_at(&plain, 10));
        assert!(!court.kind_in_force_at(&float, u64::MAX), "the court's fence never arms a route's form");
        let armed = court.with(PalwLifecycleKindFenceV1::StagedFloatConstraintV1, Some(ForkActivation::new(20)));
        assert!(!armed.kind_in_force_at(&float, 19));
        assert!(armed.kind_in_force_at(&float, 20));
        // `Params` resolves the route's fence only where the staged court is armed too, at the later height.
        let mut p = crate::config::params::Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        assert_eq!(p.palw_lifecycle_kind_fences_v1().activation(PalwLifecycleKindFenceV1::StagedFloatConstraintV1), None);
        p.palw_staged_float_constraint_v1 = Some(ForkActivation::new(30));
        p.palw_staged_public_court_v1 = None;
        assert_eq!(p.palw_lifecycle_kind_fences_v1().activation(PalwLifecycleKindFenceV1::StagedFloatConstraintV1), None);
    }
}
