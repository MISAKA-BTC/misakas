//! Object authentication and admission for the existing structural family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_structural_object_v1(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            // (tag 108): the acceptance walk replaces the envelope by its registration before this gate; one that reaches it was
            // not unwrapped (a direct caller of the gate), and is refused.
            Obj::SignedRegistrationV1 { .. } => {
                return Err("a signed registration envelope is unwrapped by the acceptance walk, not judged as itself".to_string());
            }
            // Phase F's reserved tags 84 and 85 carry an uninhabited payload: no such object can be decoded.
            Obj::ReservedPipelineDa84(never) | Obj::ReservedPipelineDa85(never) => match *never {},
            _ => unreachable!("structural validator received another object family"),
        }
    }
}
