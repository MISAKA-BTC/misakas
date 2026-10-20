//! Prospective claim contract/continuation writers. All failures precede claim acceptance writes.
use super::*;
use crate::palw_staged_court_state_v1::*;
impl PalwChainStateV2 {
    /// Native verified-state import only. No rooted row or dynamic material is
    /// cloned, and this iterator is not an anonymous request-facing reader.
    /// Claim-contract rows strictly after `after`, in key order; owner/inventory rows end the walk.
    pub fn staged_claim_rows_after_v1(&self, after: Option<Hash64>) -> impl Iterator<Item = (Hash64, &PalwStagedCourtRowV1)> {
        let start = match after {
            Some(claim) => std::ops::Bound::Excluded(PalwStagedCourtKeyV1::Claim(claim)),
            None => std::ops::Bound::Unbounded,
        };
        self.staged_courts.range((start, std::ops::Bound::Unbounded)).map_while(|(key, row)| match key {
            PalwStagedCourtKeyV1::Claim(claim) => Some((*claim, row)),
            _ => None,
        })
    }
    pub fn staged_cache_lease_rows_v1(&self) -> impl Iterator<Item = (PalwStagedCourtKeyV1, &PalwStagedCourtRowV1)> {
        self.staged_courts.iter().map(|(key, row)| (*key, row))
    }
    pub fn staged_required_families_v1(&self, original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3) -> bool {
        self.class_semantics.get(&original.job.class_id).is_some_and(|semantics| {
            original.job.is_prefix_state() || original.job.is_constraint() || semantics.raw_tokenizer_bytes.is_some()
        })
    }
    /// Native admitted upper bound, including the producer but not the optional
    /// outside owner. It never depends on how many seats have answered yet.
    pub fn staged_maximum_custodians_v1(
        &self,
        class_id: Hash64,
        now: u64,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
    ) -> Result<u64, PalwStateV2Error> {
        Self::staged_custodian_upper_v1(self.tir_classes.contains_key(&class_id), now, params, extras)
    }
    /// Class-independent native upper for a prospective REGISTERED TIR claim.
    /// The claim adapter still checks registration and rederives this bound.
    pub fn staged_registered_tir_maximum_custodians_v1(
        now: u64,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
    ) -> Result<u64, PalwStateV2Error> {
        Self::staged_custodian_upper_v1(true, now, params, extras)
    }
    fn staged_custodian_upper_v1(
        registered_tir: bool,
        now: u64,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
    ) -> Result<u64, PalwStateV2Error> {
        let flat = if params.panel_v3_rule_at(now) {
            u64::from(params.panel_v3().ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?.policy.seat_count)
        } else {
            u64::from(extras.model_registry.as_ref().map_or(u16::MAX, |r| r.globals.seat_count))
        };
        let mut maximum = flat;
        if extras.shard_licensing.is_some() {
            maximum = maximum.max(
                flat.checked_mul(u64::from(crate::palw_shard_licensing_v1::PALW_SHARD_LICENSING_MAX_SHARDS_V1))
                    .ok_or(PalwStateV2Error::Overflow("prospective source shard custodians"))?,
            );
        }
        if params.tir_shard_active_at(now) && registered_tir {
            maximum = maximum.max(u64::from(misaka_palw_panel::MAX_STRATIFIED_SEATS_V1));
        }
        maximum.checked_add(1).ok_or(PalwStateV2Error::Overflow("prospective source custodians"))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn staged_preclaim_quote_v1(
        &self,
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        header: &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
        quote_daa: u64,
        maximum_custodians: u64,
        maximum_leaves: u64,
        config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
        gates: crate::palw_staged_preclaim_v1::PalwStagedRouteGatesV1,
    ) -> Result<Option<crate::palw_staged_preclaim_v1::PalwStagedPreclaimQuoteV1>, String> {
        if !self.staged_required_families_v1(original) {
            return Ok(None);
        }
        let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id: original.job.class_id,
            producer: PalwBondKeyV2(original.job.executor_bond),
            execution_root: original.execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original),
            accepted_daa: quote_daa,
        };
        let semantics =
            self.class_semantics.get(&geometry.class_id).ok_or("preclaim quote has no immutable admitted class semantics")?;
        if matches!(header, crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(_)) {
            let record =
                self.tir_classes.get(&geometry.class_id).ok_or("TIR preclaim quote has no native registered class record")?;
            let quote = crate::palw_staged_preclaim_v1::staged_preclaim_tir_quote_v1(
                &geometry,
                original,
                header,
                record,
                crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(self, geometry.class_id),
                semantics,
                self.staged_tokenizer_inventory_v1(semantics.tokenizer_id),
                maximum_custodians,
                maximum_leaves,
                config,
            )?;
            if quote.is_some() && !gates.tir {
                return Err("TIR required-family claims are refused below palw_staged_tir_route_v1".into());
            }
            return Ok(quote);
        }
        let profile = self
            .fp_work_profiles
            .get(&geometry.class_id)
            .ok_or("legacy preclaim quote has no native registered shape profile")?;
        crate::palw_staged_preclaim_v1::staged_preclaim_quote_with_gates_v1(
            &geometry,
            original,
            header,
            profile,
            semantics,
            self.staged_tokenizer_inventory_v1(semantics.tokenizer_id),
            maximum_custodians,
            maximum_leaves,
            config,
            gates,
        )
    }
    pub(super) fn staged_retirement_floor_v1(&self, claim: &Hash64, ordinary: u64) -> u64 {
        if self.claims.get(claim).is_some_and(|c| c.phase.is_terminal()) {
            let original = self.staged_claim_contract_v1(claim).map(|c| ordinary.max(c.terms.material_until_daa)).unwrap_or(ordinary);
            original.max(self.staged_tir_source_until_v1(*claim).unwrap_or(0))
        } else {
            ordinary
        }
    }
    pub fn staged_claim_contract_v1(&self, claim: &Hash64) -> Option<&PalwStagedClaimContractV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::Claim(*claim)) {
            Some(PalwStagedCourtRowV1::Claim(c)) => Some(c),
            Some(PalwStagedCourtRowV1::ClaimBundle(c)) => Some(&c.claim),
            _ => None,
        }
    }
    pub fn staged_bundle_contract_v1(&self, claim: Hash64) -> Option<&crate::palw_staged_bundle_v1::PalwStagedBundleContractV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::Claim(claim)) {
            Some(PalwStagedCourtRowV1::ClaimBundle(c)) => Some(c),
            _ => None,
        }
    }
    pub(super) fn staged_owner_allowance_used_v1(&self, owner: PalwBondKeyV2, limit: usize) -> usize {
        let regular = self.staged_by_owner.range((owner, ZERO_HASH64)..).take_while(|(bond, _)| *bond == owner).take(limit).count();
        let inventory = self
                .staged_inventory_by_owner
                .range((owner, ZERO_HASH64)..)
                .take_while(|(bond, _)| *bond == owner)
                .take(limit.saturating_sub(regular))
                .count();
        regular + inventory + self.staged_tir_acquisition_count_v1(owner, limit.saturating_sub(regular + inventory))
    }
    /// Separate allocation keys: one claim's old Owner row and multiple paid
    /// sidecars cannot overwrite each other's quota or collateral exposure.
    pub fn staged_tir_acquisition_v1(
        &self,
        claim: Hash64,
        owner: PalwBondKeyV2,
        allocation: Hash64,
    ) -> Option<&crate::palw_tir_public_state_v1::PalwTirPublicAcquisitionRowV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::TirAcquisition(claim, owner, allocation)) {
            Some(PalwStagedCourtRowV1::TirAcquisition(row)) => Some(row.as_ref()),
            _ => None,
        }
    }
    pub fn staged_tir_acquisition_count_v1(&self, owner: PalwBondKeyV2, limit: usize) -> usize {
        self.staged_tir_by_owner.range((owner, ZERO_HASH64, ZERO_HASH64)..)
            .take_while(|(bond, _, _)| *bond == owner).take(limit).count()
    }
    pub fn staged_tir_acquisition_exposure_v1(&self, owner: &PalwBondKeyV2) -> u128 {
        self.staged_tir_by_owner.range((*owner, ZERO_HASH64, ZERO_HASH64)..)
            .take_while(|(bond, _, _)| bond == owner)
            .filter_map(|(_, claim, allocation)| self.staged_tir_acquisition_v1(*claim, *owner, *allocation))
            .map(|row| u128::from(row.owned.contract_v1().terms.plan.deposit_sompi))
            .fold(0u128, u128::saturating_add)
    }
    /// Original full-allocation escrow is distinct from both the ordinary FP
    /// reservation and each outside owner's separately funded acquisition.
    pub fn staged_tir_claim_exposure_v1(&self, producer: &PalwBondKeyV2) -> u128 {
        self.staged_tir_claims_by_producer.range((*producer, ZERO_HASH64)..)
            .take_while(|(bond, _)| bond == producer)
            .filter_map(|(_, claim)| crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(self, *claim))
            .map(|frame| u128::from(frame.plan_v1().terms.plan.deposit_sompi))
            .fold(0u128, u128::saturating_add)
    }
    /// **G14 R5: an outside owner's demand on an ORIGINAL TIR frame is open** — a pending accusation (the one
    /// `palw_accusation_pending_v1` predicate), so no non-fraud end closes the claim neutrally while the demanded custodians still
    /// owe their answer or the owner its collection. Rows exist only past `palw_tir_public_conviction_v1`.
    pub fn staged_tir_public_demand_open_v1(&self, claim: &Hash64) -> bool {
        self.staged_tir_by_claim.get(claim).is_some_and(|rows| {
            rows.iter().any(|(_, key)| match key {
                PalwStagedCourtKeyV1::TirAcquisition(row_claim, owner, allocation) => self
                    .staged_tir_acquisition_v1(*row_claim, *owner, *allocation)
                    .is_some_and(|row| row.owned.active_v1().is_some()),
                _ => false,
            })
        })
    }
    /// **G14 R4 (node discovery): the ORIGINAL frames after `after`, in key order** — at most `min(limit, 64)` claim ids, read from
    /// the rooted `TirClaim` rows. Metadata only; an outside verifier's worker pages through it to choose what to check.
    pub fn staged_tir_frames_after_v1(&self, after: Option<Hash64>, limit: usize) -> Vec<Hash64> {
        use std::ops::Bound;
        let lower = match after {
            Some(claim) => Bound::Excluded(PalwStagedCourtKeyV1::TirClaim(claim)),
            None => Bound::Included(PalwStagedCourtKeyV1::TirClaim(ZERO_HASH64)),
        };
        self.staged_courts
            .range((lower, Bound::Unbounded))
            .map_while(|(key, _)| match key {
                PalwStagedCourtKeyV1::TirClaim(claim) => Some(*claim),
                _ => None,
            })
            .take(limit.min(64))
            .collect()
    }
    /// **G14 R4 (node discovery): the outside owners' acquisition keys on `claim`** — `(owner, allocation)` of every live
    /// `TirAcquisition` row (at most K_owner). Metadata only: an owner finds its own row after its Open is accepted.
    pub fn staged_tir_acquisitions_of_v1(&self, claim: &Hash64) -> Vec<(PalwBondKeyV2, Hash64)> {
        self.staged_tir_by_claim.get(claim).map_or_else(Vec::new, |rows| {
            rows.iter()
                .filter_map(|(_, key)| match key {
                    PalwStagedCourtKeyV1::TirAcquisition(_, owner, allocation) => Some((*owner, *allocation)),
                    _ => None,
                })
                .collect()
        })
    }
    /// The live `TirAcquisition` rows of `claim`, with their keys (at most the rows of one frame: K_owner slot holders plus the
    /// retired ones that wait for the material horizon).
    fn staged_tir_owner_rows_v1(
        &self,
        claim: &Hash64,
    ) -> Vec<(PalwStagedCourtKeyV1, &crate::palw_tir_public_state_v1::PalwTirPublicAcquisitionRowV1)> {
        self.staged_tir_by_claim.get(claim).map_or_else(Vec::new, |rows| {
            rows.iter()
                .filter_map(|(_, key)| match key {
                    PalwStagedCourtKeyV1::TirAcquisition(c, owner, allocation) => {
                        self.staged_tir_acquisition_v1(*c, *owner, *allocation).map(|row| (*key, row))
                    }
                    _ => None,
                })
                .collect()
        })
    }
    /// **G14 R1/R7: outside owners holding one of `claim`'s K_owner slots** — every live `TirAcquisition` row that can still demand
    /// or still has an unpaid demand open ([`crate::palw_tir_public_owned_v1::PalwTirPublicOwnedV1::holds_owner_slot_v1`]). A row
    /// that spent its turns, or whose idle slot a later Open reclaimed, stays in state to its horizon but holds no slot. The K_owner
    /// cap reads this count before any duty is created.
    pub fn staged_tir_owner_count_v1(&self, claim: &Hash64) -> u64 {
        self.staged_tir_owner_rows_v1(claim).into_iter().filter(|(_, row)| row.owned.holds_owner_slot_v1()).count() as u64
    }
    /// **Lane g14-court-capacity-r2 (past `palw_court_capacity_v1`): the owner queue of `claim`** — every row that still wants a slot
    /// (`holds_owner_slot_v1`), in arrival order `(reserved_daa, key)` (one claim: one door, so arrival order is deadline order). The
    /// first K_owner of it are ADMITTED (they may demand); the rest WAIT, imposing no custodian load, and are admitted in this order
    /// as the rows ahead release their slots — so an Open filed later never starts before an Open filed earlier. Each entry carries the
    /// DAA from which it holds its slot (`admitted_since`): its Open, or the latest release (`last_progress_daa` of a row ahead that
    /// stopped wanting — its last move, or its reclamation) that let it in.
    pub fn staged_tir_owner_queue_v1(&self, claim: &Hash64) -> Vec<(PalwStagedCourtKeyV1, PalwBondKeyV2, bool, u64)> {
        let rows = self.staged_tir_owner_rows_v1(claim);
        let mut wanting: Vec<(u64, PalwStagedCourtKeyV1, PalwBondKeyV2)> = rows
            .iter()
            .filter(|(_, row)| row.owned.holds_owner_slot_v1())
            .map(|(key, row)| (row.owned.contract_v1().reserved_daa, *key, row.owned.contract_v1().owner))
            .collect();
        wanting.sort();
        let mut released: Vec<u64> = rows
            .iter()
            .filter(|(_, row)| !row.owned.holds_owner_slot_v1())
            .map(|(_, row)| row.owned.spent_v1().last_progress_daa)
            .collect();
        released.sort_unstable();
        let cap = crate::palw_tir_public_claim_v1::PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1 as usize;
        wanting
            .into_iter()
            .enumerate()
            .map(|(position, (reserved, key, owner))| {
                // The j-th admitted row beyond the first K_owner needed the j-th release (in time) to get in.
                let since = if position < cap {
                    reserved
                } else {
                    released.get(position - cap).copied().unwrap_or(reserved).max(reserved)
                };
                (key, owner, position < cap, since)
            })
            .collect()
    }
    /// **Lane g14-court-capacity-r2: is the owner row `key` of `claim` admitted** (among the first K_owner of the owner queue)?
    pub fn staged_tir_owner_admitted_v1(&self, claim: &Hash64, key: &PalwStagedCourtKeyV1) -> bool {
        self.staged_tir_owner_queue_v1(claim).iter().any(|(k, _, admitted, _)| k == key && *admitted)
    }
    /// **Lane g14-court-capacity-r2: the admitted rows of `claim` idle at `now`**, idleness counted from the later of the row's own
    /// last progress and the DAA it was admitted (a row that waited is never idle for the time it waited).
    pub fn staged_tir_idle_admitted_owners_v1(&self, claim: &Hash64, now: u64) -> Vec<(PalwStagedCourtKeyV1, PalwBondKeyV2)> {
        let queue = self.staged_tir_owner_queue_v1(claim);
        self.staged_tir_owner_rows_v1(claim)
            .into_iter()
            .filter_map(|(key, row)| {
                let (_, owner, admitted, since) = *queue.iter().find(|(k, ..)| *k == key)?;
                let idle_since = row.owned.idle_since_v1().max(since);
                (admitted && now >= idle_since.saturating_add(row.owned.owner_idle_grace_daa_v1())).then_some((key, owner))
            })
            .collect()
    }
    /// **Lane g14-court-capacity-r2: the admitted rows of `claim` past their tenure at `now`** — only while some row WAITS: an admitted
    /// row holds its slot at most `tenure` DAA from its admission while another verifier waits, then yields it (retired in place,
    /// its deposit NOT forfeited — it was active; its accepted answers stay rooted, and a conviction needs no slot).
    pub fn staged_tir_overstayed_owners_v1(&self, claim: &Hash64, now: u64, tenure: u64) -> Vec<(PalwStagedCourtKeyV1, PalwBondKeyV2)> {
        let queue = self.staged_tir_owner_queue_v1(claim);
        if queue.iter().all(|(_, _, admitted, _)| *admitted) {
            return Vec::new();
        }
        queue
            .into_iter()
            .filter(|(_, _, admitted, since)| *admitted && now >= since.saturating_add(tenure))
            .map(|(key, owner, ..)| (key, owner))
            .collect()
    }
    /// **Lane g14-court-capacity-r2: would a Demand of the owner row `key` at `now` be admitted** — it is admitted already, or the
    /// rotation that Demand triggers (overstayed rows past `tenure` yield, idle admitted rows are reclaimed) frees enough slots for
    /// every waiting row ahead of it and itself. What a node's owner worker asks before it signs a Demand (a refused Demand is a
    /// wasted carrier); the fold decides.
    pub fn staged_tir_owner_demand_admits_v1(&self, claim: &Hash64, key: &PalwStagedCourtKeyV1, now: u64, tenure: u64) -> bool {
        let queue = self.staged_tir_owner_queue_v1(claim);
        let Some(position) = queue.iter().position(|(k, ..)| k == key) else { return false };
        if queue[position].2 {
            return true;
        }
        let mut freed: std::collections::BTreeSet<PalwStagedCourtKeyV1> =
            self.staged_tir_idle_admitted_owners_v1(claim, now).into_iter().map(|(k, _)| k).collect();
        freed.extend(self.staged_tir_overstayed_owners_v1(claim, now, tenure).into_iter().map(|(k, _)| k));
        let cap = crate::palw_tir_public_claim_v1::PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1 as usize;
        position < cap + freed.len()
    }
    /// **G14 R1 / G-T1: the slot an Open at DAA `now` reclaims when the frame is at K_owner** — the slot holder that has been idle
    /// longest ([`crate::palw_tir_public_owned_v1::PalwTirPublicOwnedV1::owner_idle_at_v1`]; ties by key), or `None` when every
    /// holder is still making progress.
    pub fn staged_tir_idle_owner_v1(&self, claim: &Hash64, now: u64) -> Option<(PalwStagedCourtKeyV1, PalwBondKeyV2)> {
        self.staged_tir_owner_rows_v1(claim)
            .into_iter()
            .filter(|(_, row)| row.owned.owner_idle_at_v1(now))
            .min_by_key(|(key, row)| (row.owned.idle_since_v1(), *key))
            .map(|(key, row)| (key, row.owned.contract_v1().owner))
    }
    /// **G14 R1 / G-T1: can an independent bond Open an owner acquisition on `claim` at DAA `now`?** — a free slot under K_owner, or
    /// an idle holder whose slot the Open reclaims. Metadata for the node's owner worker; the fold decides.
    pub fn staged_tir_owner_slot_open_v1(&self, claim: &Hash64, now: u64) -> bool {
        self.staged_tir_owner_count_v1(claim) < crate::palw_tir_public_claim_v1::PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1
            || self.staged_tir_idle_owner_v1(claim, now).is_some()
    }
    /// **G14 R6: an ORIGINAL TIR frame's d0 conviction door is still open at `now`** — `[accepted, collect_until)` of the frame's
    /// own terms. Gate C waits on it (never a deadline failure), and the claim's `Final` floor is its close.
    pub fn palw_tir_public_prosecution_open_v1(&self, claim: &Hash64, now: u64) -> bool {
        crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(self, *claim)
            .is_some_and(|frame| now < frame.plan_v1().terms.collect_until_daa)
    }
    pub fn staged_tir_source_until_v1(&self, claim: Hash64) -> Option<u64> {
        self.staged_tir_by_claim.get(&claim).and_then(|rows| rows.last().map(|(until, _)| *until))
    }
    pub fn staged_tir_preparation_v1(
        &self,
        claim: Hash64,
        allocation: Hash64,
    ) -> Option<&crate::palw_tir_public_state_v1::PalwTirPublicPreparationRowV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::TirPreparation(claim, allocation)) {
            Some(PalwStagedCourtRowV1::TirPreparation(row)) => Some(row.as_ref()),
            _ => None,
        }
    }
    pub fn staged_original_job_v1(
        &self,
        claim: Hash64,
    ) -> Result<Option<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>, String> {
        self.fp_public_jobs.get(&claim).map(|raw| borsh::from_slice(raw).map_err(|e| e.to_string())).transpose()
    }
    pub fn staged_owned_row_v1(&self, claim: Hash64, owner: PalwBondKeyV2) -> Option<&PalwStagedCourtRowV1> {
        self.staged_courts.get(&PalwStagedCourtKeyV1::Owner(claim, owner))
    }
    pub fn staged_tokenizer_inventory_v1(
        &self,
        artifact: Hash64,
    ) -> Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::Inventory(artifact)) {
            Some(PalwStagedCourtRowV1::ArtifactInventory(cert)) => Some(cert),
            _ => None,
        }
    }
    pub fn staged_tokenizer_inventory_open_v1(
        &self,
        artifact: Hash64,
        owner: PalwBondKeyV2,
    ) -> Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::InventoryOpen(artifact, owner)) {
            Some(PalwStagedCourtRowV1::ArtifactInventoryOpen(row)) => Some(row),
            _ => None,
        }
    }
    pub fn staged_owned_row_for_seed_v1(&self, claim: Hash64, owner: PalwBondKeyV2, seed: Hash64) -> Option<&PalwStagedCourtRowV1> {
        match self.staged_owned_row_v1(claim, owner)? {
            PalwStagedCourtRowV1::OwnerBundle(bundle) => bundle.seed_row_v1(seed),
            row => row.active_row_v1().filter(|row| row.public_cache_leases_v1().iter().any(|lease| lease.seed_id == seed)),
        }
    }
    /// A quote is not an admission credential: BundleOpen repeats this exact native derivation.
    pub fn staged_bundle_plan_v1(
        &self,
        claim: Hash64,
        actual_raw_cap: Option<u64>,
    ) -> Result<Option<crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1>, String> {
        use crate::palw_staged_bundle_v1::*;
        let Some(PalwStagedCourtRowV1::ClaimBundle(contract)) = self.staged_courts.get(&PalwStagedCourtKeyV1::Claim(claim)) else {
            return Ok(None);
        };
        let target =
            crate::palw_offence_attribution_v1::palw_offence_target_v1(self, &claim).ok_or("bundle quote has no accepted target")?;
        let mut families: Vec<_> = contract.plan.stages.iter().map(|s| (s.family, s.terms.plan, s.acquisition)).collect();
        let tokenizer = families.iter_mut().find(|(family, _, _)| *family == PalwStagedFamilyV1::Tokenizer);
        let inventory = match (tokenizer, actual_raw_cap) {
            (Some((_, plan, acquisition)), Some(cap)) => {
                let semantics = target.registered_semantics.as_ref().ok_or("bundle quote has no immutable tokenizer statement")?;
                let certificate = self
                    .staged_tokenizer_inventory_v1(semantics.tokenizer_id)
                    .ok_or("bundle quote has no accepted exact raw EOF inventory")?;
                if cap != certificate.actual_bytes {
                    return Err("owner actual cap changes accepted exact raw EOF inventory".into());
                }
                let bound = certificate.key_event_upper_bound_v1();
                *plan = crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(
                    cap,
                    bound,
                    contract.config.prices,
                )?
                .court_envelope(plan.vocab, contract.config.prices)?;
                let profile = self.fp_work_profiles.get(&target.class_id).ok_or("bundle quote has no authenticated profile")?;
                *acquisition = Some(staged_context_resource_plan_v1(
                    borsh::object_length(profile.as_ref()).map_err(|e| e.to_string())? as u64,
                    *plan,
                    contract.config,
                )?);
                Some(PalwTokenizerInventoryPlanV1::derive_v1(cap, contract.config)?)
            }
            (Some(_), None) => {
                return Err("tokenizer bundle requires verifier's actual own-file cap, not producer declared length".into());
            }
            (None, None) => None,
            (None, Some(_)) => return Err("bundle substitutes an unrequired raw inventory".into()),
        };
        PalwStagedBundlePlanV1::derive_with_acquisitions_v1(contract.plan.terms.accepted_daa, families, inventory, contract.config)
            .map(Some)
    }
    pub fn staged_public_prefix_rows_v1(
        &self,
    ) -> impl Iterator<Item = &crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1> {
        self.staged_courts.values().filter_map(|row| match row {
            row => match row.active_row_v1() {
                Some(PalwStagedCourtRowV1::PublicPrefix(row)) => Some(row.as_ref()),
                _ => None,
            },
        })
    }
    pub fn staged_public_prefix_duties_after_v1(
        &self,
        signer: PalwBondKeyV2,
        now: u64,
        after: Option<(u64, Hash64, PalwBondKeyV2)>,
        limit: usize,
    ) -> Result<(Vec<&crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1>, Option<(u64, Hash64, PalwBondKeyV2)>), String>
    {
        use std::ops::Bound;
        if self.last_point().is_none_or(|p| p.daa_score != now) || limit == 0 {
            return Err("public response page changes accepted clock or has no bounded rows".into());
        }
        let Some(first_live) = now.checked_add(1) else { return Ok((Vec::new(), None)) };
        let lower = match after {
            Some((at, claim, owner)) if at >= first_live => Bound::Excluded((signer, at, claim, owner)),
            _ => Bound::Included((signer, first_live, ZERO_HASH64, PALW_BOND_KEY_V2_MIN)),
        };
        let limit = limit.min(64);
        let mut page = Vec::new();
        let mut cursor = None;
        for (_, deadline, claim, owner) in self
            .staged_prefix_responders
            .range((lower, Bound::Unbounded))
            .take_while(|(bond, _, _, _)| *bond == signer)
            .take(limit + 1)
        {
            if page.len() == limit {
                return Ok((page, cursor));
            }
            let Some(PalwStagedCourtRowV1::PublicPrefix(row)) =
                self.staged_owned_row_v1(*claim, *owner).and_then(PalwStagedCourtRowV1::active_row_v1)
            else {
                return Err("public response index has no literal owned row".into());
            };
            if row.pending_response_deadline_v1() != Some(*deadline)
                || !row.pending_progress_v1().is_some_and(|p| {
                    p.signers
                        .get(&signer)
                        .is_some_and(|s| *s != crate::palw_prefix_public_source_v1::PalwPrefixPublicSignerCursorV1::Invalid)
                })
            {
                return Err("public response index differs from saved live signer obligation".into());
            }
            page.push(row.as_ref());
            cursor = Some((*deadline, *claim, *owner));
        }
        Ok((page, None))
    }
    pub fn staged_owned_constraint_v1(
        &self,
        claim: Hash64,
        owner: PalwBondKeyV2,
    ) -> Option<&crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::Owner(claim, owner)).and_then(PalwStagedCourtRowV1::active_row_v1) {
            Some(PalwStagedCourtRowV1::Constraint(c)) => Some(c),
            _ => None,
        }
    }
    pub fn staged_selected_duties_after_v1(
        &self,
        signer: PalwBondKeyV2,
        now: u64,
        after: Option<(u64, Hash64, PalwBondKeyV2)>,
        limit: usize,
    ) -> Result<(Vec<(Hash64, PalwBondKeyV2)>, Option<(u64, Hash64, PalwBondKeyV2)>), String> {
        use std::ops::Bound;
        if self.last_point().is_none_or(|p| p.daa_score != now) || limit == 0 {
            return Err("selected duty page changes accepted clock or finite capacity".into());
        }
        let Some(first_live) = now.checked_add(1) else { return Ok((Vec::new(), None)) };
        let lower = match after {
            Some((at, claim, owner)) if at >= first_live => Bound::Excluded((signer, at, claim, owner)),
            _ => Bound::Included((signer, first_live, ZERO_HASH64, PALW_BOND_KEY_V2_MIN)),
        };
        let cap = limit.min(64);
        let mut page = Vec::new();
        let mut cursor = None;
        for (_, at, claim, owner) in self
            .staged_selected_responders
            .range((lower, Bound::Unbounded))
            .take_while(|(bond, _, _, _)| *bond == signer)
            .take(cap + 1)
        {
            if page.len() == cap {
                return Ok((page, cursor));
            }
            let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = self.staged_owned_row_v1(*claim, *owner) else {
                return Err("selected index has no paid frame".into());
            };
            let session = bundle.selected.as_ref().ok_or("selected index has no frozen session")?;
            if session.respond_until_daa != *at
                || !session.custodians.contains(&signer)
                || !session.requests.values().any(|r| !r.collected && !r.answered.contains(&signer))
            {
                return Err("selected index differs from exact signed custody".into());
            }
            page.push((*claim, *owner));
            cursor = Some((*at, *claim, *owner));
        }
        Ok((page, None))
    }
    /// Authenticated snapshot only; each page reads at most 64 indexed duties.
    pub fn staged_flat_duties_after_v1(
        &self,
        signer: PalwBondKeyV2,
        now: u64,
        after: Option<(u64, Hash64, PalwBondKeyV2)>,
        limit: usize,
    ) -> Result<(Vec<(Hash64, PalwBondKeyV2)>, Option<(u64, Hash64, PalwBondKeyV2)>), String> {
        use std::ops::Bound;
        if self.last_point().is_none_or(|p| p.daa_score != now) || limit == 0 {
            return Err("Flat duty page changes accepted clock or finite capacity".into());
        }
        let Some(first_live) = now.checked_add(1) else { return Ok((Vec::new(), None)) };
        let lower = match after {
            Some((at, claim, owner)) if at >= first_live => Bound::Excluded((signer, at, claim, owner)),
            _ => Bound::Included((signer, first_live, ZERO_HASH64, PALW_BOND_KEY_V2_MIN)),
        };
        let cap = limit.min(64);
        let mut page = Vec::new();
        let mut cursor = None;
        for (_, at, claim, owner) in
            self.staged_flat_responders.range((lower, Bound::Unbounded)).take_while(|(bond, _, _, _)| *bond == signer).take(cap + 1)
        {
            if page.len() == cap {
                return Ok((page, cursor));
            }
            let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = self.staged_owned_row_v1(*claim, *owner) else {
                return Err("Flat response index has no paid frame".into());
            };
            // The streamed-source index is shared by Flat, greedy Float32 and TIR prefix-state duties.
            if !flat_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone())).contains(&(signer, *at, *claim, *owner)) {
                return Err("Flat response index changes literal phase or original deadline".into());
            }
            page.push((*claim, *owner));
            cursor = Some((*at, *claim, *owner));
        }
        Ok((page, None))
    }
    pub fn staged_bond_exit_held_v1(&self, bond: &PalwBondKeyV2, now: u64) -> bool {
        if self.court_service_holds_bond_v1(bond) { return true; }
        now.checked_add(1).is_some_and(|after| {
            self.staged_holds
                .range((*bond, after, PalwStagedCourtKeyV1::Claim(ZERO_HASH64))..)
                .next()
                .is_some_and(|(held, _, _)| held == bond)
        })
    }
    pub fn staged_history_duties_after_v1(
        &self,
        signer: PalwBondKeyV2,
        now: u64,
        after: Option<(u64, Hash64, PalwBondKeyV2, Hash64)>,
        limit: usize,
    ) -> Result<(Vec<(Hash64, PalwBondKeyV2, Hash64)>, Option<(u64, Hash64, PalwBondKeyV2, Hash64)>), String> {
        use std::ops::Bound;
        if self.last_point().is_none_or(|p| p.daa_score != now) || limit == 0 {
            return Err("history duty page changes accepted clock or finite capacity".into());
        }
        let Some(first_live) = now.checked_add(1) else { return Ok((Vec::new(), None)) };
        let lower = match after {
            Some((at, claim, owner, seed)) if at >= first_live => Bound::Excluded((signer, at, claim, owner, seed)),
            _ => Bound::Included((signer, first_live, ZERO_HASH64, PALW_BOND_KEY_V2_MIN, ZERO_HASH64)),
        };
        let cap = limit.min(64);
        let mut page = Vec::new();
        let mut cursor = None;
        for (_, at, claim, owner, seed) in self
            .staged_history_responders
            .range((lower, Bound::Unbounded))
            .take_while(|(bond, _, _, _, _)| *bond == signer)
            .take(cap + 1)
        {
            if page.len() == cap {
                return Ok((page, cursor));
            }
            let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = self.staged_owned_row_v1(*claim, *owner) else {
                return Err("history response index has no paid owner frame".into());
            };
            let recovery = bundle.history_recovery.get(seed).ok_or("history index has no literal rooted stage cursor")?;
            if recovery.default.is_some()
                || !recovery.custodians.contains(&signer)
                || recovery.demand.as_ref().is_none_or(|d| d.answer_until_daa != *at)
            {
                return Err("history response index differs from exact original obligation".into());
            }
            page.push((*claim, *owner, *seed));
            cursor = Some((*at, *claim, *owner, *seed));
        }
        Ok((page, None))
    }
    /// Fixed original preparation duties, independent of owner Opens. This is
    /// metadata discovery only, not an admission, work or sanction credential.
    pub fn staged_tir_preparation_duties_after_v1(
        &self,
        signer: PalwBondKeyV2,
        now: u64,
        after: Option<(u64, Hash64, Hash64)>,
        limit: usize,
    ) -> Result<(Vec<(Hash64, Hash64)>, Option<(u64, Hash64, Hash64)>), String> {
        use std::ops::Bound;
        if self.last_point().is_none_or(|p| p.daa_score != now) || limit == 0 {
            return Err("TIR preparation page changes accepted snapshot clock or finite capacity".into());
        }
        let Some(first_live) = now.checked_add(1) else { return Ok((Vec::new(), None)) };
        let lower = match after {
            Some((at, claim, allocation)) if at >= first_live => Bound::Excluded((signer, at, claim, allocation)),
            _ => Bound::Included((signer, first_live, ZERO_HASH64, ZERO_HASH64)),
        };
        let cap = limit.min(64);
        let mut page = Vec::with_capacity(cap);
        let mut cursor = None;
        for (_, at, claim, allocation) in self.staged_tir_preparers.range((lower, Bound::Unbounded))
            .take_while(|(bond, ..)| *bond == signer).take(cap + 1)
        {
            if page.len() == cap { return Ok((page, cursor)); }
            let row = self.staged_tir_preparation_v1(*claim, *allocation)
                .ok_or("TIR preparation index lost its original claim-wide row")?;
            let c = row.preparation.contract_v1();
            if c.preparation_until_daa != *at || now < c.original_accepted_daa || now >= c.material_until_daa
                || !row.preparation.pending_signers_v1().contains(&signer)
            {
                return Err("TIR preparation index differs from original unpaid literal duty".into());
            }
            page.push((*claim, *allocation));
            cursor = Some((*at, *claim, *allocation));
        }
        Ok((page, None))
    }
    /// Accepted-snapshot EDF page over TIR acquisition's exact outstanding
    /// literal slots. Metadata only, at most64 rows plus one indexed lookahead.
    /// This reader cannot create a grant, paid allocation or collection right.
    pub fn staged_tir_duties_after_v1(
        &self,
        signer: PalwBondKeyV2,
        now: u64,
        after: Option<(u64, Hash64, PalwBondKeyV2, Hash64, u64, Hash64)>,
        limit: usize,
    ) -> Result<
        (Vec<(Hash64, PalwBondKeyV2, Hash64, u64, Hash64)>, Option<(u64, Hash64, PalwBondKeyV2, Hash64, u64, Hash64)>),
        String,
    > {
        use std::ops::Bound;
        if self.last_point().is_none_or(|p| p.daa_score != now) || limit == 0 {
            return Err("TIR duty page changes accepted snapshot clock or finite capacity".into());
        }
        let Some(first_live) = now.checked_add(1) else { return Ok((Vec::new(), None)) };
        let lower = match after {
            Some((at, claim, owner, allocation, turn, demand)) if at >= first_live => {
                Bound::Excluded((signer, at, claim, owner, allocation, turn, demand))
            }
            _ => Bound::Included((signer, first_live, ZERO_HASH64, PALW_BOND_KEY_V2_MIN, ZERO_HASH64, 0, ZERO_HASH64)),
        };
        let cap = limit.min(64);
        let mut page = Vec::with_capacity(cap);
        let mut cursor = None;
        for (_, at, claim, owner, allocation, turn, demand) in self.staged_tir_responders
            .range((lower, Bound::Unbounded)).take_while(|(bond, ..)| *bond == signer).take(cap + 1)
        {
            if page.len() == cap { return Ok((page, cursor)); }
            let row = self.staged_tir_acquisition_v1(*claim, *owner, *allocation)
                .ok_or("TIR duty index has no literal allocation row")?;
            let c = row.owned.contract_v1();
            let pending = row.owned.active_v1().ok_or("TIR duty index lost its accepted demand")?;
            if pending.collected || pending.respond_until_daa != *at || pending.turn != *turn || pending.demand_id != *demand
                || !row.owned.response_custodians_v1().contains(&signer) || pending.slots.get(&signer).is_some_and(|slot| slot.finished.is_some())
                || now < c.reserved_daa || now >= c.terms.material_until_daa
            {
                return Err("TIR duty index differs from exact unpaid signer/source/turn/clock".into());
            }
            page.push((*claim, *owner, *allocation, *turn, *demand));
            cursor = Some((*at, *claim, *owner, *allocation, *turn, *demand));
        }
        Ok((page, None))
    }
}
impl TransitionBuilder<'_> {
    /// Pure preflight: checked clocks are established before even an attempt's first write.
    pub(super) fn prepare_staged_claim_v1(
        &self,
        claim: &mut PalwClaimStateV2,
    ) -> Result<Option<PalwStagedCourtRowV1>, PalwStateV2Error> {
        self.prepare_staged_claim_with_job_v1(ZERO_HASH64, claim, None)
    }
    pub(super) fn prepare_staged_claim_with_job_v1(
        &self,
        claim_id: Hash64,
        claim: &mut PalwClaimStateV2,
        public_job: Option<&crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
    ) -> Result<Option<PalwStagedCourtRowV1>, PalwStateV2Error> {
        let Some(config) = self.extras.staged_public_court else {
            return Ok(None);
        };
        if let Some(original) = public_job.filter(|original| self.state.staged_required_families_v1(original)) {
            return self.prepare_required_bundle_v1(claim_id, claim, original, config);
        }
        let vocab = self.state.fp_work_profiles.get(&claim.class_id).map(|p| p.vocab_size).unwrap_or((1 << 22) - 1);
        // The authenticated class profile narrows vocabulary. The existing generated-position
        // format caps every other claim independently of a producer's asserted prompt/rendering size.
        let mut terms = config.claim_terms(claim.accepted_daa, 4095, vocab).map_err(PalwStateV2Error::CarriageInconsistent)?;
        if let Some(registered) = self.state.class_semantics.get(&claim.class_id)
            && let (Some(raw), Some(bound)) = (registered.raw_tokenizer_bytes, registered.tokenizer_key_events)
        {
            let tokenizer =
                crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(raw, bound, config.prices)
                    .and_then(|p| p.court_envelope(vocab, config.prices))
                    .map_err(PalwStateV2Error::CarriageInconsistent)?;
            terms = crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1::derive(
                claim.accepted_daa,
                terms.plan.cover(tokenizer),
                config.acquisition_daa,
                config.collection_daa,
                config.reorg_retention_daa,
            )
            .map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        terms
            .plan
            .rent_sompi
            .checked_add(terms.plan.deposit_sompi)
            .ok_or(PalwStateV2Error::Overflow("prospective complete court funding"))?;
        claim.trace_retention_daa = claim.trace_retention_daa.max(terms.material_until_daa);
        Ok(Some(PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 {
            terms,
            producer: claim.bond,
            panel: BTreeSet::new(),
            prefix_custody: None,
        })))
    }
    fn prepare_required_bundle_v1(
        &self,
        claim_id: Hash64,
        claim: &mut PalwClaimStateV2,
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
    ) -> Result<Option<PalwStagedCourtRowV1>, PalwStateV2Error> {
        // The staged public court needs the commitment's job pin on its own claims even where
        // `palw_offence_attribution` has not yet stamped it: every public court derives the
        // offence target's job identity from this field. Never overwrite a different pin.
        let pin = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original);
        if claim.job_identity == Hash64::default() {
            claim.job_identity = pin;
        } else if claim.job_identity != pin {
            return Err(stage_refused(claim_id, "required-family claim job identity differs from its original commitment pin"));
        }
        let header = self
            .extras
            .public_original_bindings
            .get(&claim_id)
            .ok_or_else(|| stage_refused(claim_id, "prospective claim lacks its transaction-local original public binding"))?;
        let maximum_custodians =
            self.state.staged_maximum_custodians_v1(claim.class_id, claim.accepted_daa, self.params, self.extras)?;
        let ladder = self.state.class_step_ladder_v1(
            &claim.class_id,
            self.extras.held_context_ladder.or(self.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
        );
        let quote = self
            .state
            .staged_preclaim_quote_v1(
                original,
                header,
                claim.accepted_daa,
                maximum_custodians,
                ladder,
                config,
                crate::palw_staged_preclaim_v1::PalwStagedRouteGatesV1::of_extras_v1(self.extras),
            )
            .map_err(|e| stage_refused(claim_id, e))?;
        let Some(quote) = quote else {
            // No staged semantic family is claimed by this descriptor. This is
            // not a certificate covering the remaining ordinary FP predicates.
            let terms = config
                .claim_terms(
                    claim.accepted_daa,
                    4095,
                    self.state
                        .fp_work_profiles
                        .get(&claim.class_id)
                        .ok_or_else(|| stage_refused(claim_id, "missing native profile"))?
                        .vocab_size,
                )
                .map_err(|e| stage_refused(claim_id, e))?;
            claim.trace_retention_daa = claim.trace_retention_daa.max(terms.material_until_daa);
            return Ok(Some(PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 {
                terms,
                producer: claim.bond,
                panel: BTreeSet::new(),
                prefix_custody: None,
            })));
        };
        let semantics = self
            .state
            .class_semantics
            .get(&claim.class_id)
            .ok_or_else(|| stage_refused(claim_id, "required family immutable semantics absent"))?;
        let pledge = self
            .extras
            .history_custody_pledges
            .get(&claim_id)
            .ok_or_else(|| {
                stage_refused(claim_id, "required-family claim lacks its transaction-local original signed history pledge")
            })?
            .clone();
        pledge.check_v1().map_err(|e| stage_refused(claim_id, e))?;
        let producer = self.state.bond(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?;
        pledge
            .validate_signature_v1(&producer.pubkey, crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
            .map_err(|e| stage_refused(claim_id, e))?;
        if (
            pledge.claim,
            pledge.producer,
            pledge.execution_root,
            pledge.job_identity,
            pledge.cu_ruleset_id,
            pledge.resource_plan_id,
            pledge.maximum_records,
            pledge.maximum_custodians,
        ) != (
            claim_id,
            claim.bond,
            claim.execution_root,
            claim.job_identity,
            semantics.cu_ruleset_id,
            quote.resource_plan_id,
            quote.maximum_records,
            quote.maximum_custodians,
        ) || claim.accepted_daa >= pledge.accept_until_daa
            || quote.plan.terms.material_until_daa > pledge.material_until_daa
        {
            return Err(stage_refused(claim_id, "original pledge changes native EOF, family SUM, identity, capacity or paid horizon"));
        }
        // An external producer's node custodian: both keys signed the standing window, it covers
        // this acceptance and the whole pledged material horizon, and the custodian is an active
        // distinct bond. An invalid delegation refuses the claim; an absent one leaves the producer
        // its own custodian.
        let producer_custodian = match self.extras.producer_custody_delegations.get(&claim_id) {
            None => None,
            Some(delegation) => {
                let custodian = self.state.bond(&delegation.custodian).ok_or(PalwStateV2Error::MissingBond(delegation.custodian))?;
                if delegation.producer != claim.bond
                    || !delegation.covers_v1(claim.accepted_daa, pledge.material_until_daa)
                    || !matches!(custodian.status, crate::palw_state_v2::PalwBondStatusV2::Active)
                {
                    return Err(stage_refused(claim_id, "producer custody delegation is foreign, out of window or names an inactive custodian"));
                }
                delegation
                    .validate_signatures_v1(
                        &producer.pubkey,
                        &custodian.pubkey,
                        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
                    )
                    .map_err(|e| stage_refused(claim_id, e))?;
                Some(delegation.custodian)
            }
        };
        let terms = quote.plan.terms;
        terms
            .plan
            .rent_sompi
            .checked_add(terms.plan.deposit_sompi)
            .ok_or(PalwStateV2Error::Overflow("prospective complete court funding"))?;
        let custody = crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1::new(
            claim.execution_root,
            claim.job_identity,
            semantics.cu_ruleset_id,
            &terms,
        );
        claim.trace_retention_daa = claim.trace_retention_daa.max(terms.material_until_daa);
        Ok(Some(PalwStagedCourtRowV1::ClaimBundle(Box::new(crate::palw_staged_bundle_v1::PalwStagedBundleContractV1 {
            claim: PalwStagedClaimContractV1 {
                terms,
                producer: claim.bond,
                panel: BTreeSet::new(),
                prefix_custody: Some(custody.clone()),
            },
            plan: quote.plan,
            config,
            original_binding: header.clone(),
            producer_custodian,
            history_custody: crate::palw_history_custody_v1::PalwHistoryCustodyContractV1 {
                custody,
                producer: claim.bond,
                maximum_records: quote.maximum_records,
                maximum_custodians: quote.maximum_custodians,
                pledge,
            },
        }))))
    }
    /// Proposed native journal adapter. Production block/admission callers are
    /// still absent. Recovery cannot clear a binding, move a duty deadline,
    /// release collateral, or turn quota into default permission.
    pub(super) fn recover_court_service_supply_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
    ) -> Result<Vec<crate::palw_court_service_state_v2::ClaimServiceRecovery>, PalwStateV2Error> {
        let Some(old) = self.state.court_service_state_v1() else { return Ok(Vec::new()) };
        if old.book.last_observed_daa != Some(ctx.daa_score) {
            return Err(PalwStateV2Error::CarriageInconsistent("court service recovery needs the current accepting-block supply clock".into()));
        }
        let mut next = old.clone();
        let report = next.repair_missing_supply().map_err(PalwStateV2Error::CarriageInconsistent)?;
        if &next != old {
            self.write_staged_row_v1(PalwStagedCourtKeyV1::CourtService,
                Some(PalwStagedCourtRowV1::CourtService(Box::new(next).into())))?;
        }
        Ok(report)
    }

    pub(super) fn write_staged_row_v1(&mut self, key: PalwStagedCourtKeyV1, new: Option<PalwStagedCourtRowV1>) -> Result<(), PalwStateV2Error> {
        if let Some(PalwStagedCourtRowV1::CourtDutyClock(clock)) = &new {
            clock.check(&key).and_then(|()| clock.check_context(&self.state)).map_err(PalwStateV2Error::CarriageInconsistent)?;
            if self.state.staged_court_row_v1(&key).is_some_and(|old| Some(old) != new.as_ref()) {
                return Err(PalwStateV2Error::CarriageInconsistent("native duty clock is immutable".into()));
            }
        }
        if key == PalwStagedCourtKeyV1::CourtService {
            for row in self.state.staged_courts.values() {
                if let PalwStagedCourtRowV1::CourtDutyClock(clock) = row {
                    let Some(PalwStagedCourtRowV1::CourtService(service)) = &new else {
                        return Err(PalwStateV2Error::CarriageInconsistent("unresolved duty history retains service binding".into()));
                    };
                    clock.check_service_binding(service).map_err(PalwStateV2Error::CarriageInconsistent)?;
                }
            }
        }
        if let Some(PalwStagedCourtRowV1::CourtService(service)) = &new {
            service.check_context(&self.state).map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        if new.is_none() && self.state.court_service_holds_material_v1(&key) {
            return Err(PalwStateV2Error::CarriageInconsistent("unresolved court service retains original evidence".into()));
        }
        self.reserve_map_row_v1(&key, &new, |s| &s.staged_courts, false, "native staged_courts writer")?;
        self.reserve_staged_indices_v1(&key, &new)?;
        let old = match &new {
            Some(row) => self.state.staged_courts.insert(key, row.clone()),
            None => self.state.staged_courts.remove(&key),
        };
        update_indices(&mut self.state, key, old.as_ref(), new.as_ref());
        if key == PalwStagedCourtKeyV1::CourtService {
            if let Some(PalwStagedCourtRowV1::CourtService(service)) = &new {
                // Do not re-date unrelated claims midway through an accepting
                // block. This candidate only installs/updates unresolved holds.
                self.state.deadlines.retain(|(_, claim)| !service.claims.contains_key(claim));
            }
        }
        if old != new {
            self.entries.push(PalwDeltaEntryV2::StagedCourtSeed { key, old, new });
        }
        Ok(())
    }
    /// RFC-0006: what `update_indices` allocates for this write, bounded from the ACTUAL old and new rows without calling the
    /// Vec-returning index helpers. Every derived index row (responder/prefix/selected/flat/history/TIR/preparer/hold) is a
    /// projection of one nested typed membership of the row, so `native_members_v1() + 1` bounds each family's output; the
    /// helpers' scratch Vecs (old and new) and the new index nodes (sized by the largest index tuple) are admitted together.
    fn reserve_staged_indices_v1(
        &mut self,
        key: &PalwStagedCourtKeyV1,
        new: &Option<PalwStagedCourtRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1};
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let members = |row: Option<&PalwStagedCourtRowV1>| -> Result<u64, PalwStateV2Error> {
            row.map_or(Some(0), NativeHeapV1::native_members_v1).and_then(|m| m.checked_add(1)).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwStagedCourtRowV1>("staged index membership count"))
            })
        };
        let old_m = members(self.state.staged_courts.get(key))?;
        let new_m = members(new.as_ref())?;
        // The widest index tuple (`staged_tir_responders`) bounds every family's element and node size.
        type Widest = (PalwBondKeyV2, u64, Hash64, PalwBondKeyV2, Hash64, u64, Hash64);
        const FAMILIES: u64 = 9;
        let mut q = super::native_writers::NativeWriteQuoteV1::new("native staged court index scratch and nodes");
        for m in [old_m, new_m] {
            for _ in 0..FAMILIES {
                q.grown::<Widest>(m)?;
            }
        }
        let nodes = new_m.checked_mul(FAMILIES).ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwStagedCourtRowV1>("staged index node count"))
        })?;
        q.member::<Widest, ()>(nodes)?;
        // `staged_tir_by_claim` may add an outer bucket for a new claim.
        q.member::<Hash64, BTreeSet<(u64, PalwStagedCourtKeyV1)>>(u64::from(new.is_some()))?;
        self.admit_native_write_v1(q)
    }
    pub(super) fn stamp_staged_claim_v1(&mut self, claim: Hash64, contract: Option<PalwStagedCourtRowV1>) -> Result<(), PalwStateV2Error> {
        if let Some(c) = contract {
            self.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(claim), Some(c))?;
        }
        Ok(())
    }
    fn replace_staged_contract_v1(&mut self, claim: Hash64, contract: PalwStagedClaimContractV1) -> Result<(), PalwStateV2Error> {
        let row = match self.state.staged_courts.get(&PalwStagedCourtKeyV1::Claim(claim)) {
            Some(PalwStagedCourtRowV1::ClaimBundle(bundle)) => {
                let mut bundle = bundle.clone();
                bundle.claim = contract;
                PalwStagedCourtRowV1::ClaimBundle(bundle)
            }
            _ => PalwStagedCourtRowV1::Claim(contract),
        };
        self.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(claim), Some(row))?;
        Ok(())
    }
    pub(super) fn record_staged_panel_v1(&mut self, claim: Hash64, panel: &PalwPanelStateV2) -> Result<(), PalwStateV2Error> {
        if let Some(mut contract) = self.state.staged_claim_contract_v1(&claim).cloned() {
            contract.panel.extend(panel.seats.iter().map(|seat| seat.bond));
            if let Some(custody) = &mut contract.prefix_custody {
                custody.expected_panel = panel.seats.iter().map(|seat| seat.bond).collect();
            }
            self.replace_staged_contract_v1(claim, contract)?;
            if let Some(PalwStagedCourtRowV1::ClaimBundle(mut bundle)) =
                self.state.staged_courts.get(&PalwStagedCourtKeyV1::Claim(claim)).cloned()
            {
                bundle.history_custody.custody.expected_panel =
                    panel.seats.iter().map(|seat| seat.bond).filter(|bond| *bond != bundle.history_custody.producer).collect();
                self.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(claim), Some(PalwStagedCourtRowV1::ClaimBundle(bundle)))?;
            }
        }
        Ok(())
    }
    /// Called only on actual signed Valid credit edges; ordinary membership/Sampled pay is
    /// insufficient. Qualification remains claim-rooted after duty/receipt pruning.
    pub(super) fn record_staged_prefix_receipts_v1(
        &mut self,
        claim: Hash64,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        credited: &[PalwBondKeyV2],
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.extras.staged_public_court.is_none() {
            return Ok(());
        }
        let Some(mut contract) = self.state.staged_claim_contract_v1(&claim).cloned() else {
            return Ok(());
        };
        let Some(custody) = &mut contract.prefix_custody else {
            return Ok(());
        };
        let mut changed = false;
        for receipt in receipts.iter().filter(|r| {
            credited.contains(&r.seat_bond)
                || self.state.panel_duties.get(&claim).is_some_and(|duties| duties.seats.get(&r.seat_bond).is_some_and(|at| *at != 0))
        }) {
            changed |= custody.qualify(claim, receipt, now);
        }
        if changed {
            self.replace_staged_contract_v1(claim, contract)?;
        }
        Ok(())
    }
}

