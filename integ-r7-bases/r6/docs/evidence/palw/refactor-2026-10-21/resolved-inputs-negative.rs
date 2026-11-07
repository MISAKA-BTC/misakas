mod production {
/// Immutable output of the existing production resolver at one block. The legacy
/// extras remain readable for the existing fold APIs, but cannot be mutated through
/// this boundary. Accepted-fact assembly explicitly takes ownership of those extras.
#[derive(Clone)]
pub(super) struct PalwResolvedBlockInputsV1 {
    point: kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    extras: kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
}
impl PalwResolvedBlockInputsV1 {
    fn at(point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
          extras: kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1) -> Self {
        Self { point: *point, extras }
    }
    pub(super) fn point(&self) -> &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        &self.point
    }
    pub(super) fn into_legacy_extras(self) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        self.extras
    }
}
impl std::fmt::Debug for PalwResolvedBlockInputsV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.extras, f)
    }
}
impl std::ops::Deref for PalwResolvedBlockInputsV1 {
    type Target = kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1;
    fn deref(&self) -> &Self::Target { &self.extras }
}

pub(super) fn make(point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
 extras: kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1) -> PalwResolvedBlockInputsV1 {
 PalwResolvedBlockInputsV1::at(point, extras)
}
}
fn main() {
let mut point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
 block: Default::default(), daa_score: 200, blue_score: 1, subsidy: 0,
};
let extras = kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {canonical_work_daa:Some(100),..Default::default()};
let mut resolved = production::make(&point, extras);
resolved.canonical_work_daa = None;
}
