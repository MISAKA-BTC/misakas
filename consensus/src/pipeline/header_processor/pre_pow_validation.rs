use super::*;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::block_window_cache::BlockWindowHeap;
use crate::processes::window::WindowManager;
use kaspa_consensus_core::header::Header;

impl HeaderProcessor {
    pub(super) fn pre_pow_validation(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        self.check_pruning_violation(ctx)?;
        self.check_difficulty_and_daa_score(ctx, header)?;
        Ok(())
    }

    fn check_pruning_violation(&self, ctx: &HeaderProcessingContext) -> BlockProcessResult<()> {
        let known_parents = ctx.known_direct_parents.as_slice();

        // We check that the new block is in the future of the pruning point by verifying that at least
        // one of its parents is in the pruning point future (or the pruning point itself). Otherwise,
        // the Prunality proof implies that the block can be discarded.
        if !self.reachability_service.is_dag_ancestor_of_any(ctx.pruning_point, &mut known_parents.iter().copied()) {
            return Err(RuleError::PruningViolation(ctx.pruning_point));
        }
        Ok(())
    }

    fn check_difficulty_and_daa_score(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let ghostdag_data = ctx.ghostdag_data();
        let daa_window = self.window_manager.block_daa_window(ghostdag_data)?;

        if daa_window.daa_score != header.daa_score {
            return Err(RuleError::UnexpectedHeaderDaaScore(daa_window.daa_score, header.daa_score));
        }

        // kaspa-pq ADR-0039 §16.3 / C6 clause 7 — LANE-AWARE difficulty. On a PALW-active net EVERY block
        // is v3 and each LANE retargets on its OWN blocks (the hash floor on algo-3 blocks, the replica
        // lane on algo-4 blocks), so a mixed-lane header's `bits` must match its lane's difficulty, not
        // the single-lane average over both lanes. The else-branch is BYTE-FOR-BYTE the pre-PALW path.
        // Mainnet, testnet-10, simnet and devnet use `palw_activation_daa_score == u64::MAX` and take
        // the legacy branch. The three PALW presets use 0 and take the lane-aware branch.
        let expected_bits = if header.daa_score >= self.palw_activation_daa_score {
            let per_set = self.palw_per_set_sublane(header)?;
            self.calculate_palw_lane_difficulty_bits(&daa_window.window, header.pow_algo_id, per_set)
        } else {
            self.window_manager.calculate_difficulty_bits(ghostdag_data, &daa_window)
        };
        ctx.mergeset_non_daa = Some(daa_window.mergeset_non_daa);

        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.hash, header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(daa_window.window);
        Ok(())
    }

    /// ADR-0039 §16.3 / C6 clause 7 — the expected `bits` for a v3 header's LANE, from the same DAA
    /// window filtered to same-lane blocks (each block's `pow_algo_id` read from its header — it is not
    /// in `CompactHeaderData`). Delegates the trim + retarget to the pure, live-engine-equivalent
    /// [`crate::processes::difficulty::lane_expected_bits`]. Below the lane's `min_samples` it HOLDs the
    /// lane's `genesis_bits` — a PURE header-window value (NOT the virtual, pruned lane-bits store, which
    /// would reintroduce the C6 order/prune hazard). Only reached inside the `palw_active` gate, so it
    /// never runs on a shipped preset.
    ///
    /// The body lives in [`crate::processes::difficulty::lane_bits_from_window`] so the algo-4 mining
    /// template derives `bits` through the SAME code this check runs — construction == validation for a
    /// field the miner does not get to choose.
    fn calculate_palw_lane_difficulty_bits(
        &self,
        window: &BlockWindowHeap,
        header_algo_id: u8,
        per_set: Option<crate::processes::difficulty::PalwSetSublane>,
    ) -> u32 {
        crate::processes::difficulty::lane_bits_from_window(
            self.headers_store.as_ref(),
            window,
            header_algo_id,
            &self.palw_lane_difficulty,
            per_set,
        )
    }