fn stage_refused(claim: Hash64, why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ObjectiveOffenceRefused(claim, why.into())
}
fn gate<'a>(
    b: &'a TransitionBuilder<'_>,
    claim: &Hash64,
    owner: PalwBondKeyV2,
    reserved: u64,
    now: u64,
) -> Result<&'a PalwStagedClaimContractV1, PalwStateV2Error> {
    gate_clock(b, claim, owner, reserved, now, false)
}
fn gate_clock<'a>(
    b: &'a TransitionBuilder<'_>,
    claim: &Hash64,
    owner: PalwBondKeyV2,
    reserved: u64,
    now: u64,
    collecting: bool,
) -> Result<&'a PalwStagedClaimContractV1, PalwStateV2Error> {
    if b.extras.staged_public_court.is_none() {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let c =
        b.state.staged_claim_contract_v1(claim).ok_or_else(|| stage_refused(*claim, "claim has no prospective staged contract"))?;
    let (literal_reserved, terms) = match b.state.legacy_dispute_reservation_v1(claim, &owner) {
        Some(reservation) => (reservation.reserved_daa, c.terms),
        None => {
            let paid = crate::palw_staged_court_state_v1::palw_staged_paid_authority_v1(&b.state, (*claim, owner))
                .map_err(|e| stage_refused(*claim, e))?
                .ok_or_else(|| stage_refused(*claim, "staged owner has neither literal reserve nor paid continuation"))?;
            (paid.reserved_daa, paid.terms)
        }
    };
    if literal_reserved != reserved
        || now < terms.accepted_daa
        || now >= if collecting { terms.collect_until_daa } else { terms.stream_until_daa }
        || now >= terms.material_until_daa
    {
        return Err(stage_refused(*claim, "staged owner is not the literal live reservation or its admitted clock ended"));
    }
    Ok(c)
}
pub(super) fn open(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    seed: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_staged_bundle_v1::PalwStagedFamilyV1 as Family;
    let key = PalwStagedCourtKeyV1::Owner(seed.claim, seed.reserver);
    let bundle = match b.state.staged_courts.get(&key) {
        Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => Some(bundle.clone()),
        _ => None,
    };
    if bundle.is_none()
        && b.state.legacy_dispute_reservation_v1(&seed.claim, &seed.reserver).is_none_or(|r| r.reserved_daa != seed.reserved_daa)
    {
        return Err(stage_refused(seed.claim, "new staged seed requires its literal live reservation"));
    }
    let mut contract = if let Some(bundle) = &bundle {
        if seed.reserved_daa != bundle.reserved_daa
            || ctx.daa_score < bundle.plan.terms.accepted_daa
            || ctx.daa_score >= bundle.plan.terms.stream_until_daa
            || ctx.daa_score >= bundle.plan.terms.material_until_daa
        {
            return Err(stage_refused(seed.claim, "child seed changes original prepaid frame clock"));
        }
        if b.extras.staged_public_court.is_none() {
            return Err(PalwStateV2Error::ObjectiveOffenceDormant);
        }
        b.state
            .staged_claim_contract_v1(&seed.claim)
            .ok_or_else(|| stage_refused(seed.claim, "paid child lacks original claim contract"))?
            .clone()
    } else {
        gate(b, &seed.claim, seed.reserver, seed.reserved_daa, ctx.daa_score)?.clone()
    };
    if let Some(bundle) = &bundle {
        bundle.check_v1().map_err(|e| stage_refused(seed.claim, e))?;
        if bundle.reserved_daa != seed.reserved_daa
            || bundle.opened.len() >= bundle.plan.stages.len()
            || bundle.opened.len() != bundle.active.map_or(0, |n| n as usize + 1)
            || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
            || bundle.inventory.as_ref().is_some_and(|intake| !intake.sealed || intake.length_fault_v1())
        {
            return Err(stage_refused(seed.claim, "child seed requires an honest completed predecessor and authenticated intake"));
        }
        let family = match &seed.auxiliary {
            Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::PublicPrefix { .. }) => {
                Family::PublicPrefix
            }
            Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::Tokenizer { .. }) => Family::Tokenizer,
            None
            | Some(
                crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FlatConstraint { .. }
                | crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FloatConstraint { .. },
            ) => Family::Constraint,
            _ => return Err(stage_refused(seed.claim, "bundle requires the exact public required family")),
        };
        let next = &bundle.plan.stages[bundle.opened.len()];
        if next.family != family {
            return Err(stage_refused(seed.claim, "child seed substitutes required family"));
        }
        contract.terms = next.terms;
    } else if matches!(b.state.staged_courts.get(&PalwStagedCourtKeyV1::Claim(seed.claim)), Some(PalwStagedCourtRowV1::ClaimBundle(_)))
    {
        return Err(stage_refused(seed.claim, "required family claim needs prepaid BundleOpen before a child seed"));
    }
    if ctx.daa_score >= contract.terms.open_until_daa {
        return Err(stage_refused(seed.claim, "seed is after immutable prospective opening deadline"));
    }
    if bundle.is_none() && b.state.staged_courts.contains_key(&key) {
        return Err(stage_refused(seed.claim, "owned staged seed is immutable"));
    }
    let allowance = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?.maximum_owned_rows_per_reserver;
    // Only this reserver's paid rows consume its allowance. No claim-wide first-writer cap.
    if bundle.is_none() && b.state.staged_owner_allowance_used_v1(seed.reserver, allowance as usize) >= allowance as usize {
        return Err(stage_refused(seed.claim, "reserver's own staged work allowance is occupied"));
    }
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &seed.claim)
        .ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
    let binding = seed
        .resolved_binding_v1(target.class_id, b.state.fp_work_profiles.get(&target.class_id).map(|p| p.as_ref()))
        .map_err(|e| stage_refused(seed.claim, e))?;
    if crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(&target, &binding, b.identity_rules_v1(ctx.daa_score), false)
        .map_err(|e| stage_refused(seed.claim, e.to_string()))?
        .is_some()
    {
        return Err(stage_refused(seed.claim, "staged seed binding answers another claim's job"));
    }
    let original = b
        .state
        .fp_public_jobs
        .get(&seed.claim)
        .ok_or_else(|| stage_refused(seed.claim, "staged selection seed requires accepted original public FP job"))?;
    let original: crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 =
        borsh::from_slice(original).map_err(|_| stage_refused(seed.claim, "accepted public FP job encoding invalid"))?;
    if seed.job.as_deref().is_some_and(|j| *j != original.job) {
        return Err(stage_refused(seed.claim, "staged seed changes accepted original FP job"));
    }
    if contract.panel.is_empty() {
        return Err(stage_refused(seed.claim, "staged contract has no actual bound Panel"));
    }
    use crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1 as Auxiliary;
    let prefix = match &seed.auxiliary {
        Some(Auxiliary::Prefix { maximum_leaves, prefix_tokens, relation }) => {
            Some((*maximum_leaves, *prefix_tokens, *relation, None))
        }
        Some(Auxiliary::PublicPrefix { maximum_leaves, prefix_tokens, relation, slots }) => {
            Some((*maximum_leaves, *prefix_tokens, *relation, Some(*slots)))
        }
        _ => None,
    };
    if let Some((maximum_leaves, prefix_tokens, relation, public_slots)) = prefix {
        let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
        let ladder = b.state.class_step_ladder_v1(
            &target.class_id,
            b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
        );
        let named =
            crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(&original.job).map_err(|e| stage_refused(seed.claim, e.to_string()))?;
        if maximum_leaves != ladder
            || prefix_tokens != named.prefix_tokens
            || target.registered_semantics.as_ref().and_then(|s| s.prefix_state_relation) != Some(relation)
        {
            return Err(stage_refused(seed.claim, "prefix fee metadata changes actual job, immutable relation or rule ladder"));
        }
        let base = crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1::derive(
            &binding.shape_profile,
            relation,
            prefix_tokens,
            binding.step_leaf_count,
            90_000,
            config.prices,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        let public_custody = public_slots
            .map(|_| {
                contract
                    .prefix_custody
                    .as_ref()
                    .map(|c| c.current_signed_v1())
                    .ok_or_else(|| stage_refused(seed.claim, "no signed prefix custody"))
            })
            .transpose()?;
        let acquisition = if let Some(slots) = public_slots {
            let custody = public_custody.as_ref().expect("public seed selected above");
            custody.check(&contract.panel).map_err(|e| stage_refused(seed.claim, e))?;
            let actual_slots =
                custody.expected_panel.iter().copied().chain([contract.producer, seed.reserver]).collect::<BTreeSet<_>>().len();
            if !custody.public_qualified_v1()
                || slots as usize != actual_slots
                || contract.panel.contains(&seed.reserver)
                || contract.producer == seed.reserver
                || custody.job_identity != target.job_identity
                || custody.cu_ruleset_id != binding.job_context.cu_ruleset_id
            {
                return Err(stage_refused(seed.claim, "public prefix requires exact qualified custody and an independent owner"));
            }
            Some(
                crate::palw_prefix_public_source_v1::PalwPrefixPublicAcquisitionPlanV1::derive(
                    base,
                    actual_slots,
                    config.prices,
                    borsh::object_length(&binding.shape_profile).map_err(|e| stage_refused(seed.claim, e.to_string()))? as u64,
                )
                .map_err(|e| stage_refused(seed.claim, e))?,
            )
        } else {
            None
        };
        let plan = acquisition.as_ref().map_or(base.envelope, |p| p.envelope);
        let extended_terms = frozen_or_extended_terms_v1(&contract.terms, plan, ctx.daa_score, bundle.is_some())
            .map_err(|e| stage_refused(seed.claim, e))?;
        let mut extended_claim = b.state.claim(&seed.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
        // X3 (REQ-X3-3, `palw_claim_hard_deadline_v1`): a claim accepted past the fence has its deadlines fixed at acceptance — a
        // paid stage that would move its retention (and with it the hard deadline: `hard.max(new)` below) is refused, before any
        // write. Below the fence the extension is the OLD rule, unchanged.
        if extended_terms.material_until_daa > extended_claim.trace_retention_daa
            && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(b.params, &extended_claim)
        {
            return Err(stage_refused(seed.claim, "X3: a paid stage cannot extend a claim's deadlines fixed at acceptance"));
        }
        extended_claim.trace_retention_daa = extended_claim.trace_retention_daa.max(extended_terms.material_until_daa);
        let hard = crate::palw_legacy_public_filer_v1::palw_dispute_hard_deadline_v1(
            &extended_claim,
            palw_da_disclose_window_daa_v1(b.params),
        );
        let until = hard
            .checked_add(b.params.window_challenge_at(ctx.daa_score))
            .ok_or(PalwStateV2Error::Overflow("paid prefix DA extension"))?
            .max(extended_terms.bond_exit_after_daa);
        super::palw_exec_v2_rewards_v1::require_native_session_window(&b.state, b.params, &seed.claim, until)?;
        if bundle.is_none() {
            b.check_accuser_room(seed.reserver, u128::from(plan.deposit_sompi), "staged prefix court", ctx.daa_score)?;
        }
        let funding_plan = if bundle.is_some() { extended_terms.plan } else { plan };
        let funded = funding_plan
            .rent_sompi
            .checked_add(funding_plan.deposit_sompi)
            .ok_or(PalwStateV2Error::Overflow("staged prefix funding"))?;
        let exits = vec![extended_terms.bond_exit_after_daa; contract.panel.len()];
        let encoded = borsh::to_vec(seed).map_err(|e| stage_refused(seed.claim, e.to_string()))?;
        let message = seed.signing_message();
        let seed_id = Hash64::from_bytes(message.try_into().map_err(|_| stage_refused(seed.claim, "prefix seed identity length"))?);
        let row = crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1::admit(
            seed_id,
            seed.claim,
            seed.reserver,
            seed.reserved_daa,
            &binding,
            &original.job,
            &target,
            extended_terms,
            config.prices,
            ctx.daa_score,
            funded,
            90_000,
            ladder,
            encoded.len() as u64,
            extended_claim.trace_retention_daa,
            extended_terms.bond_exit_after_daa,
            &exits,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        let row = if let Some(acquisition) = acquisition {
            let custody = public_custody.expect("checked above");
            let row = if bundle.is_some() {
                crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1::admit_with_allocation_v1(
                    row, contract.producer, custody, acquisition, extended_terms, ladder, funded,
                )
            } else {
                crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1::admit(
                    row, contract.producer, custody, acquisition, extended_terms, ladder, funded,
                )
            }
            .map_err(|e| stage_refused(seed.claim, e))?;
            PalwStagedCourtRowV1::PublicPrefix(Box::new(row).into())
        } else {
            PalwStagedCourtRowV1::Prefix(Box::new(row).into())
        };
        write_admitted_family_v1(b, key, row, bundle, seed, &binding, &original.job)?;
        if extended_terms != contract.terms {
            b.write_staged_row_v1(
                PalwStagedCourtKeyV1::Claim(seed.claim),
                Some(PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 { terms: extended_terms, ..contract.clone() })),
            )?;
            b.write_claim(seed.claim, Some(extended_claim))?;
            if let Some(mut dispute) = b.state.legacy_disputes.get(&seed.claim).cloned() {
                dispute.hard_deadline_daa = dispute.hard_deadline_daa.max(hard);
                b.write_legacy_dispute(seed.claim, Some(dispute))?;
            }
            b.da_rekey_v1(seed.claim, until, ctx.daa_score)?;
            if let Some(mut liability) = b.state.panel_liabilities.get(&seed.claim).cloned()
                && liability.voided_daa.is_none()
                && liability.void_reason.is_none()
                && liability.expiry_daa < until
            {
                liability.expiry_daa = until;
                b.write_panel_liability(seed.claim, Some(liability))?;
            }
            for (lock_key, mut lock) in b.claim_locks_v1(&seed.claim) {
                if lock.expiry_daa < until {
                    lock.expiry_daa = until;
                    b.write_slashable_lock(lock_key, Some(lock))?;
                }
            }
        }
        return Ok(());
    }
    if matches!(&seed.auxiliary, Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::Tokenizer { .. })) {
        let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
        let semantics = target
            .registered_semantics
            .as_ref()
            .ok_or_else(|| stage_refused(seed.claim, "tokenizer court has no immutable claim semantics"))?;
        let (Some(_raw), Some(bound)) = (semantics.raw_tokenizer_bytes, semantics.tokenizer_key_events) else {
            return Err(stage_refused(seed.claim, "historical tokenizer has no prospective raw/work obligation"));
        };
        let Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::Tokenizer {
            operand_tokenizer_bytes, ..
        }) = &seed.auxiliary
        else {
            unreachable!()
        };
        let plan = crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(
            *operand_tokenizer_bytes,
            bound,
            config.prices,
        )
        .and_then(|p| p.court_envelope(binding.shape_profile.vocab_size, config.prices))
        .map_err(|e| stage_refused(seed.claim, e))?;
        let extended_terms = frozen_or_extended_terms_v1(&contract.terms, plan, ctx.daa_score, bundle.is_some())
            .map_err(|e| stage_refused(seed.claim, e))?;
        let mut extended_claim = b.state.claim(&seed.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
        // X3 (REQ-X3-3, `palw_claim_hard_deadline_v1`): a claim accepted past the fence has its deadlines fixed at acceptance — a
        // paid stage that would move its retention (and with it the hard deadline: `hard.max(new)` below) is refused, before any
        // write. Below the fence the extension is the OLD rule, unchanged.
        if extended_terms.material_until_daa > extended_claim.trace_retention_daa
            && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(b.params, &extended_claim)
        {
            return Err(stage_refused(seed.claim, "X3: a paid stage cannot extend a claim's deadlines fixed at acceptance"));
        }
        extended_claim.trace_retention_daa = extended_claim.trace_retention_daa.max(extended_terms.material_until_daa);
        let hard = crate::palw_legacy_public_filer_v1::palw_dispute_hard_deadline_v1(
            &extended_claim,
            palw_da_disclose_window_daa_v1(b.params),
        );
        let until = hard
            .checked_add(b.params.window_challenge_at(ctx.daa_score))
            .ok_or(PalwStateV2Error::Overflow("paid DA extension"))?
            .max(extended_terms.bond_exit_after_daa);
        // EXECv2 retains its independently frozen horizon. A prospective stage cannot silently
        // reinterpret that historical contract to buy a later native reward/DA window.
        super::palw_exec_v2_rewards_v1::require_native_session_window(&b.state, b.params, &seed.claim, until)?;
        if bundle.is_none() {
            b.check_accuser_room(seed.reserver, u128::from(plan.deposit_sompi), "staged tokenizer court", ctx.daa_score)?;
        }
        let funded = plan.rent_sompi.checked_add(plan.deposit_sompi).ok_or(PalwStateV2Error::Overflow("staged tokenizer funding"))?;
        let exits = vec![extended_terms.bond_exit_after_daa; contract.panel.len()];
        let row = crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1::admit_with_resolved_binding_v1(
            seed,
            &original.job,
            &target,
            extended_terms,
            config.prices,
            ctx.daa_score,
            funded,
            b.extras.terminal_evidence_max_bytes.unwrap_or(100000),
            extended_claim.trace_retention_daa,
            extended_terms.bond_exit_after_daa,
            &exits,
            &binding,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        // Fee validation already burned the full declared carrier rent; the paid row reserves
        // the ordinary owner deposit before accused/material clocks are enlarged. Every
        // fallible identity/funding/overflow check is above these journaled writes.
        write_admitted_family_v1(
            b,
            key,
            PalwStagedCourtRowV1::Tokenizer(Box::new(row).into()),
            bundle,
            seed,
            &binding,
            &original.job,
        )?;
        if extended_terms != contract.terms {
            b.write_staged_row_v1(
                PalwStagedCourtKeyV1::Claim(seed.claim),
                Some(PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 { terms: extended_terms, ..contract.clone() })),
            )?;
            b.write_claim(seed.claim, Some(extended_claim))?;
            if let Some(mut dispute) = b.state.legacy_disputes.get(&seed.claim).cloned() {
                dispute.hard_deadline_daa = dispute.hard_deadline_daa.max(hard);
                b.write_legacy_dispute(seed.claim, Some(dispute))?;
            }
            b.da_rekey_v1(seed.claim, until, ctx.daa_score)?;
            if let Some(mut liability) = b.state.panel_liabilities.get(&seed.claim).cloned()
                && liability.voided_daa.is_none()
                && liability.void_reason.is_none()
                && liability.expiry_daa < until
            {
                liability.expiry_daa = until;
                b.write_panel_liability(seed.claim, Some(liability))?;
            }
            // The initial prospective holds keep these locks economically attributed even if
            // an older ordinary lock expiry has passed. Extend the actual saved locked rows.
            for (lock_key, mut lock) in b.claim_locks_v1(&seed.claim) {
                if lock.expiry_daa < until {
                    lock.expiry_daa = until;
                    b.write_slashable_lock(lock_key, Some(lock))?;
                }
            }
        }
        return Ok(());
    }
    if let Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FlatConstraint { source_id, answerer }) =
        &seed.auxiliary
    {
        let frame =
            bundle.as_ref().ok_or_else(|| stage_refused(seed.claim, "flat semantic seed requires original prepaid source frame"))?;
        let source = frame
            .flat
            .as_ref()
            .ok_or_else(|| stage_refused(seed.claim, "flat semantic seed lacks authenticated original EOF source"))?;
        if source.source_id != *source_id
            || frame
                .flat_context
                .as_ref()
                .is_none_or(|context| context.binding.as_ref() != &binding || context.job.as_ref() != &original.job)
        {
            return Err(stage_refused(seed.claim, "flat seed substitutes accepted original source, binding or job"));
        }
        let pin = source.authenticated_pin_v1(answerer).map_err(|e| stage_refused(seed.claim, e))?;
        let funded = contract
            .terms
            .plan
            .rent_sompi
            .checked_add(contract.terms.plan.deposit_sompi)
            .ok_or(PalwStateV2Error::Overflow("flat semantic funding"))?;
        let exits = vec![contract.terms.bond_exit_after_daa; contract.panel.len()];
        let row = crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit_with_authenticated_flat_pin_v1(
            seed,
            &original,
            target.execution_root,
            contract.terms,
            ctx.daa_score,
            funded,
            90_000,
            b.state.claim(&seed.claim).ok_or(PalwStateV2Error::MissingClaim(seed.claim))?.trace_retention_daa,
            contract.terms.bond_exit_after_daa,
            &exits,
            &binding,
            &pin,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        write_admitted_family_v1(
            b,
            key,
            PalwStagedCourtRowV1::Constraint(Box::new(row).into()),
            bundle,
            seed,
            &binding,
            &original.job,
        )?;
        return Ok(());
    }
    if let Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FloatConstraint { source_id, answerer }) =
        &seed.auxiliary
    {
        if !b.extras.staged_float_constraint_active {
            return Err(PalwStateV2Error::ObjectiveOffenceDormant);
        }
        let frame =
            bundle.as_ref().ok_or_else(|| stage_refused(seed.claim, "Float32 semantic seed requires original prepaid source frame"))?;
        let source = frame
            .float
            .as_ref()
            .ok_or_else(|| stage_refused(seed.claim, "Float32 semantic seed lacks an authenticated original source"))?;
        if source.source_id != *source_id
            || frame
                .float_context
                .as_ref()
                .is_none_or(|context| context.binding.as_ref() != &binding || context.job.as_ref() != &original.job)
        {
            return Err(stage_refused(seed.claim, "Float32 seed substitutes accepted original source, binding or job"));
        }
        let pin = source.authenticated_pin_v1(answerer).map_err(|e| stage_refused(seed.claim, e))?;
        let funded = contract
            .terms
            .plan
            .rent_sompi
            .checked_add(contract.terms.plan.deposit_sompi)
            .ok_or(PalwStateV2Error::Overflow("Float32 semantic funding"))?;
        let exits = vec![contract.terms.bond_exit_after_daa; contract.panel.len()];
        let row = crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit_with_authenticated_float_pin_v1(
            seed,
            &original,
            target.execution_root,
            contract.terms,
            ctx.daa_score,
            funded,
            90_000,
            b.state.claim(&seed.claim).ok_or(PalwStateV2Error::MissingClaim(seed.claim))?.trace_retention_daa,
            contract.terms.bond_exit_after_daa,
            &exits,
            &binding,
            &pin,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        write_admitted_family_v1(
            b,
            key,
            PalwStagedCourtRowV1::Constraint(Box::new(row).into()),
            bundle,
            seed,
            &binding,
            &original.job,
        )?;
        return Ok(());
    }
    // A root-authenticated profile may narrow a global fallback plan after seed admission.
    // Claim-wide clocks/holds remain the immutable conservative envelope stamped at acceptance.
    let mut terms = contract.terms;
    let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    let actual =
        crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1::derive(4095, binding.shape_profile.vocab_size, config.prices)
            .map_err(|e| stage_refused(seed.claim, e))?;
    if bundle.is_none() {
        terms.plan = actual;
    } else {
        frozen_or_extended_terms_v1(&terms, actual, ctx.daa_score, true).map_err(|e| stage_refused(seed.claim, e))?;
    }
    if terms.plan.maximum_moves > contract.terms.plan.maximum_moves || terms.plan.inclusion_daa > contract.terms.plan.inclusion_daa {
        return Err(stage_refused(seed.claim, "authenticated seed exceeds immutable claim resource envelope"));
    }
    if bundle.is_none() {
        b.check_accuser_room(seed.reserver, u128::from(terms.plan.deposit_sompi), "staged public court", ctx.daa_score)?;
    }
    // Rent is burned by this object's normative minimum carrier fee; only the deposit occupies
    // bond capital. The acceptance walk validates that fee before invoking this fold.
    let funded = contract
        .terms
        .plan
        .rent_sompi
        .checked_add(contract.terms.plan.deposit_sompi)
        .ok_or(PalwStateV2Error::Overflow("staged funding"))?;
    let exits = vec![contract.terms.bond_exit_after_daa; contract.panel.len()];
    let row = crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit_with_resolved_binding_v1(
        seed,
        &original.job,
        target.execution_root,
        terms,
        ctx.daa_score,
        funded,
        b.extras.terminal_evidence_max_bytes.unwrap_or(100_000),
        b.state.class_step_ladder_v1(
            &target.class_id,
            b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
        ),
        b.state.claim(&seed.claim).ok_or(PalwStateV2Error::MissingClaim(seed.claim))?.trace_retention_daa,
        contract.terms.bond_exit_after_daa,
        &exits,
        &binding,
    )
    .map_err(|e| stage_refused(seed.claim, e))?;
    write_admitted_family_v1(b, key, PalwStagedCourtRowV1::Constraint(Box::new(row).into()), bundle, seed, &binding, &original.job)?;
    Ok(())
}
fn frozen_or_extended_terms_v1(
    terms: &crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
    actual: crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1,
    now: u64,
    frozen: bool,
) -> Result<crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, String> {
    if !frozen {
        return terms.extend_paid_work_v1(now, actual);
    }
    if terms.plan.cover(actual) != terms.plan {
        return Err("child resource plan exceeds frozen family allocation".into());
    }
    Ok(*terms)
}
fn write_admitted_family_v1(
    b: &mut TransitionBuilder<'_>,
    key: PalwStagedCourtKeyV1,
    row: PalwStagedCourtRowV1,
    bundle: Option<PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwOwnedStagedBundleV1>>,
    seed: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Result<(), PalwStateV2Error> {
    let row = if let Some(mut bundle) = bundle {
        let context = crate::palw_staged_bundle_v1::PalwStagedStageContextV1::Legacy(crate::palw_staged_bundle_v1::PalwStagedSeedContextV1 {
            seed: Box::new(seed.clone()),
            binding: Box::new(binding.clone()),
            job: Box::new(job.clone()),
        });
        bundle.opened.push(crate::palw_staged_bundle_v1::PalwStagedOpenedFamilyV1 {
            row: crate::palw_staged_bundle_v1::PalwStagedFamilyRowV1(row),
            context: Box::new(context).into(),
        });
        bundle.check_v1().map_err(|e| stage_refused(seed.claim, e))?;
        PalwStagedCourtRowV1::OwnerBundle(bundle)
    } else {
        row
    };
    b.write_staged_row_v1(key, Some(row))?;
    Ok(())
}
pub(super) fn leaf(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    leaf: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamLeafV1,
) -> Result<(), PalwStateV2Error> {
    gate(b, &leaf.claim, leaf.reserver, leaf.reserved_daa, ctx.daa_score)?;
    let key = (leaf.claim, leaf.reserver);
    let row = b
        .state
        .staged_courts
        .get_mut(&PalwStagedCourtKeyV1::Owner(key.0, key.1))
        .ok_or_else(|| stage_refused(key.0, "owned staged seed disappeared"))?;
    let old = row.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
    let Some(PalwStagedCourtRowV1::Constraint(child)) = row.active_row_mut_v1() else {
        return Err(stage_refused(key.0, "staged leaf lacks active constraint seed"));
    };
    if let Err(error) = child.leaf(ctx.daa_score, leaf) {
        row.restore_checkpoint_v1(&old).map_err(|e| stage_refused(key.0, e))?;
        return Err(stage_refused(key.0, error));
    }
    let new = row.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key, old, new });
    Ok(())
}
pub(super) fn movement(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    if matches!(movement.action, Action::ArtifactInventoryOpen { .. } | Action::ArtifactInventoryMove(_)) {
        return artifact_inventory_control_v1(b, ctx, movement);
    }
    if matches!(
        movement.action,
        Action::SelectedEventDemand { .. } | Action::SelectedEventAnswer(_) | Action::SelectedEventCollect { .. }
    ) {
        return selected_control_v1(b, ctx, movement);
    }
    if matches!(
        movement.action,
        Action::FlatSourceOpen { .. } | Action::FlatSourceAnswer(_) | Action::FlatSourceNominate { .. } | Action::FlatSourceCollect
    ) {
        return flat_control_v1(b, ctx, movement);
    }
    if matches!(
        movement.action,
        Action::FloatSourceOpen { .. } | Action::FloatSourceAnswer(_) | Action::FloatSourceNominate { .. } | Action::FloatSourceCollect
    ) {
        return float_control_v1(b, ctx, movement);
    }
    if matches!(movement.action, Action::NativeSelectedDemand { .. } | Action::NativeSelectedAnswer(_)) {
        return native_selected_control_v1(b, ctx, movement);
    }
    if matches!(movement.action, Action::NativeTirPrefixAnswer(_) | Action::NativeTirPrefixCollect) {
        return tir_prefix_control_v1(b, ctx, movement);
    }
    if matches!(movement.action, Action::HistoryCustodyGrant { .. } | Action::HistoryRecovery { .. }) {
        return history_control_v1(b, ctx, movement);
    }
    if matches!(
        movement.action,
        Action::BundleOpen { .. } | Action::ActivateStage { .. } | Action::InventoryPart { .. } | Action::InventoryFinish
    ) {
        return bundle_control_v1(b, ctx, movement);
    }
    let collecting = matches!(movement.action, Action::PrefixPublicCollect { .. });
    gate_clock(b, &movement.claim, movement.reserver, movement.reserved_daa, ctx.daa_score, collecting)?;
    let key = (movement.claim, movement.reserver);
    if let Action::PrefixPublicCollect { cursor_id, demand_id } = movement.action {
        movement.check().map_err(|e| stage_refused(key.0, e))?;
        let lookup = PalwStagedCourtKeyV1::Owner(key.0, key.1);
        let savedrow = b.state.staged_courts.get(&lookup).ok_or_else(|| stage_refused(key.0, "public collection lacks owner"))?;
        let Some(PalwStagedCourtRowV1::PublicPrefix(saved)) = savedrow.active_row_v1() else {
            return Err(stage_refused(key.0, "public collection lacks literal owned prefix seed"));
        };
        let old_due = index_rows(lookup, savedrow).0;
        let old_responses = public_response_index_rows(saved);
        let old = savedrow.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
        let mut updatedrow = savedrow.clone();
        let Some(PalwStagedCourtRowV1::PublicPrefix(updated)) = updatedrow.active_row_mut_v1() else { unreachable!() };
        let bytes = borsh::object_length(movement).map_err(|e| stage_refused(key.0, e.to_string()))? as u64;
        let default = updated
            .collection_v1(ctx.daa_score, key.0, key.1, movement.reserved_daa, movement.seed_id, cursor_id, demand_id, bytes)
            .map_err(|e| stage_refused(key.0, e))?;
        let (sequence, previous) = updated.history_cursor_v1();
        let record = crate::palw_prefix_history_v1::PalwPrefixHistoryRecordV1::new(
            previous,
            sequence.checked_add(1).ok_or(PalwStateV2Error::Overflow("public prefix history sequence"))?,
            ctx.block,
            ctx.daa_score,
            updated.terms.material_until_daa,
            movement.clone(),
        )
        .map_err(|e| stage_refused(key.0, e))?;
        updated.append_history_v1(&record).map_err(|e| stage_refused(key.0, e))?;
        collect_prefix_default(b, ctx, &default)?;
        let new_due = index_rows(lookup, &updatedrow).0;
        let new = updatedrow.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
        b.state.staged_courts.insert(lookup, updatedrow);
        b.state.staged_due.remove(&(old_due, lookup));
        b.state.staged_due.insert((new_due, lookup));
        update_response_indices(&mut b.state, old_responses, Vec::new());
        b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key, old, new });
        return Ok(());
    }
    let independently_closed =
        b.state.claim(&key.0).is_some_and(|claim| claim.phase.is_terminal() && !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }));
    let row = b
        .state
        .staged_courts
        .get_mut(&PalwStagedCourtKeyV1::Owner(key.0, key.1))
        .ok_or_else(|| stage_refused(key.0, "staged move lacks immutable owned seed"))?;
    let lookup = PalwStagedCourtKeyV1::Owner(key.0, key.1);
    let old_due = index_rows(lookup, row).0;
    let old_responses = response_index_rows(row);
    let old = row.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
    let history = if let Some(PalwStagedCourtRowV1::PublicPrefix(c)) = row.active_row_v1() {
        let (sequence, previous) = c.history_cursor_v1();
        Some(
            crate::palw_prefix_history_v1::PalwPrefixHistoryRecordV1::new(
                previous,
                sequence.checked_add(1).ok_or(PalwStateV2Error::Overflow("public prefix history sequence"))?,
                ctx.block,
                ctx.daa_score,
                c.terms.material_until_daa,
                movement.clone(),
            )
            .map_err(|e| stage_refused(key.0, e))?,
        )
    } else {
        None
    };
    let result = match row.active_row_mut_v1() {
        Some(PalwStagedCourtRowV1::Constraint(c)) => c.movement(ctx.daa_score, movement),
        Some(PalwStagedCourtRowV1::Tokenizer(c)) => c.movement(ctx.daa_score, movement),
        Some(PalwStagedCourtRowV1::Prefix(c)) => {
            if movement.phase != crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt {
                return Err(stage_refused(movement.claim, "prefix movement changes typed phase"));
            }
            let carrier_len = borsh::to_vec(movement).map_err(|e| stage_refused(movement.claim, e.to_string()))?.len() as u64;
            movement.check().map_err(|e| stage_refused(movement.claim, e))?;
            match &movement.action {
                crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::Prefix(action) => c.movement(
                    ctx.daa_score,
                    movement.claim,
                    movement.reserver,
                    movement.reserved_daa,
                    movement.seed_id,
                    action,
                    carrier_len,
                ),
                crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::PrefixBatch(actions) => c.movement_batch_v1(
                    ctx.daa_score,
                    movement.claim,
                    movement.reserver,
                    movement.reserved_daa,
                    movement.seed_id,
                    actions,
                    carrier_len,
                ),
                _ => Err("prefix movement changes owned court action".into()),
            }
        }
        Some(PalwStagedCourtRowV1::PublicPrefix(c)) => {
            use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
            movement.check().map_err(|e| stage_refused(movement.claim, e))?;
            if movement.phase != crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt {
                return Err(stage_refused(movement.claim, "public prefix changes typed phase"));
            }
            let bytes = borsh::object_length(movement).map_err(|e| stage_refused(movement.claim, e.to_string()))? as u64;
            match &movement.action {
                Action::PrefixPublicDemand(demand) => {
                    let id = Hash64::from_bytes(
                        movement
                            .signing_message()
                            .try_into()
                            .map_err(|_| stage_refused(movement.claim, "public demand digest length"))?,
                    );
                    c.demand_v1(ctx.daa_score, key.0, key.1, movement.reserved_daa, movement.seed_id, id, demand, bytes)
                }
                Action::PrefixPublicAnswer(answer) => {
                    c.answer_v1(ctx.daa_score, key.0, key.1, movement.reserved_daa, movement.seed_id, answer, bytes)
                }
                Action::Prefix(action) => c.owner_batch_v1(
                    ctx.daa_score,
                    key.0,
                    key.1,
                    movement.reserved_daa,
                    movement.seed_id,
                    std::slice::from_ref(action),
                    bytes,
                ),
                Action::PrefixBatch(actions) => {
                    c.owner_batch_v1(ctx.daa_score, key.0, key.1, movement.reserved_daa, movement.seed_id, actions, bytes)
                }
                _ => Err("public prefix changes owned action".into()),
            }
        }
        _ => Err("claim contract alone cannot accept an owned move".into()),
    };
    let result = result.and_then(|_| {
        if let (Some(PalwStagedCourtRowV1::PublicPrefix(c)), Some(record)) = (row.active_row_mut_v1(), &history) {
            if independently_closed {
                c.mark_claim_closed_v1()?;
            }
            c.append_history_v1(record)?;
        }
        Ok(())
    });
    if let Err(error) = result {
        row.restore_checkpoint_v1(&old).map_err(|e| stage_refused(key.0, e))?;
        return Err(stage_refused(key.0, error));
    }
    let new = row.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
    let new_due = index_rows(lookup, row).0;
    let new_responses = response_index_rows(row);
    b.state.staged_due.remove(&(old_due, lookup));
    b.state.staged_due.insert((new_due, lookup));
    update_response_indices(&mut b.state, old_responses, new_responses);
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key, old, new });
    Ok(())
}
fn history_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    use crate::palw_history_custody_v1::*;
    if b.extras.staged_public_court.is_none() {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let claim_key = PalwStagedCourtKeyV1::Claim(movement.claim);
    let Some(PalwStagedCourtRowV1::ClaimBundle(mut contract)) = b.state.staged_courts.get(&claim_key).cloned() else {
        return Err(stage_refused(movement.claim, "history custody requires original signed accepted bundle contract"));
    };
    if let Action::HistoryCustodyGrant { grant, receipt } = &movement.action {
        if movement.reserver != grant.seat
            || movement.seed_id != contract.history_custody.commitment_v1().map_err(|e| stage_refused(movement.claim, e))?
            || movement.reserved_daa != contract.claim.terms.accepted_daa
            || ctx.daa_score >= contract.claim.terms.material_until_daa
            || !contract
                .claim
                .prefix_custody
                .as_ref()
                .is_some_and(|custody| custody.matches_credited_receipt_v1(movement.claim, receipt))
        {
            return Err(stage_refused(
                movement.claim,
                "history grant does not name the exact already-native-credited Valid signature",
            ));
        }
        let seat = b.state.bond(&grant.seat).ok_or(PalwStateV2Error::MissingBond(grant.seat))?;
        if !contract.history_custody.qualify_v1(
            movement.claim,
            receipt,
            grant,
            ctx.daa_score,
            &seat.pubkey,
            crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
        ) {
            return Err(stage_refused(movement.claim, "history grant is unsigned, duplicated or outside exact original custody"));
        }
        contract.history_custody.check_v1(&contract.claim.panel).map_err(|e| stage_refused(movement.claim, e))?;
        b.write_staged_row_v1(claim_key, Some(PalwStagedCourtRowV1::ClaimBundle(contract)))?;
        return Ok(());
    }
    let Action::HistoryRecovery { stage_seed, action, .. } = &movement.action else {
        return Err(stage_refused(movement.claim, "not a history custody action"));
    };
    let owner_key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&owner_key).cloned() else {
        return Err(stage_refused(movement.claim, "history recovery has no prepaid literal owner frame"));
    };
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != bundle.plan.id_v1(bundle.claim)
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.collect_until_daa
    {
        return Err(stage_refused(movement.claim, "history recovery changes original paid frame or horizon"));
    }
    let old_index = history_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone()));
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    if !bundle.history_recovery.contains_key(stage_seed) {
        if !matches!(action, PalwHistoryRecoveryActionV1::Demand) {
            return Err(stage_refused(movement.claim, "history answer/collection lacks accepted original demand"));
        }
        let Some(PalwStagedCourtRowV1::PublicPrefix(prefix)) = bundle.seed_row_v1(*stage_seed) else {
            return Err(stage_refused(movement.claim, "history recovery substitutes an unopened or non-history stage"));
        };
        let (sequence, head) = prefix.history_cursor_v1();
        if sequence == 0 || sequence > contract.history_custody.maximum_records {
            return Err(stage_refused(movement.claim, "history cursor exceeds original signed record capacity"));
        }
        let request = crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1 {
            claim: bundle.claim,
            owner: bundle.owner,
            reserved_daa: bundle.reserved_daa,
            seed_id: *stage_seed,
            head,
            sequence,
            material_until_daa: prefix.terms.material_until_daa,
        };
        let recovery = PalwHistoryRecoveryV1::new_v1(
            request,
            ctx.daa_score,
            bundle.plan.terms.stream_until_daa,
            bundle.plan.terms.collect_until_daa,
            contract.config.acquisition_daa,
            contract.history_custody.maximum_custodians,
            contract.history_custody.current_signed_custodians_v1(),
        )
        .map_err(|e| stage_refused(movement.claim, e))?;
        bundle.history_recovery.insert(*stage_seed, Box::new(recovery).into());
    }
    let recovery = bundle.history_recovery.get_mut(stage_seed).expect("inserted bounded literal seed");
    movement.check_history_scope_v1(recovery).map_err(|e| stage_refused(movement.claim, e))?;
    let effect = recovery.apply_v1(action, ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?;
    if let PalwHistoryRecoveryEffectV1::Default(default) = effect {
        let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        b.staged_history_default_charge_v1(ctx, &claim, &contract.history_custody, &default, movement.reserver)?;
    }
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let row = PalwStagedCourtRowV1::OwnerBundle(bundle);
    let new = row.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let new_index = history_response_index_rows_v1(&row);
    b.state.staged_courts.insert(owner_key, row);
    update_history_response_indices_v1(&mut b.state, old_index, new_index);
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    Ok(())
}
fn artifact_inventory_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::InventoryOpen(movement.claim, movement.reserver);
    match &movement.action {
        Action::ArtifactInventoryOpen { class_id, offer } => {
            let registered = b
                .state
                .class_semantics
                .get(class_id)
                .ok_or_else(|| stage_refused(movement.claim, "inventory has no immutable registered artifact"))?;
            if registered.tokenizer_id != movement.claim
                || registered.raw_tokenizer_bytes.is_none()
                || offer.artifact_hash != movement.claim
                || offer.opener != movement.reserver
                || offer.id_v1() != movement.seed_id
                || movement.reserved_daa != 0
                || b.state.staged_courts.contains_key(&key)
            {
                return Err(stage_refused(movement.claim, "inventory substitutes registered raw hash, opener or immutable offer"));
            }
            let allowance = config.maximum_owned_rows_per_reserver as usize;
            if b.state.staged_owner_allowance_used_v1(movement.reserver, allowance) >= allowance {
                return Err(stage_refused(movement.claim, "inventory opener's own paid allowance is occupied"));
            }
            let row = crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryV1::new_v1(
                offer.clone(),
                ctx.daa_score,
                config,
            )
            .map_err(|e| stage_refused(movement.claim, e))?;
            b.check_accuser_room(
                movement.reserver,
                u128::from(row.plan.deposit_sompi),
                "public tokenizer EOF inventory",
                ctx.daa_score,
            )?;
            b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::ArtifactInventoryOpen(Box::new(row).into())))?;
        }
        Action::ArtifactInventoryMove(action) => {
            let Some(PalwStagedCourtRowV1::ArtifactInventoryOpen(mut row)) = b.state.staged_courts.get(&key).cloned() else {
                return Err(stage_refused(movement.claim, "inventory move has no literal paid attempt"));
            };
            if movement.seed_id != row.offer.id_v1() || movement.reserved_daa != row.opened_daa {
                return Err(stage_refused(movement.claim, "inventory move changes original offer or accepted opening clock"));
            }
            let certificate = row.apply_v1(action, ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?;
            if let Some(certificate) = certificate {
                if let Some(existing) = b.state.staged_tokenizer_inventory_v1(movement.claim) {
                    if existing.actual_bytes != certificate.actual_bytes {
                        return Err(stage_refused(movement.claim, "accepted exact raw hash has inconsistent EOF size"));
                    }
                } else {
                    b.write_staged_row_v1(
                        PalwStagedCourtKeyV1::Inventory(movement.claim),
                        Some(PalwStagedCourtRowV1::ArtifactInventory(certificate)),
                    )?;
                }
                b.write_staged_row_v1(key, None)?;
            } else {
                b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::ArtifactInventoryOpen(row)))?;
            }
        }
        _ => return Err(stage_refused(movement.claim, "not an artifact inventory action")),
    }
    Ok(())
}
fn selected_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(movement.claim, "selected source requires already prepaid owner frame"));
    };
    // A TIR claim's selected source collects through its native context.
    if bundle.selected_native.is_some() {
        return native_selected_control_v1(b, ctx, movement);
    }
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != bundle.plan.id_v1(bundle.claim)
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.collect_until_daa
    {
        return Err(stage_refused(movement.claim, "selected source changes original paid frame identity or clock"));
    }
    let initial = bundle.selected.is_none();
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let old_index = selected_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone()));
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &movement.claim)
        .ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
    let profile = b
        .state
        .fp_work_profiles
        .get(&target.class_id)
        .ok_or_else(|| stage_refused(movement.claim, "selected source class profile absent"))?;
    let supplied = match &movement.action {
        Action::SelectedEventDemand { binding, .. } => Some(binding),
        Action::SelectedEventAnswer(answer) => Some(&answer.binding),
        _ => None,
    };
    let supplied = supplied
        .map(|supplied| supplied.resolve_v1(target.class_id, profile.as_ref()))
        .transpose()
        .map_err(|e| stage_refused(movement.claim, e))?;
    if initial {
        if !matches!(movement.action, Action::SelectedEventDemand { .. }) {
            return Err(stage_refused(movement.claim, "selected answer/collect lacks accepted literal demand"));
        }
        let next = bundle.active.map_or(0, |n| n as usize + 1);
        if bundle.opened.len() != next
            || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
            || bundle.inventory.as_ref().is_some_and(|i| !i.sealed || i.length_fault_v1())
        {
            return Err(stage_refused(movement.claim, "selected acquisition cannot bypass honest predecessor or initial EOF intake"));
        }
        let progress =
            crate::palw_staged_court_state_v1::palw_staged_selected_events_v1(&b.state, (movement.claim, movement.reserver))
                .map_err(|e| stage_refused(movement.claim, e))?
                .ok_or_else(|| stage_refused(movement.claim, "selected source is not the next required native-priced family"))?;
        let binding =
            supplied.as_ref().ok_or_else(|| stage_refused(movement.claim, "selected source lacks signed original binding"))?;
        let raw = b
            .state
            .fp_public_jobs
            .get(&movement.claim)
            .ok_or_else(|| stage_refused(movement.claim, "selected source accepted original job absent"))?;
        let original: crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 =
            borsh::from_slice(raw).map_err(|e| stage_refused(movement.claim, e.to_string()))?;
        crate::palw_offence_attribution_v1::palw_constrained_job_target_v1(&target, binding, &original.job)
            .map_err(|e| stage_refused(movement.claim, e.to_string()))?;
        if !original.job.is_constraint()
            || crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(
                &target,
                binding,
                b.identity_rules_v1(ctx.daa_score),
                false,
            )
            .map_err(|e| stage_refused(movement.claim, e.to_string()))?
            .is_some()
        {
            return Err(stage_refused(movement.claim, "selected source substitutes original constrained context"));
        }
        bundle.selected = Some(progress.session);
        bundle.selected_context = Some(
            Box::new(crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1 {
                binding: Box::new(binding.clone()),
                job: Box::new(original.job),
            })
            .into(),
        );
    }
    // A shared (COW) handle: cloning it is a reference count, and it frees `bundle` for the
    // session's mutable borrow below.
    let context = bundle
        .selected_context
        .clone()
        .ok_or_else(|| stage_refused(movement.claim, "selected source authenticated context disappeared"))?;
    if supplied.as_ref().is_some_and(|binding| binding != context.binding.as_ref()) {
        return Err(stage_refused(movement.claim, "selected movement substitutes frozen signed binding"));
    }
    let session = bundle.selected.as_mut().ok_or_else(|| stage_refused(movement.claim, "selected source session disappeared"))?;
    let unpaid = match &movement.action {
        Action::SelectedEventDemand { demand, .. } => {
            session.demand_v1(ctx.daa_score, &context.binding, demand).map_err(|e| stage_refused(movement.claim, e))?;
            None
        }
        Action::SelectedEventAnswer(answer) => {
            let ladder = b.state.class_step_ladder_v1(
                &target.class_id,
                b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
            );
            session.answer_v1(ctx.daa_score, &context.binding, answer, ladder).map_err(|e| stage_refused(movement.claim, e))?;
            None
        }
        Action::SelectedEventCollect { coordinate, demand_id } => {
            Some(session.collect_v1(ctx.daa_score, *coordinate, *demand_id).map_err(|e| stage_refused(movement.claim, e))?)
        }
        _ => return Err(stage_refused(movement.claim, "not a selected source action")),
    };
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    if let Some(unpaid) = unpaid.filter(|set| !set.is_empty()) {
        let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        b.staged_selected_default_charge_v1(ctx, movement.claim, &claim, target.executor_bond, &unpaid, movement.reserver)?;
    }
    let row = PalwStagedCourtRowV1::OwnerBundle(bundle);
    if initial {
        b.write_staged_row_v1(key, Some(row))?;
    } else {
        let new = row.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
        let new_index = selected_response_index_rows_v1(&row);
        b.state.staged_courts.insert(key, row);
        update_selected_response_indices_v1(&mut b.state, old_index, new_index);
        b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    }
    Ok(())
}
/// **The TIR selected source** (behind `palw_staged_tir_route_v1`): the legacy selected source's
/// session, clocks and literal custody, with no binding on any carrier. The decode view is derived
/// here, from the claim's admitted TIR public header and its registered class record, at every move.
fn native_selected_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    // Defence in depth: below its fence the form never reaches the fold (`crate::palw_staged_route_forms_v1`).
    if b.extras.staged_public_court.is_none() || !b.extras.staged_tir_route_active {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(movement.claim, "native selected source requires already prepaid owner frame"));
    };
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != bundle.plan.id_v1(bundle.claim)
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.collect_until_daa
    {
        return Err(stage_refused(movement.claim, "native selected source changes original paid frame identity or clock"));
    }
    let initial = bundle.selected.is_none();
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let old_index = selected_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone()));
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &movement.claim)
        .ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
    let ladder = b.state.class_step_ladder_v1(
        &target.class_id,
        b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
    );
    let (view, header) = crate::palw_staged_court_state_v1::palw_staged_tir_view_v1(&b.state, movement.claim, ladder)
        .map_err(|e| stage_refused(movement.claim, e))?
        .ok_or_else(|| stage_refused(movement.claim, "native selected source serves a TIR claim only"))?;
    if !view.is_tiled_v1() {
        return Err(stage_refused(movement.claim, "native selected source needs the claim's tiled TIR logits"));
    }
    if initial {
        if !matches!(movement.action, Action::NativeSelectedDemand { .. }) {
            return Err(stage_refused(movement.claim, "native selected answer/collect lacks accepted literal demand"));
        }
        let next = bundle.active.map_or(0, |n| n as usize + 1);
        if bundle.opened.len() != next
            || bundle.flat.is_some()
            || bundle.float.is_some()
            || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
            || bundle.inventory.as_ref().is_some_and(|i| !i.sealed || i.length_fault_v1())
        {
            return Err(stage_refused(movement.claim, "native selected acquisition cannot bypass honest predecessor or initial EOF intake"));
        }
        let progress =
            crate::palw_staged_court_state_v1::palw_staged_selected_events_v1(&b.state, (movement.claim, movement.reserver))
                .map_err(|e| stage_refused(movement.claim, e))?
                .ok_or_else(|| stage_refused(movement.claim, "native selected source is not the next required native-priced family"))?;
        let original = b
            .state
            .staged_original_job_v1(movement.claim)
            .map_err(|e| stage_refused(movement.claim, e))?
            .ok_or_else(|| stage_refused(movement.claim, "native selected source accepted original job absent"))?;
        crate::palw_decode_view_v1::palw_constrained_job_target_view_v1(&target, &view, &original.job)
            .map_err(|e| stage_refused(movement.claim, e))?;
        if !original.job.is_constraint() {
            return Err(stage_refused(movement.claim, "native selected source serves a constrained job only"));
        }
        bundle.selected = Some(progress.session);
        bundle.selected_native = Some(
            Box::new(crate::palw_staged_bundle_v1::PalwNativeSourceContextV1 { header, job: Box::new(original.job) }).into(),
        );
    } else if bundle.selected_native.as_ref().is_none_or(|context| *context.header != *header) {
        return Err(stage_refused(movement.claim, "native selected movement substitutes the claim's admitted header"));
    }
    let session =
        bundle.selected.as_mut().ok_or_else(|| stage_refused(movement.claim, "native selected source session disappeared"))?;
    let unpaid = match &movement.action {
        Action::NativeSelectedDemand { demand } => {
            session.demand_view_v1(ctx.daa_score, &view, demand).map_err(|e| stage_refused(movement.claim, e))?;
            None
        }
        Action::NativeSelectedAnswer(answer) => {
            answer.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
            session
                .answer_event_view_v1(
                    ctx.daa_score,
                    &view,
                    answer.answerer,
                    answer.demand_id,
                    answer.coordinate,
                    &answer.event,
                    ladder,
                )
                .map_err(|e| stage_refused(movement.claim, e))?;
            None
        }
        Action::SelectedEventCollect { coordinate, demand_id } => {
            Some(session.collect_v1(ctx.daa_score, *coordinate, *demand_id).map_err(|e| stage_refused(movement.claim, e))?)
        }
        _ => return Err(stage_refused(movement.claim, "not a native selected source action")),
    };
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    if let Some(unpaid) = unpaid.filter(|set| !set.is_empty()) {
        let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        b.staged_selected_default_charge_v1(ctx, movement.claim, &claim, target.executor_bond, &unpaid, movement.reserver)?;
    }
    let row = PalwStagedCourtRowV1::OwnerBundle(bundle);
    if initial {
        b.write_staged_row_v1(key, Some(row))?;
    } else {
        let new = row.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
        let new_index = selected_response_index_rows_v1(&row);
        b.state.staged_courts.insert(key, row);
        update_selected_response_indices_v1(&mut b.state, old_index, new_index);
        b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    }
    Ok(())
}

