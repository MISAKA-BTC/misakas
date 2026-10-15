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
pub const PALW_STAGED_ROUTE_FORMS_V1: [(&str, PalwLifecycleKindFenceV1); 3] = [
    ("PalwConstraintStreamActionV1::FloatSource*", PalwLifecycleKindFenceV1::StagedFloatConstraintV1),
    ("PalwAuxiliaryPublicCourtSeedV1::FloatConstraint", PalwLifecycleKindFenceV1::StagedFloatConstraintV1),
    ("PalwConstraintStreamActionV1::NativeSelected*", PalwLifecycleKindFenceV1::StagedTirRouteV1),
];

/// The newer fence that owns `object`'s appended staged-route form, if it carries one.
pub fn palw_staged_route_form_fence_v1(object: &PalwConsensusObjectV2) -> Option<PalwLifecycleKindFenceV1> {
    use crate::palw_constraint_stream_carriage_v1::{PalwAuxiliaryPublicCourtSeedV1 as Aux, PalwConstraintStreamActionV1 as A};
    match object {
        PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } => match &movement.action {
            A::FloatSourceOpen { .. } | A::FloatSourceAnswer(_) | A::FloatSourceNominate { .. } | A::FloatSourceCollect => {
                Some(PalwLifecycleKindFenceV1::StagedFloatConstraintV1)
            }
            A::NativeSelectedDemand { .. } | A::NativeSelectedAnswer(_) => Some(PalwLifecycleKindFenceV1::StagedTirRouteV1),
            _ => None,
        },
        PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed } => match &seed.auxiliary {
            Some(Aux::FloatConstraint { .. }) => Some(PalwLifecycleKindFenceV1::StagedFloatConstraintV1),
            _ => None,
        },
        _ => None,
    }
}