    /// ADR-MA §13.1/§13.2 — the header-stage Compute Set resolution, feeding the §12 per-set
    /// difficulty check. Below the registry fence (every shipped preset), below Header v5, or on
    /// the hash lane: `None` — the flat single-lane path, byte-identical to today.
    ///
    /// On a registry-active net a v5 PALW-lane header must resolve, from the content-addressed
    /// registry records it COMMITTED, a registered descriptor (§13.2 unknown set ⇒ reject), the
    /// exact policy/plan revisions effective at its own DAA with `state == Active`, and a nonzero
    /// allocation share (`resolve_source_policy_for_credit`, the same §14 rule the GHOSTDAG
    /// credit seam applies). Failure REJECTS the block — §13.2 forbids any default-scale
    /// fallback.
    ///
    /// A LOCALLY MISSING record (any tier) is also a header rejection, not a process abort. The
    /// §21.4 delivery duty is real — body-stage admission folds records before a v5 header can
    /// commit them, and since protocol v104 catch-up IBD pre-delivers the peer's record set
    /// before header download — so absence here means the DATA PATH failed this header: an
    /// un-upgraded or misbehaving IBD peer that withheld records, a forged commit, or a relay
    /// that outran the local body fold. Rejecting fails that path closed (the IBD attempt aborts
    /// and another peer is tried; a forged commit dies with its subtree), whereas the previous
    /// `panic!` here handed any such peer a remote crash trigger.
    fn palw_per_set_sublane(&self, header: &Header) -> BlockProcessResult<Option<crate::processes::difficulty::PalwSetSublane>> {
        use crate::model::stores::palw_compute_registry::PalwComputeRegistryStoreReader;
        use kaspa_consensus_core::palw_compute_set::resolve_source_policy_for_credit;
        let registry_active = self.palw_compute_registry_activation_daa_score != u64::MAX
            && header.daa_score >= self.palw_compute_registry_activation_daa_score
            && header.version >= kaspa_consensus_core::constants::PALW_COMPUTE_SET_HEADER_VERSION;
        if !registry_active || header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA {
            return Ok(None);
        }
        let missing = |what: &str, id: kaspa_consensus_core::Hash64| -> RuleError {
            RuleError::PalwComputeSetResolution(
                header.hash,
                format!(
                    "committed {what} {id} has no local record — on a registry-active net the sync data path must \
                     deliver registry records before the headers committing them (ADR-MA §21.4 / protocol v104)"
                ),
            )
        };
        // §13.2 — an UNREGISTERED set is a header rejection (a peer's forged id): nothing was
        // ever admitted under that id, so nothing can be missing.
        match self.palw_compute_registry_store.descriptor(header.palw_compute_set_id) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(RuleError::PalwComputeSetResolution(
                    header.hash,
                    format!("unknown compute_set_id {} (no registered descriptor)", header.palw_compute_set_id),
                ));
            }
            Err(store_error) => panic!("compute registry descriptor read failed for header {}: {store_error}", header.hash),
        }
        let policy = match self.palw_compute_registry_store.policy(header.palw_compute_policy_id) {
            Ok(Some(policy)) => policy,
            Ok(None) => return Err(missing("policy", header.palw_compute_policy_id)),
            Err(store_error) => panic!("compute registry policy read failed for header {}: {store_error}", header.hash),
        };
        let plan = match self.palw_compute_registry_store.plan(header.palw_allocation_plan_id) {
            Ok(Some(plan)) => plan,
            Ok(None) => return Err(missing("allocation plan", header.palw_allocation_plan_id)),
            Err(store_error) => panic!("compute registry plan read failed for header {}: {store_error}", header.hash),
        };
        let resolution = resolve_source_policy_for_credit(
            &policy,
            &plan,
            header.palw_compute_policy_id,
            header.palw_allocation_plan_id,
            header.palw_compute_set_id,
            header.daa_score,
        )
        .map_err(|rejection| RuleError::PalwComputeSetResolution(header.hash, rejection.to_string()))?;
        Ok(Some(crate::processes::difficulty::PalwSetSublane {
            compute_set_id: header.palw_compute_set_id,
            target_share_bps: resolution.target_share_bps,
        }))
    }
}