/// **A TIR prefix-state stage's moves** (behind `palw_staged_tir_route_v1`): a custodian's (or
/// the owner's) next authenticated boundary leaf, or the owner's collection after the custody
/// clock. The class's step space and job context are rebuilt natively at every move: the
/// admitted header, the registered program and the published layout.
fn tir_prefix_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    if b.extras.staged_public_court.is_none() || !b.extras.staged_tir_route_active {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(movement.claim, "TIR prefix move has no prepaid owner frame"));
    };
    let native = bundle
        .active_native_seed_v1()
        .cloned()
        .ok_or_else(|| stage_refused(movement.claim, "TIR prefix move has no active native stage"))?;
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != native.seed.seed_id_v1()
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.collect_until_daa
    {
        return Err(stage_refused(movement.claim, "TIR prefix move changes the paid frame, active seed or clock"));
    }
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &movement.claim)
        .ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
    let ladder = b.state.class_step_ladder_v1(
        &target.class_id,
        b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
    );
    let record = b
        .state
        .tir_classes
        .get(&target.class_id)
        .ok_or_else(|| stage_refused(movement.claim, "TIR prefix move has no registered class record"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, target.class_id)
        .ok_or_else(|| stage_refused(movement.claim, "TIR prefix move lost the class's published layout"))?;
    let context = crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(record, layout, &native.header, &native.job, ladder)
        .map_err(|e| stage_refused(movement.claim, e))?;
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let old_index = flat_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone()));
    let Some(PalwStagedCourtRowV1::TirPrefix(court)) = bundle.active_mut_v1() else {
        return Err(stage_refused(movement.claim, "TIR prefix move has no active TIR prefix court"));
    };
    let (unpaid, producer) = match &movement.action {
        Action::NativeTirPrefixAnswer(answer) => {
            court
                .answer_v1(ctx.daa_score, &context.space, &context.job_context, answer)
                .map_err(|e| stage_refused(movement.claim, e))?;
            (Vec::new(), court.producer)
        }
        Action::NativeTirPrefixCollect => {
            if movement.reserver != court.reserver {
                return Err(stage_refused(movement.claim, "only the stage owner collects its custody clock"));
            }
            (court.collect_v1(ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?, court.producer)
        }
        _ => return Err(stage_refused(movement.claim, "not a TIR prefix move")),
    };
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    if !unpaid.is_empty() {
        let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        let unpaid: BTreeSet<_> = unpaid.into_iter().collect();
        b.staged_selected_default_charge_v1(ctx, movement.claim, &claim, producer, &unpaid, movement.reserver)?;
    }
    let row = PalwStagedCourtRowV1::OwnerBundle(bundle);
    let new = row.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let new_index = flat_response_index_rows_v1(&row);
    b.state.staged_courts.insert(key, row);
    update_flat_response_indices_v1(&mut b.state, old_index, new_index);
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    Ok(())
}

