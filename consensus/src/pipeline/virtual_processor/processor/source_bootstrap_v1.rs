//! Own selected-parent branch observations for the finite first-source epoch.
//! Carrier/RPC bytes never supply these snapshots. Header hashes authenticate
//! data and branch identity; they do not establish unbiased entropy or work
//! correctness. Those assumptions and selected approvals remain explicit.
use super::*;
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::NativeRetainedV1,
    palw_source_bootstrap_v1::{
        PalwSelectedSourceBootstrapPolicyV1, PalwSourceBootstrapContextFaultV1, PalwSourceBootstrapOwnContextV1,
    },
    palw_state_v2::{PalwChainStateV2, PalwStateV2Error},
};

fn store_fault(error: kaspa_database::prelude::StoreError) -> PalwSourceBootstrapContextFaultV1 {
    use kaspa_database::prelude::StoreError;
    match error {
        StoreError::KeyNotFound(_) => PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
        StoreError::LocalResourceUnavailable(_) => PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
        _ => PalwSourceBootstrapContextFaultV1::StoreUnreadable,
    }
}
fn import_fault(error: kaspa_consensus_core::errors::pruning::PruningImportError) -> PalwSourceBootstrapContextFaultV1 {
    use kaspa_consensus_core::errors::pruning::PruningImportError as E;
    match error {
        E::PruningPointImportContextUnavailable(_, _)
        | E::PruningImportSessionRefused { .. }
        | E::ImportedPalwStateHeaderMissing(_)
        | E::MissingPointedPruningPoint
        | E::PruningPointsWrongGenesis(_, _) => PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
        _ => PalwSourceBootstrapContextFaultV1::StoreUnreadable,
    }
}
fn state_fault(error: PalwStateV2Error) -> PalwSourceBootstrapContextFaultV1 {
    match error {
        PalwStateV2Error::NativeResource(why) => PalwSourceBootstrapContextFaultV1::Capacity(why),
        PalwStateV2Error::NativeContextUnavailable(why) => why,
        _ => PalwSourceBootstrapContextFaultV1::StoreUnreadable,
    }
}

fn snapshot_fault(error: PalwWeighFaultV2) -> PalwSourceBootstrapContextFaultV1 {
    match error {
        PalwWeighFaultV2::NativeCapacityUnavailable => PalwSourceBootstrapContextFaultV1::CapacityUnavailable,
        PalwWeighFaultV2::NoOpinion => PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
        PalwWeighFaultV2::StoreUnreadable => PalwSourceBootstrapContextFaultV1::StoreUnreadable,
    }
}

struct Boundary {
    block: BlockHash,
    daa: u64,
    /// Existing selected child commits the boundary's AFTER-state. None is
    /// allowed only when the proposed child itself crosses the cutoff.
    witness_child: Option<BlockHash>,
}

impl VirtualStateProcessor {
    /// Attach only the source epoch observation. Ordinary fee/pre-object inputs
    /// retain their existing model and class court semantics; object consumers
    /// can add those fields separately. Resolve before mutating the parent state.
    pub(in crate::pipeline::virtual_processor) fn palw_attach_source_bootstrap_context_v1(
        &self,
        state: &PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        extras: &mut kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
    ) {
        if let Some(fence) = self
            .palw_selected_source_bootstrap_v1
            .as_ref()
            .filter(|f| f.at != kaspa_consensus_core::config::params::ForkActivation::never() && f.at.is_active(point.daa_score))
        {
            use kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1 as Fault;
            let result = match state.last_point() {
                Some(parent) => self.palw_source_bootstrap_own_context_v1(state, parent.block, point.daa_score, &fence.policy),
                None if point.daa_score <= fence.policy.freeze_cutoff_daa => Ok(None),
                None => Err(Fault::HistoryUnavailable),
            };
            match result {
                Ok(Some(context)) => match context.into_shared_source_bootstrap_v1() {
                    Ok(owned) => extras.selected_source_bootstrap_v1 = Some(owned),
                    Err(coverage) => extras.selected_source_bootstrap_fault_v1 = Some(Fault::Capacity(coverage)),
                },
                Ok(None) => {}
                Err(fault) => extras.selected_source_bootstrap_fault_v1 = Some(fault),
            }
        }
    }

    fn source_bootstrap_boundary_v1(
        &self,
        parent: BlockHash,
        candidate_daa: u64,
        cutoff: u64,
        remaining: &mut u32,
    ) -> Result<Option<Boundary>, PalwSourceBootstrapContextFaultV1> {
        if candidate_daa <= cutoff {
            return Ok(None);
        }
        let mut block = parent;
        let mut upper_daa = candidate_daa;
        let mut child = None;
        loop {
            *remaining = remaining.checked_sub(1).ok_or(PalwSourceBootstrapContextFaultV1::HistoryUnavailable)?;
            let header = self.headers_store.get_header(block).map_err(store_fault)?;
            if kaspa_consensus_core::hashing::header::hash(&header) != block || header.daa_score > upper_daa {
                return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
            }
            if header.daa_score <= cutoff {
                return Ok(Some(Boundary { block, daa: header.daa_score, witness_child: child }));
            }
            if block == self.genesis.hash {
                return Err(PalwSourceBootstrapContextFaultV1::HistoryUnavailable);
            }
            let previous = self.ghostdag_store.get_data(block).map_err(store_fault)?.selected_parent;
            if previous == block {
                return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
            }
            upper_daa = header.daa_score;
            child = Some(block);
            block = previous;
        }
    }

