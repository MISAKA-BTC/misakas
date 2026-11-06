//! RFC-0037 adapters. The serving state is a pinned, validated snapshot; the
//! importing state uses the full import's selected-chain witness and rules.
use super::*;
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeStateCarriageV1},
    palw_row_stream_v1::PalwRowInventoryV1,
    palw_state_v2::{PalwStateCarriageV2, PalwStateV2Error},
};

impl VirtualStateProcessor {
    pub fn write_pruning_point_palw_row_generation_v1_impl(
        &self,
        point: BlockHash,
        suffix: &mut dyn std::io::Read,
        suffix_len: u64,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<Option<u64>> {
        let Some(params) = self.palw_state_params_v2.as_ref() else { return Ok(None) };
        self.palw_state_v2_store.read().write_pruning_snapshot_row_generation_v1(params, point, suffix, suffix_len, writer)
    }

    pub fn pruning_point_palw_row_witness_root_v1_impl(&self, point: BlockHash) -> Option<Hash64> {
        // Row v1 authenticates the flat state preimage. Other header forms keep
        // the existing carriage path instead of reinterpreting a commitment.
        if self.palw_fork_choice_commitment_v1.is_some() {
            return None;
        }
        let witness = self.pruning_point_witness_child(point)?;
        Some(self.headers_store.get_header(witness).ok()?.palw_state_root)
    }

    pub fn palw_validate_row_stream_snapshot_v1_impl(
        &self,
        point: BlockHash,
        snapshot: &PalwStateCarriageV2,
        inventory: &PalwRowInventoryV1,
    ) -> Result<(), NativeDecodeErrorV1<String>> {
        let unavailable = |why: &str| {
            NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<PalwStateCarriageV2>(
                "selected-chain row snapshot context",
                why,
            ))
        };
        let params = self.palw_state_params_v2.as_ref().ok_or_else(|| unavailable("PALW state parameters unavailable"))?;
        if self.palw_fork_choice_commitment_v1.is_some() {
            return Err(unavailable("nonflat header commitment uses carriage IBD"));
        }
        let witness = self.pruning_point_witness_child(point).ok_or_else(|| unavailable("selected-chain witness unavailable"))?;
        let header = self.headers_store.get_header(witness).map_err(|_| unavailable("selected-chain witness header unavailable"))?;
        if inventory.state_root() != header.palw_state_root {
            return Err(unavailable("selected-chain witness changed since row authentication; retry the transfer"));
        }
        let owned = NativeStateCarriageV1::try_from_clone_v1(snapshot).map_err(NativeDecodeErrorV1::Resource)?;
        let _checked = owned
            .try_restore_checked_v1(
                point,
                params,
                Some(header.palw_state_root),
                self.palw_uncertified_weightless_at(header.daa_score),
                self.palw_canonical_work_daa,
                |state| {
                    kaspa_consensus_core::palw_native39_frontier_v1::validate_native39_frontier_context_v1(
                        state,
                        params,
                        self.palw_exec_quantum_maturity_daa,
                    )?;
                    let boundary_blue = self.headers_store.get_blue_score(point).map_err(|why| {
                        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwStateCarriageV2>(
                            "row snapshot pruning boundary",
                            &why.to_string(),
                        ))
                    })?;
                    self.validate_native_checkpoint_headers_v1(state, boundary_blue)?;
                    match inventory.matches_state(state) {
                        Ok(true) => Ok(()),
                        Ok(false) => Err(PalwStateV2Error::CarriageInconsistent(
                            "row stream omitted or substituted a committed collection".into(),
                        )),
                        Err(why) => Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwStateCarriageV2>(
                            "canonical row inventory",
                            &why.to_string(),
                        ))),
                    }
                },
            )
            .map_err(|why| match why {
                NativeDecodeErrorV1::Resource(why) => NativeDecodeErrorV1::Resource(why),
                NativeDecodeErrorV1::Decode(why) => NativeDecodeErrorV1::Decode(why.to_string()),
            })?;
        // Read-only validation only. The caller must still pass the original
        // pruning-import session to the ordinary atomic full import.
        Ok(())
    }
}