/// **A registered TIR class's layout, published** (tag 169, behind `palw_staged_tir_route_v1`):
/// authenticated by the class record's `layout_digest` and program and checked to build a step
/// space. Publishing it again is a no-op; nothing about it is a claim or a court.
pub(super) fn publish_tir_layout(
    b: &mut TransitionBuilder<'_>,
    _ctx: &PalwBlockContextV2,
    class_id: Hash64,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
) -> Result<(), PalwStateV2Error> {
    if b.extras.staged_public_court.is_none() || !b.extras.staged_tir_route_active {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let key = PalwStagedCourtKeyV1::TirLayout(class_id);
    if b.state.staged_courts.contains_key(&key) {
        return Ok(());
    }
    let record = b.state.tir_classes.get(&class_id).ok_or_else(|| stage_refused(class_id, "layout names no registered TIR class"))?;
    let class = crate::palw_tir_class_v1::PalwTirClassV1 {
        version: crate::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
        program: record.program.as_ref().clone(),
        layout: layout.clone(),
        tokenizer_id: record.tokenizer_id,
    };
    if class.layout_digest() != record.layout_digest || class.graph_ir_root() != record.graph_ir_root {
        return Err(stage_refused(class_id, "published layout does not authenticate the registered class"));
    }
    crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).map_err(|e| stage_refused(class_id, e.to_string()))?;
    b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::TirLayout(Box::new(layout.clone()))))?;
    Ok(())
}

