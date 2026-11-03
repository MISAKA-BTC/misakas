//! Header context for the consensus-derived archived native settlement row.
//! Historical references below the import boundary do not create EVM heads;
//! positive head certification still requires the ordinary executed-chain walk.
use super::*;
use kaspa_consensus_core::{
    palw_native_resource_v1::NativeCoverageV1,
    palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateV2Error},
};

impl VirtualStateProcessor {
    pub(crate) fn validate_reader_native_checkpoint_v1(
        &self,
        state: &PalwChainStateV2,
        params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        boundary_blue: u64,
    ) -> kaspa_database::prelude::StoreResult<()> {
        kaspa_consensus_core::palw_native39_frontier_v1::validate_native39_frontier_context_v1(
            state,
            params,
            self.palw_exec_quantum_maturity_daa,
        )
        .and_then(|()| self.validate_native_checkpoint_headers_v1(state, boundary_blue))
        .map_err(|why| match why {
            PalwStateV2Error::NativeResource(resource) => {
                kaspa_database::prelude::StoreError::LocalResourceUnavailable(resource.to_string())
            }
            other => kaspa_database::prelude::StoreError::DataInconsistency(other.to_string()),
        })
    }

    pub(crate) fn validate_native_checkpoint_headers_v1(
        &self,
        state: &PalwChainStateV2,
        boundary_blue: u64,
    ) -> Result<(), PalwStateV2Error> {
        kaspa_consensus_core::palw_state_v2::validate_fact_archives_context_v1(state, &self.native_redemption_params_v33)?;
        let Some(row) = read_native39_settlement_checkpoint_v1(state)? else { return Ok(()) };
        let invalid = |why: &str| PalwStateV2Error::CarriageInconsistent(why.into());
        let missing = |why: &str| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwChainStateV2>(
                "native archival checkpoint header context",
                why,
            ))
        };
        let Some(retirement) = self.palw_dns_retirement else { return Err(invalid("native archived settlement policy absent")) };
        if row.ruleset != self.palw_native_ruleset_id
            || row.policy_id != retirement.settlement.id()
            || row.retirement_daa != retirement.activation.daa_score()
            || row.effect.daa_score < self.evm_activation_daa_score
        {
            return Err(invalid("native archived settlement policy/ruleset differs from this node"));
        }
        let tip = state.last_point().ok_or_else(|| invalid("native checkpoint has no state point"))?;
        let check_point = |point: &PalwBlockContextV2| -> Result<(), PalwStateV2Error> {
            if point.blue_score < boundary_blue {
                return Ok(());
            }
            let header = self
                .headers_store
                .get_header(point.block)
                .map_err(|_| missing("referenced header at or above pruning boundary is unavailable"))?;
            if header.daa_score != point.daa_score || header.blue_score != point.blue_score {
                return Err(invalid("native checkpoint point clock differs from its actual header"));
            }
            let ancestor = point.block == tip.block
                || self
                    .reachability_service
                    .try_is_dag_ancestor_of(point.block, tip.block)
                    .map_err(|_| missing("native checkpoint ancestry data is unavailable"))?;
            if !ancestor {
                return Err(invalid("native checkpoint reference is outside this state point's DAG ancestry"));
            }
            Ok(())
        };
        for point in [&row.effect, &row.point, &row.parent] {
            check_point(point)?;
        }
        if row.frontier != row.effect.block || row.frontier_blue != row.effect.blue_score {
            return Err(invalid("archived frontier is not the exact irreversible native effect"));
        }
        if row.point.blue_score >= boundary_blue {
            let header =
                self.headers_store.get_header(row.point.block).map_err(|_| missing("native certification header is unavailable"))?;
            if header.palw_state_root != row.parent_committed_root_v1(self.palw_fork_choice_commitment_v1)
                || !header.direct_parents().contains(&row.parent.block)
            {
                return Err(invalid("native archival checkpoint parent commitment/header differs"));
            }
            if row.parent.blue_score >= boundary_blue {
                let parent = self
                    .ghostdag_store
                    .get_selected_parent(row.point.block)
                    .map_err(|_| missing("native certification selected-parent data is unavailable"))?;
                if parent != row.parent.block {
                    return Err(invalid("native certificate parent is not the actual selected parent"));
                }
            }
        }
        Ok(())
    }
}