    /// The caller supplies its actual candidate's selected parent separately
    /// from the proposed DAA. Template/rehearsal must not pass a future block
    /// hash as an already stored header, or require it to exist before mining.
    /// Missing/pruned history and local capacity retain distinct retry causes.
    pub(in crate::pipeline::virtual_processor) fn palw_source_bootstrap_own_context_v1(
        &self,
        state: &PalwChainStateV2,
        selected_parent: BlockHash,
        candidate_daa: u64,
        policy: &PalwSelectedSourceBootstrapPolicyV1,
    ) -> Result<Option<NativeRetainedV1<PalwSourceBootstrapOwnContextV1>>, PalwSourceBootstrapContextFaultV1> {
        policy.validate().map_err(|_| PalwSourceBootstrapContextFaultV1::StoreUnreadable)?;
        if candidate_daa <= policy.freeze_cutoff_daa {
            return Ok(None);
        }
        let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.network_id_bytes.as_slice(),
            Some(self.genesis.hash),
        );
        let mut ruleset = [0u8; 64];
        ruleset[..32].copy_from_slice(&self.palw_kernel_route_ruleset_id.as_bytes());
        if policy.chain_genesis != self.genesis.hash || policy.network != network || policy.ruleset != Hash64::from_bytes(ruleset) {
            return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
        }
        let (before, stable) = self.pruning_import_status().map_err(import_fault)?;
        if !stable || before.active.is_some() {
            return Err(PalwSourceBootstrapContextFaultV1::HistoryUnavailable);
        }
        if state.last_point().is_none_or(|p| p.block != selected_parent)
            || !self.reachability_service.has_reachability_data(selected_parent)
        {
            return Err(PalwSourceBootstrapContextFaultV1::HistoryUnavailable);
        }
        let mut remaining = policy.maximum_history_headers;
        let previous = state.source_bootstrap_epoch_v1().map_err(state_fault)?;
        let owned = if let Some(row) = previous {
            let context = &row.context;
            context.validate_shape().map_err(|_| PalwSourceBootstrapContextFaultV1::StoreUnreadable)?;
            if context.policy != *policy {
                return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
            }
            // This row came from the caller's authenticated OWN parent state,
            // including its root-verified pruning import. The native journal
            // fixed these branch facts before pruning; requiring their old raw
            // headers/reachability forever would strand a legitimate pruned
            // node. A remote row or a replacement current roster is never read.
            let parent_daa = state.last_point().ok_or(PalwSourceBootstrapContextFaultV1::HistoryUnavailable)?.daa_score;
            if context.frozen_block_daa > parent_daa || context.anchor.is_some_and(|(_, daa)| daa > parent_daa) {
                return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
            }
            let anchor = if context.anchor.is_some() || context.population_refused || context.population.is_empty() {
                context.anchor
            } else {
                self.source_bootstrap_boundary_v1(selected_parent, candidate_daa, policy.anchor_cutoff_daa, &mut remaining)?
                    .map(|at| (at.block, at.daa))
            };
            context.with_anchor_owned_v1(anchor).map_err(state_fault)?
        } else {
            let Some(freeze) =
                self.source_bootstrap_boundary_v1(selected_parent, candidate_daa, policy.freeze_cutoff_daa, &mut remaining)?
            else {
                return Ok(None);
            };
            let historical = if freeze.block == selected_parent {
                None
            } else {
                Some(self.palw_candidate_state_v2_checked(freeze.block).map_err(snapshot_fault)?)
            };
            let frozen = historical.as_deref().unwrap_or(state);
            if frozen.last_point().is_none_or(|p| p.block != freeze.block || p.daa_score != freeze.daa) {
                return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
            }
            let root = self.palw_committed_root_v1(frozen);
            if let Some(child) = freeze.witness_child {
                remaining = remaining.checked_sub(1).ok_or(PalwSourceBootstrapContextFaultV1::HistoryUnavailable)?;
                let header = self.headers_store.get_header(child).map_err(store_fault)?;
                let committed = kaspa_consensus_core::palw_state_proof_v1::state_root_of_pinned_header_v1(&header, child)
                    .map_err(|_| PalwSourceBootstrapContextFaultV1::StoreUnreadable)?;
                if committed != root {
                    return Err(PalwSourceBootstrapContextFaultV1::StoreUnreadable);
                }
            }
            let frozen_context =
                PalwSourceBootstrapOwnContextV1::from_own_frozen_state_v1(*policy, freeze.block, root, freeze.daa, frozen, None)
                    .map_err(state_fault)?;
            if frozen_context.population_refused || frozen_context.population.is_empty() {
                // Deterministic unavailable epoch: do not demand unrelated old
                // anchor history or choose a substitute population/anchor.
                frozen_context
            } else {
                let anchor = self
                    .source_bootstrap_boundary_v1(selected_parent, candidate_daa, policy.anchor_cutoff_daa, &mut remaining)?
                    .map(|at| (at.block, at.daa));
                frozen_context.with_anchor_owned_v1(anchor).map_err(state_fault)?
            }
        };
        if candidate_daa > policy.anchor_cutoff_daa
            && owned.anchor.is_none()
            && !owned.population_refused
            && !owned.population.is_empty()
        {
            return Err(PalwSourceBootstrapContextFaultV1::HistoryUnavailable);
        }
        let (after, stable_after) = self.pruning_import_status().map_err(import_fault)?;
        if !stable_after || after.active.is_some() || before.epoch != after.epoch {
            return Err(PalwSourceBootstrapContextFaultV1::HistoryUnavailable);
        }
        Ok(Some(owned))
    }
}