/// **A TIR claim's stage seed** (tag 168, behind `palw_staged_tir_route_v1`): the Constraint or
/// tokenizer family of a prepaid required-family bundle, admitted over the decode view derived
/// from the claim's admitted TIR public header and registered class record. The seed signs that
/// header's identity; nothing on the carrier is a binding.
pub(super) fn open_native(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    seed: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamNativeSeedV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwNativeSeedFamilyV1 as NativeFamily;
    use crate::palw_staged_bundle_v1::PalwStagedFamilyV1 as Family;
    if b.extras.staged_public_court.is_none() || !b.extras.staged_tir_route_active {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    seed.check(90_000).map_err(|e| stage_refused(seed.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(seed.claim, seed.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(seed.claim, "a native seed requires its prepaid required-family bundle"));
    };
    bundle.check_v1().map_err(|e| stage_refused(seed.claim, e))?;
    if seed.reserved_daa != bundle.reserved_daa
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.stream_until_daa
        || ctx.daa_score >= bundle.plan.terms.material_until_daa
        || bundle.opened.len() >= bundle.plan.stages.len()
        || bundle.opened.len() != bundle.active.map_or(0, |n| n as usize + 1)
        || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
        || bundle.inventory.as_ref().is_some_and(|intake| !intake.sealed || intake.length_fault_v1())
    {
        return Err(stage_refused(seed.claim, "native seed requires an honest completed predecessor, intake and the paid frame clock"));
    }
    let family = match &seed.family {
        NativeFamily::Constraint { .. } => Family::Constraint,
        NativeFamily::Tokenizer { .. } => Family::Tokenizer,
        NativeFamily::PrefixState { .. } => Family::PublicPrefix,
    };
    let next = bundle.plan.stages[bundle.opened.len()].clone();
    if next.family != family {
        return Err(stage_refused(seed.claim, "native seed substitutes required family"));
    }
    let mut contract = b
        .state
        .staged_claim_contract_v1(&seed.claim)
        .ok_or_else(|| stage_refused(seed.claim, "native seed lacks original claim contract"))?
        .clone();
    contract.terms = next.terms;
    if ctx.daa_score >= contract.terms.open_until_daa || contract.panel.is_empty() {
        return Err(stage_refused(seed.claim, "native seed is after its opening deadline or has no bound Panel"));
    }
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &seed.claim)
        .ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
    let ladder = b.state.class_step_ladder_v1(
        &target.class_id,
        b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
    );
    let (view, header) = crate::palw_staged_court_state_v1::palw_staged_tir_view_v1(&b.state, seed.claim, ladder)
        .map_err(|e| stage_refused(seed.claim, e))?
        .ok_or_else(|| stage_refused(seed.claim, "a native seed serves a TIR claim only"))?;
    if seed.view_identity != crate::palw_constraint_stream_carriage_v1::palw_native_view_identity_v1(&header)
        || seed.vocab_size != view.vocab_size
    {
        return Err(stage_refused(seed.claim, "native seed signs another header or vocabulary than the claim's admitted one"));
    }
    let original = b
        .state
        .staged_original_job_v1(seed.claim)
        .map_err(|e| stage_refused(seed.claim, e))?
        .ok_or_else(|| stage_refused(seed.claim, "native seed requires the accepted original public FP job"))?;
    if seed.job.as_deref().is_some_and(|j| *j != original.job) {
        return Err(stage_refused(seed.claim, "native seed changes accepted original FP job"));
    }
    crate::palw_decode_view_v1::palw_constrained_job_target_view_v1(&target, &view, &original.job)
        .map_err(|e| stage_refused(seed.claim, e))?;
    let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    let funded = contract
        .terms
        .plan
        .rent_sompi
        .checked_add(contract.terms.plan.deposit_sompi)
        .ok_or(PalwStateV2Error::Overflow("native staged funding"))?;
    let exits = vec![contract.terms.bond_exit_after_daa; contract.panel.len()];
    let retention = b.state.claim(&seed.claim).ok_or(PalwStateV2Error::MissingClaim(seed.claim))?.trace_retention_daa;
    let row = match family {
        Family::Constraint => {
            let actual = crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1::derive(4095, view.vocab_size, config.prices)
                .map_err(|e| stage_refused(seed.claim, e))?;
            frozen_or_extended_terms_v1(&contract.terms, actual, ctx.daa_score, true).map_err(|e| stage_refused(seed.claim, e))?;
            let identity = borsh::to_vec(header.as_ref()).map_err(|e| stage_refused(seed.claim, e.to_string()))?;
            PalwStagedCourtRowV1::Constraint(
                Box::new(
                    crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit_native_view_v1(
                        seed,
                        &original.job,
                        target.execution_root,
                        contract.terms,
                        ctx.daa_score,
                        funded,
                        b.extras.terminal_evidence_max_bytes.unwrap_or(100_000),
                        ladder,
                        retention,
                        contract.terms.bond_exit_after_daa,
                        &exits,
                        &view,
                        &identity,
                    )
                    .map_err(|e| stage_refused(seed.claim, e))?,
                )
                .into(),
            )
        }
        Family::PublicPrefix => {
            let NativeFamily::PrefixState { prefix_tokens } = &seed.family else { unreachable!("mapped above") };
            let record = b
                .state
                .tir_classes
                .get(&target.class_id)
                .ok_or_else(|| stage_refused(seed.claim, "TIR prefix stage has no registered class record"))?;
            let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, target.class_id)
                .ok_or_else(|| stage_refused(seed.claim, "TIR prefix stage needs the class's published layout"))?;
            let context = crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(record, layout, &header, &original.job, ladder)
                .map_err(|e| stage_refused(seed.claim, e))?;
            let named =
                crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(&original.job).map_err(|e| stage_refused(seed.claim, e.to_string()))?;
            if named.prefix_tokens != *prefix_tokens {
                return Err(stage_refused(seed.claim, "TIR prefix seed names another boundary than the job's declared state"));
            }
            let custody = contract
                .prefix_custody
                .as_ref()
                .ok_or_else(|| stage_refused(seed.claim, "TIR prefix stage original signed material custody absent"))?
                .current_signed_v1();
            if !custody.public_qualified_v1() || contract.panel.contains(&seed.reserver) || contract.producer == seed.reserver {
                return Err(stage_refused(seed.claim, "TIR prefix stage needs qualified custody and an owner outside it"));
            }
            let custodians: BTreeSet<_> = custody.expected_panel.into_iter().chain([contract.producer]).collect();
            let actual = crate::palw_tir_prefix_owned_v1::tir_prefix_resource_plan_v1(
                context.template.total_v1(),
                context.leaf_bytes,
                context.depth,
                u32::try_from(custodians.len()).map_err(|_| stage_refused(seed.claim, "TIR prefix custody exceeds format"))?,
                config.prices,
            )
            .map_err(|e| stage_refused(seed.claim, e))?;
            frozen_or_extended_terms_v1(&contract.terms, actual, ctx.daa_score, true).map_err(|e| stage_refused(seed.claim, e))?;
            PalwStagedCourtRowV1::TirPrefix(
                Box::new(
                    crate::palw_tir_prefix_owned_v1::PalwOwnedTirPrefixCourtV1::admit_v1(
                        seed.seed_id_v1(),
                        seed.claim,
                        seed.reserver,
                        seed.reserved_daa,
                        target.execution_root,
                        contract.terms,
                        ctx.daa_score,
                        funded,
                        contract.producer,
                        custodians,
                        context.template,
                        retention,
                        contract.terms.bond_exit_after_daa,
                        &exits,
                    )
                    .map_err(|e| stage_refused(seed.claim, e))?,
                )
                .into(),
            )
        }
        _ => PalwStagedCourtRowV1::Tokenizer(
            Box::new(
                crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1::admit_native_view_v1(
                    seed,
                    &original.job,
                    &target,
                    &view,
                    contract.terms,
                    config.prices,
                    ctx.daa_score,
                    funded,
                    b.extras.terminal_evidence_max_bytes.unwrap_or(100_000),
                    retention,
                    contract.terms.bond_exit_after_daa,
                    &exits,
                )
                .map_err(|e| stage_refused(seed.claim, e))?,
            )
            .into(),
        ),
    };
    let mut bundle = bundle;
    bundle.opened.push(crate::palw_staged_bundle_v1::PalwStagedOpenedFamilyV1 {
        row: crate::palw_staged_bundle_v1::PalwStagedFamilyRowV1(row),
        context: Box::new(crate::palw_staged_bundle_v1::PalwStagedStageContextV1::Native(
            crate::palw_staged_bundle_v1::PalwStagedNativeSeedContextV1 {
                seed: Box::new(seed.clone()),
                header,
                job: Box::new(original.job),
            },
        ))
        .into(),
    });
    bundle.check_v1().map_err(|e| stage_refused(seed.claim, e))?;
    b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::OwnerBundle(bundle)))?;
    Ok(())
}

fn flat_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    if b.extras.staged_public_court.is_none() {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(movement.claim, "flat source has no original prepaid owner frame"));
    };
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != bundle.plan.id_v1(bundle.claim)
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.collect_until_daa
    {
        return Err(stage_refused(movement.claim, "flat source changes literal paid original owner, seed or clock"));
    }
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let old_index = flat_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone()));
    let initial = bundle.flat.is_none();
    if let Action::FlatSourceOpen { binding } = &movement.action {
        let next = bundle.active.map_or(0, |n| n as usize + 1);
        let stage = bundle
            .plan
            .stages
            .get(next)
            .filter(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint)
            .ok_or_else(|| stage_refused(movement.claim, "flat source is not upcoming required family"))?;
        if !initial
            || bundle.opened.len() != next
            || bundle.selected.is_some()
            || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
            || bundle.inventory.as_ref().is_some_and(|i| !i.sealed || i.length_fault_v1())
        {
            return Err(stage_refused(movement.claim, "flat source repeats intake or bypasses strict honest predecessor"));
        }
        let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &movement.claim)
            .ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        let profile = b
            .state
            .fp_work_profiles
            .get(&target.class_id)
            .ok_or_else(|| stage_refused(movement.claim, "flat source has no original native profile"))?;
        if profile.logits_scheme_id != crate::palw_step_refute::flat_logits_scheme_id_v1() {
            return Err(stage_refused(movement.claim, "flat source reinterprets an original non-flat logits scheme"));
        }
        let binding = binding.resolve_v1(target.class_id, profile).map_err(|e| stage_refused(movement.claim, e))?;
        let original = b
            .state
            .staged_original_job_v1(movement.claim)
            .map_err(|e| stage_refused(movement.claim, e))?
            .ok_or_else(|| stage_refused(movement.claim, "flat source original accepted job absent"))?;
        crate::palw_offence_attribution_v1::palw_constrained_job_target_v1(&target, &binding, &original.job)
            .map_err(|e| stage_refused(movement.claim, e.to_string()))?;
        if !original.job.is_constraint()
            || crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(
                &target,
                &binding,
                b.identity_rules_v1(ctx.daa_score),
                false,
            )
            .map_err(|e| stage_refused(movement.claim, e.to_string()))?
            .is_some()
        {
            return Err(stage_refused(movement.claim, "flat source substitutes constrained original binding"));
        }
        let contract = b
            .state
            .staged_claim_contract_v1(&movement.claim)
            .ok_or_else(|| stage_refused(movement.claim, "flat source lacks immutable custody"))?;
        if contract.producer == movement.reserver || contract.panel.contains(&movement.reserver) {
            return Err(stage_refused(movement.claim, "flat source owner must be outside entire original held Panel"));
        }
        let custody = contract
            .prefix_custody
            .as_ref()
            .ok_or_else(|| stage_refused(movement.claim, "flat source original signed material custody absent"))?
            .current_signed_v1();
        if !custody.public_qualified_v1() {
            return Err(stage_refused(movement.claim, "flat source semantic custody is unqualified"));
        }
        let custodians = custody.expected_panel.into_iter().chain([contract.producer]).collect();
        let config = b.extras.staged_public_court.expect("fence checked");
        let respond = bundle
            .plan
            .preseed_source_until_v1(crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint, config)
            .map_err(|e| stage_refused(movement.claim, e))?;
        let source = crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1::new_deferred_v1(
            bundle.claim,
            bundle.owner,
            bundle.reserved_daa,
            bundle.plan.id_v1(bundle.claim),
            contract.producer,
            custodians,
            &binding,
            respond,
            stage.terms.collect_until_daa,
            config.prices,
        )
        .map_err(|e| stage_refused(movement.claim, e))?;
        // The first pass has the same conservative work/index envelope as the
        // reserved second pass. A late owner cannot manufacture a default by
        // opening immediately before the first immutable response deadline.
        source.check_open_window_v1(ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?;
        bundle.flat = Some(Box::new(source).into());
        bundle.flat_context = Some(
            Box::new(crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1 {
                binding: Box::new(binding),
                job: Box::new(original.job),
            })
            .into(),
        );
    } else {
        let source =
            bundle.flat.as_mut().ok_or_else(|| stage_refused(movement.claim, "flat answer/control lacks accepted original source"))?;
        let unpaid = match &movement.action {
            Action::FlatSourceAnswer(answer) => {
                source.answer_v1(ctx.daa_score, answer).map_err(|e| stage_refused(movement.claim, e))?;
                None
            }
            Action::FlatSourceNominate { source_id, position, beat } => {
                source
                    .nominate_v1(ctx.daa_score, movement.reserver, *source_id, *position, *beat)
                    .map_err(|e| stage_refused(movement.claim, e))?;
                None
            }
            Action::FlatSourceCollect => Some(
                source.collect_v1(ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?.into_iter().collect::<BTreeSet<_>>(),
            ),
            _ => return Err(stage_refused(movement.claim, "not a flat source action")),
        };
        if let Some(unpaid) = unpaid.filter(|s| !s.is_empty()) {
            let producer = source.producer;
            let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
            b.staged_selected_default_charge_v1(ctx, movement.claim, &claim, producer, &unpaid, movement.reserver)?;
        }
    }
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let row = PalwStagedCourtRowV1::OwnerBundle(bundle);
    if initial {
        b.write_staged_row_v1(key, Some(row))?;
    } else {
        let new = row.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
        let new_index = flat_response_index_rows_v1(&row);
        b.state.staged_courts.insert(key, row);
        update_flat_response_indices_v1(&mut b.state, old_index, new_index);
        b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    }
    Ok(())
}

/// **The greedy Float32 source** (behind `palw_staged_float_constraint_v1`): the Flat source's
/// clocks and literal custody over the claim's ORIGINAL v2 event tree — ids first, the owner's
/// nomination over authenticated ids, then the nominated f32 row streamed into a stored row
/// continuation per isolated slot ([`crate::palw_float_constraint_source_v1`]).
fn float_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    // Defence in depth: below its fence the form is the live build's undecodable payload and
    // never reaches the fold (`crate::palw_staged_route_forms_v1`).
    if b.extras.staged_public_court.is_none() || !b.extras.staged_float_constraint_active {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(movement.claim, "Float32 source has no original prepaid owner frame"));
    };
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != bundle.plan.id_v1(bundle.claim)
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.collect_until_daa
    {
        return Err(stage_refused(movement.claim, "Float32 source changes literal paid original owner, seed or clock"));
    }
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let old_index = flat_response_index_rows_v1(&PalwStagedCourtRowV1::OwnerBundle(bundle.clone()));
    let initial = bundle.float.is_none();
    if let Action::FloatSourceOpen { binding } = &movement.action {
        let next = bundle.active.map_or(0, |n| n as usize + 1);
        let stage = bundle
            .plan
            .stages
            .get(next)
            .filter(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint)
            .ok_or_else(|| stage_refused(movement.claim, "Float32 source is not upcoming required family"))?;
        if !initial
            || bundle.opened.len() != next
            || bundle.selected.is_some()
            || bundle.flat.is_some()
            || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
            || bundle.inventory.as_ref().is_some_and(|i| !i.sealed || i.length_fault_v1())
        {
            return Err(stage_refused(movement.claim, "Float32 source repeats intake or bypasses strict honest predecessor"));
        }
        let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &movement.claim)
            .ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        let profile = b
            .state
            .fp_work_profiles
            .get(&target.class_id)
            .ok_or_else(|| stage_refused(movement.claim, "Float32 source has no original native profile"))?;
        if profile.lane != crate::palw_step::PalwStepLaneV1::Float32 {
            return Err(stage_refused(movement.claim, "Float32 source reinterprets an original integer class"));
        }
        let binding = binding.resolve_v1(target.class_id, profile).map_err(|e| stage_refused(movement.claim, e))?;
        let original = b
            .state
            .staged_original_job_v1(movement.claim)
            .map_err(|e| stage_refused(movement.claim, e))?
            .ok_or_else(|| stage_refused(movement.claim, "Float32 source original accepted job absent"))?;
        crate::palw_offence_attribution_v1::palw_constrained_job_target_v1(&target, &binding, &original.job)
            .map_err(|e| stage_refused(movement.claim, e.to_string()))?;
        if !original.job.is_constraint()
            || crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(
                &target,
                &binding,
                b.identity_rules_v1(ctx.daa_score),
                false,
            )
            .map_err(|e| stage_refused(movement.claim, e.to_string()))?
            .is_some()
        {
            return Err(stage_refused(movement.claim, "Float32 source substitutes constrained original binding"));
        }
        let contract = b
            .state
            .staged_claim_contract_v1(&movement.claim)
            .ok_or_else(|| stage_refused(movement.claim, "Float32 source lacks immutable custody"))?;
        if contract.producer == movement.reserver || contract.panel.contains(&movement.reserver) {
            return Err(stage_refused(movement.claim, "Float32 source owner must be outside entire original held Panel"));
        }
        let custody = contract
            .prefix_custody
            .as_ref()
            .ok_or_else(|| stage_refused(movement.claim, "Float32 source original signed material custody absent"))?
            .current_signed_v1();
        if !custody.public_qualified_v1() {
            return Err(stage_refused(movement.claim, "Float32 source semantic custody is unqualified"));
        }
        let custodians = custody.expected_panel.into_iter().chain([contract.producer]).collect();
        let config = b.extras.staged_public_court.expect("fence checked");
        let respond = bundle
            .plan
            .preseed_source_until_v1(crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint, config)
            .map_err(|e| stage_refused(movement.claim, e))?;
        let source = crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1::new_v1(
            bundle.claim,
            bundle.owner,
            bundle.reserved_daa,
            bundle.plan.id_v1(bundle.claim),
            contract.producer,
            custodians,
            &binding,
            &original.job,
            respond,
            stage.terms.collect_until_daa,
            config.prices,
        )
        .map_err(|e| stage_refused(movement.claim, e))?;
        source.check_open_window_v1(ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?;
        bundle.float = Some(Box::new(source).into());
        bundle.float_context = Some(
            Box::new(crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1 {
                binding: Box::new(binding),
                job: Box::new(original.job),
            })
            .into(),
        );
    } else {
        let source = bundle
            .float
            .as_mut()
            .ok_or_else(|| stage_refused(movement.claim, "Float32 answer/control lacks accepted original source"))?;
        let unpaid = match &movement.action {
            Action::FloatSourceAnswer(answer) => {
                source.answer_v1(ctx.daa_score, answer).map_err(|e| stage_refused(movement.claim, e))?;
                None
            }
            Action::FloatSourceNominate { source_id, position, beat } => {
                source
                    .nominate_v1(ctx.daa_score, movement.reserver, *source_id, *position, *beat)
                    .map_err(|e| stage_refused(movement.claim, e))?;
                None
            }
            Action::FloatSourceCollect => Some(
                source.collect_v1(ctx.daa_score).map_err(|e| stage_refused(movement.claim, e))?.into_iter().collect::<BTreeSet<_>>(),
            ),
            _ => return Err(stage_refused(movement.claim, "not a Float32 source action")),
        };
        if let Some(unpaid) = unpaid.filter(|s| !s.is_empty()) {
            let producer = source.producer;
            let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
            b.staged_selected_default_charge_v1(ctx, movement.claim, &claim, producer, &unpaid, movement.reserver)?;
        }
    }
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let row = PalwStagedCourtRowV1::OwnerBundle(bundle);
    if initial {
        b.write_staged_row_v1(key, Some(row))?;
    } else {
        let new = row.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
        let new_index = flat_response_index_rows_v1(&row);
        b.state.staged_courts.insert(key, row);
        update_flat_response_indices_v1(&mut b.state, old_index, new_index);
        b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    }
    Ok(())
}

fn bundle_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action;
    use crate::palw_staged_bundle_v1::*;
    movement.check().map_err(|e| stage_refused(movement.claim, e))?;
    let key = PalwStagedCourtKeyV1::Owner(movement.claim, movement.reserver);
    let Some(PalwStagedCourtRowV1::ClaimBundle(contract)) =
        b.state.staged_courts.get(&PalwStagedCourtKeyV1::Claim(movement.claim)).cloned()
    else {
        return Err(stage_refused(movement.claim, "bundle control has no accepted required-family contract"));
    };
    if let Action::BundleOpen { plan } = &movement.action {
        let reservation = b
            .state
            .legacy_dispute_reservation_v1(&movement.claim, &movement.reserver)
            .ok_or_else(|| stage_refused(movement.claim, "new bundle needs ordinary live reserve"))?;
        if reservation.reserved_daa != movement.reserved_daa
            || b.state.staged_courts.contains_key(&key)
            || ctx.daa_score < contract.plan.terms.accepted_daa
            || ctx.daa_score >= contract.plan.terms.open_until_daa
            || contract.claim.producer == movement.reserver
            || contract.claim.panel.contains(&movement.reserver)
            || movement.seed_id != plan.id_v1(movement.claim)
        {
            return Err(stage_refused(movement.claim, "bundle changes original owner, initial clock or immutable plan identity"));
        }
        let exact = b
            .state
            .staged_bundle_plan_v1(movement.claim, plan.inventory.as_ref().map(|i| i.maximum_bytes))
            .map_err(|e| stage_refused(movement.claim, e))?
            .ok_or_else(|| stage_refused(movement.claim, "bundle has no native quote"))?;
        if *plan != exact {
            return Err(stage_refused(movement.claim, "bundle plan changes native SUM or frozen allocation"));
        }
        let allowance = contract.config.maximum_owned_rows_per_reserver as usize;
        if b.state.staged_owner_allowance_used_v1(movement.reserver, allowance) >= allowance {
            return Err(stage_refused(movement.claim, "reserver's own bundle allowance is occupied"));
        }
        b.check_accuser_room(movement.reserver, u128::from(plan.terms.plan.deposit_sompi), "required family bundle", ctx.daa_score)?;
        let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &movement.claim)
            .ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        let inventory = match &plan.inventory {
            Some(inventory) => {
                let semantics = target
                    .registered_semantics
                    .as_ref()
                    .ok_or_else(|| stage_refused(movement.claim, "raw inventory has no immutable statement"))?;
                Some(PalwTokenizerInventoryV1::new_v1(
                    inventory,
                    semantics.tokenizer_id,
                    semantics
                        .raw_tokenizer_bytes
                        .ok_or_else(|| stage_refused(movement.claim, "raw inventory has no signed length"))?,
                    plan.inventory_until_v1().map_err(|e| stage_refused(movement.claim, e))?.expect("inventory selected"),
                ))
            }
            None => None,
        };
        let mut claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
        claim.trace_retention_daa = claim.trace_retention_daa.max(plan.terms.material_until_daa);
        let hard = crate::palw_legacy_public_filer_v1::palw_dispute_hard_deadline_v1(&claim, palw_da_disclose_window_daa_v1(b.params));
        let until = hard
            .checked_add(b.params.window_challenge_at(ctx.daa_score))
            .ok_or(PalwStateV2Error::Overflow("bundle native material window"))?
            .max(plan.terms.bond_exit_after_daa);
        super::palw_exec_v2_rewards_v1::require_native_session_window(&b.state, b.params, &movement.claim, until)?;
        // All checks precede writes. Only this original opening may extend the parent hold;
        // individual child seeds/activations never move any paid absolute door.
        let mut extended = contract.clone();
        extended.claim.terms.plan = extended.claim.terms.plan.cover(plan.terms.plan);
        extended.claim.terms.stream_until_daa = extended.claim.terms.stream_until_daa.max(plan.terms.stream_until_daa);
        extended.claim.terms.collect_until_daa = extended.claim.terms.collect_until_daa.max(plan.terms.collect_until_daa);
        extended.claim.terms.material_until_daa = extended.claim.terms.material_until_daa.max(plan.terms.material_until_daa);
        extended.claim.terms.bond_exit_after_daa = extended.claim.terms.bond_exit_after_daa.max(plan.terms.bond_exit_after_daa);
        let bundle = PalwOwnedStagedBundleV1 {
            claim: movement.claim,
            owner: movement.reserver,
            reserved_daa: movement.reserved_daa,
            execution_root: target.execution_root,
            plan: plan.clone(),
            inventory,
            selected: None,
            selected_context: None,
            selected_native: None,
            flat: None,
            flat_context: None,
            float: None,
            float_context: None,
            history_recovery: BTreeMap::new(),
            active: None,
            opened: Vec::new(),
        };
        bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
        b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::OwnerBundle(Box::new(bundle).into())))?;
        b.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(movement.claim), Some(PalwStagedCourtRowV1::ClaimBundle(extended)))?;
        b.write_claim(movement.claim, Some(claim))?;
        if let Some(mut dispute) = b.state.legacy_disputes.get(&movement.claim).cloned() {
            dispute.hard_deadline_daa = dispute.hard_deadline_daa.max(hard);
            b.write_legacy_dispute(movement.claim, Some(dispute))?;
        }
        b.da_rekey_v1(movement.claim, until, ctx.daa_score)?;
        if let Some(mut liability) = b.state.panel_liabilities.get(&movement.claim).cloned()
            && liability.voided_daa.is_none()
            && liability.void_reason.is_none()
            && liability.expiry_daa < until
        {
            liability.expiry_daa = until;
            b.write_panel_liability(movement.claim, Some(liability))?;
        }
        for (lock_key, mut lock) in b.claim_locks_v1(&movement.claim) {
            if lock.expiry_daa < until {
                lock.expiry_daa = until;
                b.write_slashable_lock(lock_key, Some(lock))?;
            }
        }
        return Ok(());
    }
    let Some(PalwStagedCourtRowV1::OwnerBundle(mut bundle)) = b.state.staged_courts.get(&key).cloned() else {
        return Err(stage_refused(movement.claim, "bundle control has no paid owner frame"));
    };
    let previous = bundle
        .active_seed_v1()
        .map(|seed| Hash64::from_bytes(seed.signing_message().try_into().expect("64")))
        .unwrap_or_else(|| bundle.plan.id_v1(bundle.claim));
    if movement.reserved_daa != bundle.reserved_daa
        || movement.seed_id != previous
        || ctx.daa_score < bundle.plan.terms.accepted_daa
        || ctx.daa_score >= bundle.plan.terms.stream_until_daa
    {
        return Err(stage_refused(movement.claim, "bundle control changes paid owner, seed or original clock"));
    }
    let old = PalwStagedCourtRowV1::OwnerBundle(bundle.clone()).checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    match &movement.action {
        Action::ActivateStage { previous_seed, next_seed } => {
            let next = bundle.active.map_or(0, |n| n as usize + 1);
            let stage =
                bundle.opened.get(next).ok_or_else(|| stage_refused(movement.claim, "activation has no signed pending child"))?;
            if *previous_seed != previous
                || stage.context.seed_id_v1() != *next_seed
                || bundle.active.is_some_and(|n| !bundle.opened[n as usize].row.honest_complete_v1().unwrap_or(false))
                || ctx.daa_score >= bundle.plan.stages[next].terms.open_until_daa
            {
                return Err(stage_refused(
                    movement.claim,
                    "activation changes strict honest predecessor, pending seed or fixed opening door",
                ));
            }
            bundle.active = Some(next as u8);
            bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
            b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::OwnerBundle(bundle)))?;
            return Ok(());
        }
        Action::InventoryPart { offset, bytes } => {
            if !bundle.opened.is_empty() {
                return Err(stage_refused(movement.claim, "inventory cannot change after first child seed"));
            }
            let plan = bundle.plan.inventory.clone().ok_or_else(|| stage_refused(movement.claim, "unrequired raw inventory"))?;
            bundle
                .inventory
                .as_mut()
                .ok_or_else(|| stage_refused(movement.claim, "raw inventory disappeared"))?
                .part_v1(ctx.daa_score, *offset, bytes, &plan)
                .map_err(|e| stage_refused(movement.claim, e))?;
        }
        Action::InventoryFinish => {
            if !bundle.opened.is_empty() {
                return Err(stage_refused(movement.claim, "inventory cannot change after first child seed"));
            }
            let plan = bundle.plan.inventory.clone().ok_or_else(|| stage_refused(movement.claim, "unrequired raw inventory"))?;
            bundle
                .inventory
                .as_mut()
                .ok_or_else(|| stage_refused(movement.claim, "raw inventory disappeared"))?
                .finish_v1(ctx.daa_score, &plan)
                .map_err(|e| stage_refused(movement.claim, e))?;
        }
        _ => return Err(stage_refused(movement.claim, "not a bundle control")),
    }
    bundle.check_v1().map_err(|e| stage_refused(movement.claim, e))?;
    let newrow = PalwStagedCourtRowV1::OwnerBundle(bundle);
    let new = newrow.checkpoint_v1().map_err(|e| stage_refused(movement.claim, e))?;
    b.state.staged_courts.insert(key, newrow);
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: (movement.claim, movement.reserver), old, new });
    Ok(())
}

/// Pure derived indexes; no additional rooted copy of a signer list or clock.
pub(super) fn index_rows(
    key: PalwStagedCourtKeyV1,
    row: &PalwStagedCourtRowV1,
) -> (u64, Vec<(PalwBondKeyV2, u64, PalwStagedCourtKeyV1)>) {
    match row {
        PalwStagedCourtRowV1::Claim(c) => {
            let PalwStagedCourtKeyV1::Claim(_) = key else {
                return (0, Vec::new());
            };
            let rows = std::iter::once(c.producer)
                .chain(c.panel.iter().copied())
                .map(|bond| (bond, c.terms.bond_exit_after_daa, key))
                .collect();
            (c.terms.bond_exit_after_daa, rows)
        }
        PalwStagedCourtRowV1::Constraint(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
        PalwStagedCourtRowV1::Tokenizer(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
        PalwStagedCourtRowV1::Prefix(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
        PalwStagedCourtRowV1::PublicPrefix(c) => {
            (c.pending_deadline_v1().unwrap_or(c.terms.material_until_daa), vec![(c.reserver, c.terms.material_until_daa, key)])
        }
        PalwStagedCourtRowV1::ClaimBundle(c) => index_rows(key, &PalwStagedCourtRowV1::Claim(c.claim.clone())),
        PalwStagedCourtRowV1::OwnerBundle(c) => {
            let due = match c.active_v1() {
                Some(PalwStagedCourtRowV1::PublicPrefix(prefix)) => {
                    prefix.pending_deadline_v1().unwrap_or(c.plan.terms.material_until_daa)
                }
                _ => c.plan.terms.material_until_daa,
            };
            (due, vec![(c.owner, c.plan.terms.material_until_daa, key)])
        }
        PalwStagedCourtRowV1::ArtifactInventoryOpen(c) => (c.until_daa, vec![(c.offer.opener, c.until_daa, key)]),
        PalwStagedCourtRowV1::ArtifactInventory(_) | PalwStagedCourtRowV1::TirLayout(_) | PalwStagedCourtRowV1::TirContainer(_) | PalwStagedCourtRowV1::CourtService(_) => {
            (u64::MAX, Vec::new())
        }
        PalwStagedCourtRowV1::CourtDutyClock(c) => (c.material_until_daa, Vec::new()),
        PalwStagedCourtRowV1::TirPrefix(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
        PalwStagedCourtRowV1::TirAcquisition(row) => {
            let c = row.owned.contract_v1();
            // Owner escrow only. The literal list is NOT a substitute for a
            // distinct native qualified custody grant; no signer hold or DA
            // sanction is created from mere membership in a drawn Panel.
            (c.terms.material_until_daa, vec![(c.owner, c.terms.material_until_daa, key)])
        }
        PalwStagedCourtRowV1::TirRawChunk(row) => (row.material_until_daa, Vec::new()),
        PalwStagedCourtRowV1::TirAcceptedBody(row) => (row.material_until_daa, Vec::new()),
        // This dormant row is a source lease, not proof of a funded pledge or
        // qualified Valid grant. Native admission must establish signer holds.
        PalwStagedCourtRowV1::TirPreparation(row) => (row.preparation.contract_v1().material_until_daa, Vec::new()),
        // A staged witness leaves at the frame's door close; it holds no bond (its filer is not liable on the claim).
        PalwStagedCourtRowV1::TirWitness(row) => (row.until_daa, Vec::new()),
        PalwStagedCourtRowV1::TirClaim(frame) => {
            if key != frame.key_v1() { return (0, Vec::new()); }
            let terms = frame.plan_v1().terms;
            let holds = frame.custody_v1().liable_signers_v1().into_iter()
                .map(|signer| (signer, terms.bond_exit_after_daa, key)).collect();
            (terms.material_until_daa, holds)
        }
    }
}
pub(super) fn update_indices(
    state: &mut PalwChainStateV2,
    key: PalwStagedCourtKeyV1,
    old: Option<&PalwStagedCourtRowV1>,
    new: Option<&PalwStagedCourtRowV1>,
) {
    if let Some(row) = old {
        update_tir_indices_v1(state, key, row, false);
        for entry in history_response_index_rows_v1(row) {
            state.staged_history_responders.remove(&entry);
        }
        for entry in flat_response_index_rows_v1(row) {
            state.staged_flat_responders.remove(&entry);
        }
        for entry in selected_response_index_rows_v1(row) {
            state.staged_selected_responders.remove(&entry);
        }
        for entry in response_index_rows(row) {
            state.staged_prefix_responders.remove(&entry);
        }
        let (due, holds) = index_rows(key, row);
        state.staged_due.remove(&(due, key));
        if let Some((claim, owner, _, _)) = row.owner_accounting_v1() {
            if matches!(row, PalwStagedCourtRowV1::ArtifactInventoryOpen(_)) {
                state.staged_inventory_by_owner.remove(&(owner, claim));
            } else {
                state.staged_by_owner.remove(&(owner, claim));
            }
        }
        for hold in holds {
            state.staged_holds.remove(&hold);
        }
    }
    if let Some(row) = new {
        update_tir_indices_v1(state, key, row, true);
        state.staged_history_responders.extend(history_response_index_rows_v1(row));
        state.staged_flat_responders.extend(flat_response_index_rows_v1(row));
        state.staged_selected_responders.extend(selected_response_index_rows_v1(row));
        state.staged_prefix_responders.extend(response_index_rows(row));
        let (due, holds) = index_rows(key, row);
        state.staged_due.insert((due, key));
        if let Some((claim, owner, _, _)) = row.owner_accounting_v1() {
            if matches!(row, PalwStagedCourtRowV1::ArtifactInventoryOpen(_)) {
                state.staged_inventory_by_owner.insert((owner, claim));
            } else {
                state.staged_by_owner.insert((owner, claim));
            }
        }
        state.staged_holds.extend(holds);
    }
}
pub(super) fn rebuild_indices(state: &mut PalwChainStateV2) {
    state.staged_holds.clear();
    state.staged_due.clear();
    state.staged_by_owner.clear();
    state.staged_inventory_by_owner.clear();
    state.staged_prefix_responders.clear();
    state.staged_selected_responders.clear();
    state.staged_flat_responders.clear();
    state.staged_history_responders.clear();
    state.staged_tir_by_owner.clear();
    state.staged_tir_claims_by_producer.clear();
    state.staged_tir_by_claim.clear();
    state.staged_tir_responders.clear();
    state.staged_tir_preparers.clear();
    for (key, row) in &state.staged_courts {
        if let Some(entry) = tir_claim_producer_index_v1(*key, row) {
            state.staged_tir_claims_by_producer.insert(entry);
        }
        if let Some((owner, claim, allocation, _)) = tir_index_metadata_v1(*key, row) {
            state.staged_tir_by_owner.insert((owner, claim, allocation));
            state.staged_tir_responders.extend(tir_response_index_rows_v1(*key, row));
        }
        if let Some((claim, until)) = tir_source_index_metadata_v1(*key, row) {
            state.staged_tir_by_claim.entry(claim).or_default().insert((until, *key));
        }
        state.staged_tir_preparers.extend(tir_preparation_index_rows_v1(*key, row));
        state.staged_history_responders.extend(history_response_index_rows_v1(row));
        state.staged_flat_responders.extend(flat_response_index_rows_v1(row));
        state.staged_selected_responders.extend(selected_response_index_rows_v1(row));
        let (due, holds) = index_rows(*key, row);
        state.staged_prefix_responders.extend(response_index_rows(row));
        state.staged_due.insert((due, *key));
        if let Some((claim, owner, _, _)) = row.owner_accounting_v1() {
            if matches!(row, PalwStagedCourtRowV1::ArtifactInventoryOpen(_)) {
                state.staged_inventory_by_owner.insert((owner, claim));
            } else {
                state.staged_by_owner.insert((owner, claim));
            }
        }
        state.staged_holds.extend(holds);
    }
}

pub(super) fn tir_index_metadata_v1(
    key: PalwStagedCourtKeyV1,
    row: &PalwStagedCourtRowV1,
) -> Option<(PalwBondKeyV2, Hash64, Hash64, u64)> {
    let (PalwStagedCourtKeyV1::TirAcquisition(claim, owner, allocation), PalwStagedCourtRowV1::TirAcquisition(row)) = (key, row) else {
        return None;
    };
    let c = row.owned.contract_v1();
    if (c.claim, c.owner) != (claim, owner) { return None; }
    Some((owner, claim, allocation, c.terms.material_until_daa))
}
pub(super) fn tir_source_index_metadata_v1(
    key: PalwStagedCourtKeyV1,
    row: &PalwStagedCourtRowV1,
) -> Option<(Hash64, u64)> {
    match (key, row) {
        (PalwStagedCourtKeyV1::TirClaim(claim), PalwStagedCourtRowV1::TirClaim(frame)) => {
            (frame.key_v1() == key).then_some((claim, frame.plan_v1().terms.material_until_daa))
        }
        (PalwStagedCourtKeyV1::TirPreparation(claim, allocation), PalwStagedCourtRowV1::TirPreparation(row)) => {
            let c = row.preparation.contract_v1();
            ((claim, allocation) == (c.claim, c.allocation_id)).then_some((claim, c.material_until_daa))
        }
        _ => tir_index_metadata_v1(key, row).map(|(_, claim, _, until)| (claim, until)),
    }
}
pub(super) fn tir_claim_producer_index_v1(
    key: PalwStagedCourtKeyV1,
    row: &PalwStagedCourtRowV1,
) -> Option<(PalwBondKeyV2, Hash64)> {
    let (PalwStagedCourtKeyV1::TirClaim(claim), PalwStagedCourtRowV1::TirClaim(frame)) = (key, row) else { return None };
    (frame.key_v1() == key).then_some((frame.custody_v1().pledge_v1().producer, claim))
}
pub(super) fn tir_preparation_index_rows_v1(
    key: PalwStagedCourtKeyV1,
    row: &PalwStagedCourtRowV1,
) -> Vec<(PalwBondKeyV2, u64, Hash64, Hash64)> {
    let (PalwStagedCourtKeyV1::TirPreparation(claim, allocation), PalwStagedCourtRowV1::TirPreparation(row)) = (key, row) else {
        return Vec::new();
    };
    let c = row.preparation.contract_v1();
    if (claim, allocation) != (c.claim, c.allocation_id) { return Vec::new(); }
    row.preparation.pending_signers_v1().into_iter()
        .map(|signer| (signer, c.preparation_until_daa, claim, allocation)).collect()
}
/// Exact current unpaid literal slots, not a Panel draw and not an admission or
/// collection credential. Native versioned admission still owes grant checks.
pub(super) fn tir_response_index_rows_v1(
    key: PalwStagedCourtKeyV1,
    row: &PalwStagedCourtRowV1,
) -> Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2, Hash64, u64, Hash64)> {
    let Some((owner, claim, allocation, _)) = tir_index_metadata_v1(key, row) else { return Vec::new() };
    let PalwStagedCourtRowV1::TirAcquisition(row) = row else { return Vec::new() };
    let Some(pending) = row.owned.active_v1().filter(|p| !p.collected) else { return Vec::new() };
    row.owned.response_custodians_v1().iter().filter(|signer| {
        pending.slots.get(signer).is_none_or(|slot| slot.finished.is_none())
    }).map(|signer| (*signer, pending.respond_until_daa, claim, owner, allocation, pending.turn, pending.demand_id)).collect()
}
fn update_tir_indices_v1(state: &mut PalwChainStateV2, key: PalwStagedCourtKeyV1, row: &PalwStagedCourtRowV1, insert: bool) {
    if let Some(entry) = tir_claim_producer_index_v1(key, row) {
        if insert { state.staged_tir_claims_by_producer.insert(entry); }
        else { state.staged_tir_claims_by_producer.remove(&entry); }
    }
    if let Some((owner, claim, allocation, _)) = tir_index_metadata_v1(key, row) {
        if insert {
            state.staged_tir_by_owner.insert((owner, claim, allocation));
            state.staged_tir_responders.extend(tir_response_index_rows_v1(key, row));
        } else {
            state.staged_tir_by_owner.remove(&(owner, claim, allocation));
            for entry in tir_response_index_rows_v1(key, row) { state.staged_tir_responders.remove(&entry); }
        }
    }
    if let Some((claim, until)) = tir_source_index_metadata_v1(key, row) {
        if insert {
            state.staged_tir_by_claim.entry(claim).or_default().insert((until, key));
        } else if let Some(rows) = state.staged_tir_by_claim.get_mut(&claim) {
            rows.remove(&(until, key));
            if rows.is_empty() { state.staged_tir_by_claim.remove(&claim); }
        }
    }
    for entry in tir_preparation_index_rows_v1(key, row) {
        if insert { state.staged_tir_preparers.insert(entry); }
        else { state.staged_tir_preparers.remove(&entry); }
    }
}
pub(super) fn selected_response_index_rows_v1(row: &PalwStagedCourtRowV1) -> Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)> {
    let PalwStagedCourtRowV1::OwnerBundle(bundle) = row else { return Vec::new() };
    let Some(selected) = &bundle.selected else { return Vec::new() };
    selected
        .custodians
        .iter()
        .filter(|signer| selected.requests.values().any(|r| !r.collected && !r.answered.contains(signer)))
        .map(|signer| (*signer, selected.respond_until_daa, bundle.claim, bundle.owner))
        .collect()
}
/// The streamed-source duty index: a Flat source's custodians and a greedy Float32 source's
/// (one bundle holds at most one of them), so a node's source responder reads one page.
pub(super) fn flat_response_index_rows_v1(row: &PalwStagedCourtRowV1) -> Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)> {
    let PalwStagedCourtRowV1::OwnerBundle(bundle) = row else { return Vec::new() };
    if let Some(PalwStagedCourtRowV1::TirPrefix(court)) = bundle.active_v1() {
        return court
            .custodians
            .iter()
            .filter(|key| court.has_response_duty_v1(key))
            .filter_map(|key| court.response_deadline_v1(key).map(|at| (*key, at, bundle.claim, bundle.owner)))
            .collect();
    }
    if let Some(float) = &bundle.float {
        return float
            .custodians
            .iter()
            .filter(|key| float.has_response_duty_v1(key))
            .filter_map(|key| float.response_deadline_v1(key).map(|at| (*key, at, bundle.claim, bundle.owner)))
            .collect();
    }
    let Some(flat) = &bundle.flat else { return Vec::new() };
    flat.custodians
        .iter()
        .filter(|key| flat.has_response_duty_v1(key))
        .filter_map(|key| flat.response_deadline_v1(key).map(|at| (*key, at, bundle.claim, bundle.owner)))
        .collect()
}
pub(super) fn history_response_index_rows_v1(row: &PalwStagedCourtRowV1) -> Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2, Hash64)> {
    let PalwStagedCourtRowV1::OwnerBundle(bundle) = row else { return Vec::new() };
    bundle
        .history_recovery
        .iter()
        .flat_map(|(seed, recovery)| {
            let demand = recovery.demand.as_ref().filter(|_| recovery.default.is_none());
            recovery
                .custodians
                .iter()
                .filter_map(move |signer| demand.map(|d| (*signer, d.answer_until_daa, bundle.claim, bundle.owner, *seed)))
        })
        .collect()
}
pub(super) fn update_history_response_indices_v1(
    state: &mut PalwChainStateV2,
    old: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2, Hash64)>,
    new: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2, Hash64)>,
) {
    for entry in old {
        state.staged_history_responders.remove(&entry);
    }
    state.staged_history_responders.extend(new);
}
pub(super) fn update_flat_response_indices_v1(
    state: &mut PalwChainStateV2,
    old: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)>,
    new: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)>,
) {
    for entry in old {
        state.staged_flat_responders.remove(&entry);
    }
    state.staged_flat_responders.extend(new);
}
pub(super) fn update_selected_response_indices_v1(
    state: &mut PalwChainStateV2,
    old: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)>,
    new: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)>,
) {
    for entry in old {
        state.staged_selected_responders.remove(&entry);
    }
    state.staged_selected_responders.extend(new);
}
pub(super) fn response_index_rows(row: &PalwStagedCourtRowV1) -> Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)> {
    let Some(PalwStagedCourtRowV1::PublicPrefix(row)) = row.active_row_v1() else { return Vec::new() };
    public_response_index_rows(row)
}
fn public_response_index_rows(
    row: &crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1,
) -> Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)> {
    let Some(deadline) = row.pending_response_deadline_v1() else { return Vec::new() };
    let Some(pending) = row.pending_progress_v1() else { return Vec::new() };
    pending
        .signers
        .into_iter()
        .filter(|(_, cursor)| *cursor != crate::palw_prefix_public_source_v1::PalwPrefixPublicSignerCursorV1::Invalid)
        .map(|(signer, _)| (signer, deadline, row.claim, row.reserver))
        .collect()
}
pub(super) fn update_response_indices(
    state: &mut PalwChainStateV2,
    old: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)>,
    new: Vec<(PalwBondKeyV2, u64, Hash64, PalwBondKeyV2)>,
) {
    for row in old {
        state.staged_prefix_responders.remove(&row);
    }
    state.staged_prefix_responders.extend(new);
}
pub(super) fn sweep(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    // The due index gives rooted-clock ordering;64 row removals bound every accepted block.
    let due: Vec<_> = b.state.staged_due.iter().take_while(|(at, _)| *at <= ctx.daa_score)
        .filter(|(_, key)| !b.state.court_service_holds_material_v1(key))
        .take(64).map(|(_, key)| *key).collect();
    for key in due {
        let Some(mut saved) = b.state.staged_courts.get(&key).cloned() else { continue };
        // Acquisition metadata and immutable public bytes have their own exact
        // paid material TTL, not an old mutable court checkpoint or DA default.
        // No semantic close, sibling default, or expired response erases them.
        let tir_until = match &saved {
            PalwStagedCourtRowV1::TirAcquisition(row) => Some(row.owned.contract_v1().terms.material_until_daa),
            PalwStagedCourtRowV1::TirRawChunk(row) => Some(row.material_until_daa),
            PalwStagedCourtRowV1::TirAcceptedBody(row) => Some(row.material_until_daa),
            PalwStagedCourtRowV1::TirPreparation(row) => Some(row.preparation.contract_v1().material_until_daa),
            PalwStagedCourtRowV1::TirClaim(frame) => Some(frame.plan_v1().terms.material_until_daa),
            PalwStagedCourtRowV1::TirWitness(row) => Some(row.until_daa),
            PalwStagedCourtRowV1::CourtDutyClock(row) => Some(row.material_until_daa),
            _ => None,
        };
        if let Some(until) = tir_until {
            if ctx.daa_score >= until { b.write_staged_row_v1(key, None)?; }
            continue;
        }
        let material = saved.owner_accounting_v1().map(|(_, _, _, until)| until);
        if material.is_none_or(|until| ctx.daa_score >= until) {
            b.write_staged_row_v1(key, None)?;
            continue;
        }
        let old = saved.checkpoint_v1().map_err(|e| stage_refused(ZERO_HASH64, e))?;
        let old_due = index_rows(key, &saved).0;
        let old_responses = response_index_rows(&saved);
        let Some(PalwStagedCourtRowV1::PublicPrefix(row)) = saved.active_row_mut_v1() else { continue };
        let row_key = (row.claim, row.reserver);
        let closed =
            b.state.claim(&row.claim).is_some_and(|c| c.phase.is_terminal() && !matches!(c.phase, PalwClaimPhaseV2::Final { .. }));
        if closed {
            row.mark_claim_closed_v1().map_err(|e| stage_refused(row.claim, e))?;
        }
        let default = row.joint_default_v1(ctx.daa_score).map_err(|e| stage_refused(row.claim, e))?;
        if let Some(default) = &default {
            row.mark_default_collected_v1(ctx.daa_score).map_err(|e| stage_refused(row.claim, e))?;
            collect_prefix_default(b, ctx, default)?;
        } else {
            row.mark_collection_expired_v1(ctx.daa_score).map_err(|e| stage_refused(row.claim, e))?;
        }
        let new = saved.checkpoint_v1().map_err(|e| stage_refused(row_key.0, e))?;
        let new_due = index_rows(key, &saved).0;
        let new_responses = response_index_rows(&saved);
        b.state.staged_courts.insert(key, saved);
        b.state.staged_due.remove(&(old_due, key));
        b.state.staged_due.insert((new_due, key));
        b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key: row_key, old, new });
        update_response_indices(&mut b.state, old_responses, new_responses);
    }
    Ok(())
}
fn collect_prefix_default(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    default: &crate::palw_prefix_public_owned_v1::PalwPrefixJointDefaultV1,
) -> Result<(), PalwStateV2Error> {
    let claim = b.state.claim(&default.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(default.claim))?;
    // The expired pending source proves these exact frozen signers, not a new
    // Panel draw. A sibling close cannot discharge an unconsumed Valid signer;
    // the existing economic helper skips an already charged producer and S4 key.
    let unpaid = default.panel.iter().copied().chain(std::iter::once(default.producer)).collect();
    b.staged_selected_default_charge_v1(ctx, default.claim, &claim, default.producer, &unpaid, default.owner)
}
