//! Separately prepaid acquisition of the exact next original prefix source. Tentative
//! custodian replies mutate only fixed protected slots; only full old-leaf authentication
//! advances the owner's independently charged semantic continuation. Outer fold verifies
//! signatures, actual credited custody and ALL liability/material holds before admission.
use crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1;
use crate::palw_prefix_owned_court_v1::{PalwOwnedPrefixCheckpointV1, PalwOwnedPrefixCourtV1};
use crate::palw_prefix_public_source_v1::*;
use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedPublicPrefixCourtV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub terms: PalwRenderingClaimTermsV1,
    pub rent_paid: u64,
    pub deposit_locked: u64,
    producer: PalwBondKeyV2,
    custody: PalwPrefixCustodyContractV1,
    plan: PalwPrefixPublicAcquisitionPlanV1,
    maximum_leaves: u64,
    inner: PalwOwnedPrefixCourtV1,
    pending: Option<PalwPrefixPublicPendingV1>,
    sources_opened: u64,
    public_moves_used: u64,
    public_carriers_used: u64,
    public_carrier_bytes_used: u64,
    default_collected: bool,
    collection_expired: bool,
    claim_closed: bool,
    history_count: u64,
    history_head: Hash64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedPublicPrefixCheckpointV1 {
    seed_id: Hash64,
    inner: PalwOwnedPrefixCheckpointV1,
    pending: Option<PalwPrefixPublicPendingV1>,
    sources_opened: u64,
    public_moves_used: u64,
    public_carriers_used: u64,
    public_carrier_bytes_used: u64,
    default_collected: bool,
    collection_expired: bool,
    claim_closed: bool,
    history_count: u64,
    history_head: Hash64,
    history_material_until_daa: u64,
}
impl PalwOwnedPublicPrefixCheckpointV1 {
    pub fn history_cursor_v1(&self) -> (u64, Hash64) {
        (self.history_count, self.history_head)
    }
    pub fn history_material_until_v1(&self) -> u64 {
        self.history_material_until_daa
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPrefixJointDefaultV1 {
    pub claim: Hash64,
    pub producer: PalwBondKeyV2,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub cursor_id: Hash64,
    pub demand_id: Hash64,
    pub panel: BTreeSet<PalwBondKeyV2>,
    pub deadline_daa: u64,
}
impl PalwOwnedPublicPrefixCourtV1 {
    pub(crate) fn admit(
        inner: PalwOwnedPrefixCourtV1,
        producer: PalwBondKeyV2,
        mut custody: PalwPrefixCustodyContractV1,
        plan: PalwPrefixPublicAcquisitionPlanV1,
        mut terms: PalwRenderingClaimTermsV1,
        maximum_leaves: u64,
        available_capital: u64,
    ) -> Result<Self, String> {
        if !custody.public_qualified_v1()
            || custody.execution_root != inner.execution_root_v1()
            || custody.accepted_daa != terms.accepted_daa
            || terms.accepted_daa != inner.terms.accepted_daa
            || terms.open_until_daa != inner.terms.open_until_daa
            || terms.stream_until_daa < inner.terms.stream_until_daa
            || terms.collect_until_daa < inner.terms.collect_until_daa
            || terms.material_until_daa < inner.terms.material_until_daa
            || terms.bond_exit_after_daa < inner.terms.bond_exit_after_daa
        {
            return Err("public prefix court lacks exact qualified prospective custody/immutable opening clock".into());
        }
        let need = plan.envelope.rent_sompi.checked_add(plan.envelope.deposit_sompi).ok_or("public prefix funding overflow")?;
        if available_capital < need {
            return Err("public prefix complete hostile acquisition work is not funded".into());
        }
        for (actual, required) in [
            (terms.plan.maximum_moves, plan.envelope.maximum_moves),
            (terms.plan.maximum_stream_bytes, plan.envelope.maximum_stream_bytes),
            (terms.plan.maximum_stream_leaves, plan.envelope.maximum_stream_leaves),
            (terms.plan.maximum_parser_steps, plan.envelope.maximum_parser_steps),
            (terms.plan.maximum_blake_blocks, plan.envelope.maximum_blake_blocks),
            (terms.plan.inclusion_daa, plan.envelope.inclusion_daa),
            (terms.plan.row_bytes, plan.envelope.row_bytes),
            (terms.plan.resident_ram_bytes, plan.envelope.resident_ram_bytes),
        ] {
            if actual < required {
                return Err("public prefix complete plan exceeds paid extended claim terms".into());
            }
        }
        // Only actual current qualified custodians are liable for this fixed seed;
        // earlier drawn credited seats remain in the claim's separate historic holds.
        custody.credited.retain(|seat, _| custody.expected_panel.contains(seat));
        terms.plan = plan.envelope;
        let row = Self {
            claim: inner.claim,
            reserver: inner.reserver,
            reserved_daa: inner.reserved_daa,
            rent_paid: plan.envelope.rent_sompi,
            deposit_locked: plan.envelope.deposit_sompi,
            terms,
            producer,
            custody,
            plan,
            maximum_leaves,
            inner,
            pending: None,
            sources_opened: 0,
            public_moves_used: 0,
            public_carriers_used: 0,
            public_carrier_bytes_used: 0,
            default_collected: false,
            collection_expired: false,
            claim_closed: false,
            history_count: 0,
            history_head: Hash64::default(),
        };
        row.check()?;
        Ok(row)
    }
    pub fn key(&self) -> (Hash64, PalwBondKeyV2, u64) {
        (self.claim, self.reserver, self.reserved_daa)
    }
    pub fn seed_id(&self) -> Hash64 {
        self.inner.seed_id()
    }
    pub fn execution_root_v1(&self) -> Hash64 {
        self.inner.execution_root_v1()
    }
    pub fn admitted_context_v1(&self) -> (&crate::palw_step_leg::PalwStepBindingV2, &crate::palw_freeprompt_v3::PalwFreePromptJobV3) {
        self.inner.court_v1().admitted_context_v1()
    }
    pub fn page_root_v1(&self) -> Option<(Hash64, u64, u8)> {
        self.inner.court_v1().page_root_v1()
    }
    pub fn history_cursor_v1(&self) -> (u64, Hash64) {
        (self.history_count, self.history_head)
    }
    pub(crate) fn append_history_v1(
        &mut self,
        record: &crate::palw_prefix_history_v1::PalwPrefixHistoryRecordV1,
    ) -> Result<(), String> {
        let next = self.history_count.checked_add(1).ok_or("public prefix history sequence overflow")?;
        if record.previous_head != self.history_head || record.sequence != next {
            return Err("public prefix history changes exact accepted predecessor".into());
        }
        let head = record.head_v1()?;
        record.authenticates_v1(
            head,
            next,
            self.claim,
            self.reserver,
            self.reserved_daa,
            self.seed_id(),
            self.terms.material_until_daa,
        )?;
        self.history_count = next;
        self.history_head = head;
        self.check()
    }
    pub fn custodian_bonds_v1(&self) -> BTreeSet<PalwBondKeyV2> {
        self.custody.expected_panel.iter().copied().chain(std::iter::once(self.producer)).collect()
    }
    pub fn preview_page_write_v1(
        &self,
        opening: &crate::palw_prefix_gdn_paged_v1::PalwGdnOwnedPageOpeningV1,
    ) -> Result<Vec<i32>, String> {
        self.check()?;
        if self.pending.is_some() {
            return Err("public prefix has no live deterministic owner page transition".into());
        }
        self.inner.court_v1().preview_page_write_v1(opening)
    }
    pub fn check_claim_identity_v1(
        &self,
        execution_root: Hash64,
        job_identity: Hash64,
        cu_ruleset_id: Hash64,
        producer: PalwBondKeyV2,
        held_panel: &BTreeSet<PalwBondKeyV2>,
    ) -> Result<(), String> {
        self.check()?;
        self.custody.check(held_panel)?;
        if self.producer != producer
            || self.custody.execution_root != execution_root
            || self.custody.job_identity != job_identity
            || self.custody.cu_ruleset_id != cu_ruleset_id
            || producer == self.reserver
            || held_panel.contains(&self.reserver)
        {
            return Err("public prefix differs from accepted claim/immutable custody identity".into());
        }
        Ok(())
    }
    pub fn check(&self) -> Result<(), String> {
        self.inner.check()?;
        self.custody.check(&self.custody.expected_panel)?;
        if !self.custody.public_qualified_v1()
            || (self.history_count == 0) != (self.history_head == Hash64::default())
            || self.history_count > self.plan.envelope.maximum_moves
            || self.custody.execution_root != self.inner.execution_root_v1()
            || self.key() != self.inner.key()
            || self.terms.plan != self.plan.envelope
            || self.rent_paid != self.plan.envelope.rent_sompi
            || self.deposit_locked != self.plan.envelope.deposit_sompi
            || self.public_moves_used > self.plan.maximum_public_moves
            || self.public_carriers_used > self.plan.maximum_public_moves
            || self.public_carrier_bytes_used > self.plan.maximum_public_carrier_bytes
            || self.sources_opened > self.plan.maximum_sources
            || self.terms.stream_until_daa > self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa > self.terms.bond_exit_after_daa
        {
            return Err("public prefix paid ownership/custody/cumulative contract inconsistent".into());
        }
        if let Some(p) = &self.pending {
            p.check()?;
            let (cursor, demand) = p.identity_v1();
            let expected_custodians: BTreeSet<_> =
                self.custody.expected_panel.iter().copied().chain(std::iter::once(self.producer)).collect();
            if p.custodians_v1() != &expected_custodians {
                return Err("public pending changes literal signed custodian set".into());
            }
            if !p.matches(self.claim, self.reserver, self.reserved_daa, self.seed_id(), cursor, demand)
                || p.deadline_daa_v1() > self.terms.stream_until_daa
            {
                return Err("public prefix pending changes immutable owner/clock".into());
            }
            let (source, header) = self.inner.next_public_source_hash_header_v1()?.ok_or("pending original source disappeared")?;
            let expected = PalwPrefixPublicSourceV1::from_owned_cursor_v1(source, header, self.maximum_leaves)?;
            if p.source() != &expected {
                return Err("pending source differs from derived unchanged owner continuation".into());
            }
        } else if self.default_collected || self.collection_expired {
            return Err("collected default has no retained literal pending obligation".into());
        }
        if self.default_collected && self.collection_expired {
            return Err("public default cannot be both collected and expired".into());
        }
        Ok(())
    }
    fn live_owner(&self, now: u64, claim: Hash64, owner: PalwBondKeyV2, reserved: u64, seed: Hash64) -> Result<(), String> {
        self.check()?;
        if self.key() != (claim, owner, reserved)
            || seed != self.seed_id()
            || self.default_collected
            || now < self.terms.accepted_daa
            || now >= self.terms.stream_until_daa
        {
            return Err("public prefix action changes owner/seed or contract clock".into());
        }
        Ok(())
    }
    fn debit_public(&mut self, logical: u64, carrier_bytes: u64) -> Result<(), String> {
        if carrier_bytes > 90_000 {
            return Err("public prefix signed carrier exceeds90k".into());
        }
        let moves = self.public_moves_used.checked_add(logical).ok_or("public prefix logical count overflow")?;
        let carriers = self.public_carriers_used.checked_add(1).ok_or("public prefix physical count overflow")?;
        let bytes = self.public_carrier_bytes_used.checked_add(carrier_bytes).ok_or("public prefix carrier bytes overflow")?;
        if moves > self.plan.maximum_public_moves
            || carriers > self.plan.maximum_public_moves
            || bytes > self.plan.maximum_public_carrier_bytes
        {
            return Err("public prefix cumulative prepaid attempt work exhausted".into());
        }
        self.public_moves_used = moves;
        self.public_carriers_used = carriers;
        self.public_carrier_bytes_used = bytes;
        Ok(())
    }
    pub fn next_demand_v1(&self, now: u64) -> Result<Option<PalwPrefixPublicDemandV1>, String> {
        self.check()?;
        if self.pending.is_some() || self.default_collected {
            return Ok(None);
        }
        let Some((source, header)) = self.inner.next_public_source_hash_header_v1()? else { return Ok(None) };
        if !source.needs_opening || source.offset != 0 {
            return Ok(None);
        }
        let source = PalwPrefixPublicSourceV1::from_owned_cursor_v1(source, header, self.maximum_leaves)?;
        let span = self.plan.maximum_response_daa;
        let earliest = now.checked_add(span).ok_or("public prefix deadline overflow")?;
        if now < self.terms.accepted_daa || earliest > self.terms.stream_until_daa {
            // No remaining paid response horizon: there is simply no demand to make. This is
            // not an error that would block the owner's other stages or progress readers.
            return Ok(None);
        }
        // The owner signs this at its own snapshot clock; the demand must stay admissible for one
        // more response span of inclusion delay, so the signed deadline carries that slack.
        // `demand_v1` re-checks it against the ACCEPTING block's clock: every custodian still
        // gets at least the full span, and the deadline never exceeds two spans or the stream.
        // Aligned to the span so an owner rebuilding its unsent demand on every poll signs the
        // SAME carrier for a whole span (no fee spent on a replacement that would be refused).
        let aligned = now - now % span.max(1);
        let deadline = span
            .checked_mul(2)
            .and_then(|n| aligned.checked_add(n))
            .ok_or("public prefix deadline overflow")?
            .min(self.terms.stream_until_daa);
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/prefix-public/cursor/v1").to_state();
        h.update(self.claim.as_byte_slice());
        h.update(self.seed_id().as_byte_slice());
        h.update(&self.reserved_daa.to_le_bytes());
        h.update(&borsh::to_vec(&source).map_err(|e| e.to_string())?);
        let cursor_id = Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64bytes"));
        Ok(Some(PalwPrefixPublicDemandV1 { cursor_id, coordinate: source.coordinate, deadline_daa: deadline }))
    }
    /// The outer verified OWNER signature binds demand_id to the complete literal carrier.
    pub(crate) fn demand_v1(
        &mut self,
        now: u64,
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved: u64,
        seed: Hash64,
        demand_id: Hash64,
        demand: &PalwPrefixPublicDemandV1,
        carrier_bytes: u64,
    ) -> Result<(), String> {
        self.live_owner(now, claim, owner, reserved, seed)?;
        // The cursor and coordinate are clock-free derivations; the signed deadline is checked
        // against THIS (accepting) clock: at least one full response span from now, at most the
        // derivation's own bound. A node's snapshot DAA never has to equal the block's.
        let expected = self.next_demand_v1(now)?.ok_or("public prefix lacks remaining paid response horizon")?;
        let earliest = now.checked_add(self.plan.maximum_response_daa).ok_or("public prefix deadline overflow")?;
        if (expected.cursor_id, expected.coordinate) != (demand.cursor_id, demand.coordinate)
            || demand.deadline_daa < earliest
            || demand.deadline_daa > expected.deadline_daa
        {
            return Err("public demand is not the derived pending cursor or leaves no full response span".into());
        }
        let (source, header) = self.inner.next_public_source_hash_header_v1()?.ok_or("public source absent")?;
        let source = PalwPrefixPublicSourceV1::from_owned_cursor_v1(source, header, self.maximum_leaves)?;
        let custodians = self.custody.expected_panel.iter().copied().chain(std::iter::once(self.producer)).collect();
        let pending = PalwPrefixPublicPendingV1::new(
            claim,
            owner,
            reserved,
            seed,
            demand.cursor_id,
            demand_id,
            now,
            demand.deadline_daa,
            source,
            custodians,
        )?;
        let mut next = self.clone();
        next.debit_public(1, carrier_bytes)?;
        next.sources_opened = next.sources_opened.checked_add(1).ok_or("public prefix source count overflow")?;
        next.pending = Some(pending);
        next.check()?;
        *self = next;
        Ok(())
    }
    /// Signature is against answer.answerer, never treated as owner movement authority.
    pub(crate) fn answer_v1(
        &mut self,
        now: u64,
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved: u64,
        seed: Hash64,
        answer: &PalwPrefixPublicAnswerV1,
        carrier_bytes: u64,
    ) -> Result<(), String> {
        self.live_owner(now, claim, owner, reserved, seed)?;
        answer.check()?;
        let mut next = self.clone();
        let pending = next.pending.as_mut().ok_or("public answer has no literal owner demand")?;
        if !pending.matches(claim, owner, reserved, seed, answer.cursor_id, answer.demand_id) {
            return Err("public answer changes literal pending identity".into());
        }
        let logical = pending.packet(answer.answerer, now, &answer.action)?;
        next.debit_public(logical, carrier_bytes)?;
        if let Some((opening, raw)) = next.pending.as_ref().and_then(|p| p.authenticated_answer()) {
            next.inner.consume_authenticated_public_source_v1(now, claim, owner, reserved, seed, opening, raw)?;
            next.pending = None;
        }
        next.check()?;
        *self = next;
        Ok(())
    }
    pub(crate) fn owner_batch_v1(
        &mut self,
        now: u64,
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved: u64,
        seed: Hash64,
        actions: &[crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1],
        carrier_bytes: u64,
    ) -> Result<(), String> {
        self.live_owner(now, claim, owner, reserved, seed)?;
        if self.pending.is_some() {
            return Err("owner must resolve its pending public source before changing cursor".into());
        }
        self.inner.movement_batch_v1(now, claim, owner, reserved, seed, actions, carrier_bytes)
    }
    pub fn cursor_v1(&self) -> Result<crate::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1, String> {
        self.check()?;
        self.inner.cursor_v1()
    }
    pub fn pending_progress_v1(&self) -> Option<PalwPrefixPublicProgressV1> {
        self.pending.as_ref().map(|p| p.progress_v1())
    }
    pub fn pending_deadline_v1(&self) -> Option<u64> {
        (!self.default_collected && !self.collection_expired && !self.claim_closed)
            .then(|| self.pending.as_ref().map(|p| p.deadline_daa_v1()))
            .flatten()
    }
    pub fn pending_response_deadline_v1(&self) -> Option<u64> {
        (!self.default_collected && !self.collection_expired).then(|| self.pending.as_ref().map(|p| p.deadline_daa_v1())).flatten()
    }
    pub fn certificate(&self) -> Result<Option<crate::palw_prefix_owned_court_v1::PalwPrefixCourtFaultV1>, String> {
        self.check()?;
        if self.pending.is_some() { Ok(None) } else { self.inner.certificate() }
    }
    pub(crate) fn joint_default_v1(&self, now: u64) -> Result<Option<PalwPrefixJointDefaultV1>, String> {
        self.check()?;
        if self.default_collected || self.collection_expired || self.claim_closed || now >= self.terms.collect_until_daa {
            return Ok(None);
        }
        let Some(pending) = &self.pending else { return Ok(None) };
        if pending.defaulted_custodians(now).is_none() {
            return Ok(None);
        }
        let (cursor_id, demand_id) = pending.identity_v1();
        Ok(Some(PalwPrefixJointDefaultV1 {
            claim: self.claim,
            producer: self.producer,
            owner: self.reserver,
            reserved_daa: self.reserved_daa,
            seed_id: self.seed_id(),
            cursor_id,
            demand_id,
            panel: self.custody.expected_panel.clone(),
            deadline_daa: pending.deadline_daa_v1(),
        }))
    }
    pub(crate) fn mark_default_collected_v1(&mut self, now: u64) -> Result<(), String> {
        if self.joint_default_v1(now)?.is_none() {
            return Err("public default is not due within collection clock".into());
        }
        self.default_collected = true;
        self.check()
    }
    pub(crate) fn mark_collection_expired_v1(&mut self, now: u64) -> Result<(), String> {
        if now < self.terms.collect_until_daa || self.pending.is_none() || self.default_collected {
            return Err("public collection has not expired with retained pending material".into());
        }
        self.collection_expired = true;
        self.check()
    }
    /// Only the native fold's independently terminal claim edge authorizes this.
    /// The literal unpaid/default material survives until its purchased retention.
    pub(crate) fn mark_claim_closed_v1(&mut self) -> Result<(), String> {
        self.claim_closed = true;
        self.check()
    }
    pub(crate) fn collection_v1(
        &mut self,
        now: u64,
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved: u64,
        seed: Hash64,
        cursor: Hash64,
        demand: Hash64,
        bytes: u64,
    ) -> Result<PalwPrefixJointDefaultV1, String> {
        if self.key() != (claim, owner, reserved) || seed != self.seed_id() {
            return Err("public collection changes literal paid owner/seed".into());
        }
        let default = self.joint_default_v1(now)?.ok_or("public collection is not due within immutable collection clock")?;
        if (cursor, demand) != (default.cursor_id, default.demand_id) {
            return Err("public collection changes literal pending cursor/demand".into());
        }
        let mut next = self.clone();
        next.debit_public(1, bytes)?;
        next.default_collected = true;
        next.check()?;
        *self = next;
        Ok(default)
    }
    pub(crate) fn checkpoint_v1(&self) -> PalwOwnedPublicPrefixCheckpointV1 {
        PalwOwnedPublicPrefixCheckpointV1 {
            seed_id: self.seed_id(),
            inner: self.inner.checkpoint_v1(),
            pending: self.pending.clone(),
            sources_opened: self.sources_opened,
            public_moves_used: self.public_moves_used,
            public_carriers_used: self.public_carriers_used,
            public_carrier_bytes_used: self.public_carrier_bytes_used,
            default_collected: self.default_collected,
            collection_expired: self.collection_expired,
            claim_closed: self.claim_closed,
            history_count: self.history_count,
            history_head: self.history_head,
            history_material_until_daa: self.terms.material_until_daa,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwOwnedPublicPrefixCheckpointV1) -> Result<(), String> {
        if cp.seed_id != self.seed_id() || cp.history_material_until_daa != self.terms.material_until_daa {
            return Err("public prefix checkpoint changes immutable seed".into());
        }
        let mut next = self.clone();
        next.inner.restore_checkpoint_v1(&cp.inner)?;
        next.pending = cp.pending.clone();
        next.sources_opened = cp.sources_opened;
        next.public_moves_used = cp.public_moves_used;
        next.public_carriers_used = cp.public_carriers_used;
        next.public_carrier_bytes_used = cp.public_carrier_bytes_used;
        next.default_collected = cp.default_collected;
        next.collection_expired = cp.collection_expired;
        next.claim_closed = cp.claim_closed;
        next.history_count = cp.history_count;
        next.history_head = cp.history_head;
        next.check()?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }
    pub(crate) fn fixture() -> (PalwOwnedPublicPrefixCourtV1, Vec<crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1>) {
        fixture_for_owner(bond(7))
    }
    pub(crate) fn inner_fixture() -> PalwOwnedPrefixCourtV1 {
        fixture().0.inner
    }
    pub(crate) fn fixture_for_owner(
        owner: PalwBondKeyV2,
    ) -> (PalwOwnedPublicPrefixCourtV1, Vec<crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1>) {
        let (target, binding, job, tiles) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(true, false);
        let config = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1();
        let relation = target.registered_semantics.as_ref().unwrap().prefix_state_relation.unwrap();
        let base = crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1::derive(
            &binding.shape_profile,
            relation,
            2,
            binding.step_leaf_count,
            90_000,
            config.prices,
        )
        .unwrap();
        let plan = PalwPrefixPublicAcquisitionPlanV1::derive(
            base,
            7,
            config.prices,
            borsh::object_length(&binding.shape_profile).unwrap() as u64,
        )
        .unwrap();
        let terms = PalwRenderingClaimTermsV1::derive(100, plan.envelope, 100, 100, 20).unwrap();
        let funding = plan.envelope.rent_sompi.checked_add(plan.envelope.deposit_sompi).unwrap();
        let seed = Hash64::from_u64_word(777);
        let inner = PalwOwnedPrefixCourtV1::admit(
            seed,
            target.claim_id,
            owner,
            100,
            &binding,
            &job,
            &target,
            terms,
            config.prices,
            100,
            funding,
            90_000,
            1 << 32,
            10_000,
            terms.material_until_daa,
            terms.bond_exit_after_daa,
            &[terms.bond_exit_after_daa; 5],
        )
        .unwrap();
        let mut custody =
            PalwPrefixCustodyContractV1::new(target.execution_root, target.job_identity, binding.job_context.cu_ruleset_id, &terms);
        custody.expected_panel = (8..=12).map(bond).collect();
        assert!(!custody.expected_panel.contains(&target.executor_bond));
        for seat in 8..=12 {
            let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
                claim: target.claim_id,
                seat_bond: bond(seat),
                signed_daa: 100,
                verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
                signature: vec![seat as u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            assert!(custody.qualify(target.claim_id, &receipt, 100));
        }
        assert!(
            PalwOwnedPublicPrefixCourtV1::admit(
                inner.clone(),
                target.executor_bond,
                custody.clone(),
                plan.clone(),
                terms,
                1 << 32,
                funding - 1
            )
            .is_err()
        );
        let row = PalwOwnedPublicPrefixCourtV1::admit(inner, target.executor_bond, custody, plan, terms, 1 << 32, funding).unwrap();
        (row, tiles)
    }
    #[test]
    fn paid_public_prefix_keeps_hostile_slots_separate_and_owner_authentication_atomic() {
        let (mut row, tiles) = fixture();
        let initial = row.clone();
        let (claim, owner, reserved) = row.key();
        let seed = row.seed_id();
        let demand = row.next_demand_v1(100).unwrap().unwrap();
        let demand_id = Hash64::from_u64_word(99);
        row.demand_v1(100, claim, owner, reserved, seed, demand_id, &demand, 6000).unwrap();
        let cp = row.checkpoint_v1();
        let family_cp = row.inner.checkpoint_v1();
        let pending = row.clone();
        let tile = tiles.iter().find(|t| t.preimage.coord == demand.coordinate).unwrap();
        let mut wrong = tile.preimage.values_le.clone();
        wrong[0] ^= 1;
        let mut answer = PalwPrefixPublicAnswerV1 {
            answerer: row.producer,
            cursor_id: demand.cursor_id,
            demand_id,
            action: PalwPrefixPublicAnswerActionV1::Batch(vec![
                PalwPrefixPublicAnswerUnitV1::Begin { opening: tile.opening.clone() },
                PalwPrefixPublicAnswerUnitV1::Part { offset: 0, bytes: wrong },
                PalwPrefixPublicAnswerUnitV1::Finish,
            ]),
        };
        row.answer_v1(101, claim, owner, reserved, seed, &answer, 10_000).unwrap();
        assert_eq!(row.inner.checkpoint_v1(), family_cp);
        assert_eq!(row.pending_progress_v1().unwrap().signers.get(&row.producer), Some(&PalwPrefixPublicSignerCursorV1::Invalid));
        let before = row.clone();
        answer.answerer = bond(99);
        assert!(row.answer_v1(102, claim, owner, reserved, seed, &answer, 10_000).is_err());
        assert_eq!(row, before);
        answer.answerer = owner;
        answer.action = PalwPrefixPublicAnswerActionV1::Batch(vec![
            PalwPrefixPublicAnswerUnitV1::Begin { opening: tile.opening.clone() },
            PalwPrefixPublicAnswerUnitV1::Part { offset: 0, bytes: tile.preimage.values_le.clone() },
            PalwPrefixPublicAnswerUnitV1::Finish,
        ]);
        row.answer_v1(103, claim, owner, reserved, seed, &answer, 10_000).unwrap();
        assert!(row.pending_progress_v1().is_none());
        assert_ne!(row.inner.checkpoint_v1(), family_cp);
        assert!(row.joint_default_v1(demand.deadline_daa).unwrap().is_none());
        let restarted: PalwOwnedPublicPrefixCourtV1 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        restarted.check().unwrap();
        assert_eq!(restarted, row);
        row.restore_checkpoint_v1(&cp).unwrap();
        assert_eq!(row, pending);
        row.restore_checkpoint_v1(&initial.checkpoint_v1()).unwrap();
        assert_eq!(row, initial);
    }
    #[test]
    fn signed_demand_survives_inclusion_delay_but_never_shortens_or_extends_the_response_span() {
        let (row, _) = fixture();
        let (claim, owner, reserved) = row.key();
        let seed = row.seed_id();
        let span = row.plan.maximum_response_daa;
        let signed = row.next_demand_v1(100).unwrap().unwrap();
        assert!(signed.deadline_daa >= 100 + span);
        // Accepted one span later than the owner's snapshot: still a full span for every custodian.
        let late = 100 + (signed.deadline_daa - 100 - span);
        let mut accepted = row.clone();
        accepted.demand_v1(late, claim, owner, reserved, seed, Hash64::from_u64_word(7), &signed, 6000).unwrap();
        assert_eq!(accepted.pending_deadline_v1(), Some(signed.deadline_daa));
        // One block later the signed deadline would leave less than the span: refused unchanged.
        let mut too_late = row.clone();
        assert!(too_late.demand_v1(late + 1, claim, owner, reserved, seed, Hash64::from_u64_word(7), &signed, 6000).is_err());
        assert_eq!(too_late, row);
        // An owner cannot buy a longer collection clock than the derivation's own bound.
        let mut extended = signed.clone();
        extended.deadline_daa += 1;
        let mut refused = row.clone();
        assert!(refused.demand_v1(100, claim, owner, reserved, seed, Hash64::from_u64_word(7), &extended, 6000).is_err());
        // Without a full span before the stream horizon there is no demand, and that is not an error.
        assert!(row.next_demand_v1(row.terms.stream_until_daa - span + 1).unwrap().is_none());
    }
    #[test]
    fn joint_default_only_at_literal_deadline_once_with_saved_all_signed_seats() {
        let (mut row, _) = fixture();
        let (claim, owner, reserved) = row.key();
        let seed = row.seed_id();
        let demand = row.next_demand_v1(100).unwrap().unwrap();
        row.demand_v1(100, claim, owner, reserved, seed, Hash64::from_u64_word(99), &demand, 6000).unwrap();
        let before = row.clone();
        assert!(row.joint_default_v1(demand.deadline_daa - 1).unwrap().is_none());
        assert!(row.mark_default_collected_v1(demand.deadline_daa - 1).is_err());
        assert_eq!(row, before);
        let default = row.joint_default_v1(demand.deadline_daa).unwrap().unwrap();
        assert_eq!(default.panel, (8..=12).map(bond).collect());
        assert_eq!(default.owner, owner);
        row.mark_default_collected_v1(demand.deadline_daa).unwrap();
        assert!(row.joint_default_v1(demand.deadline_daa).unwrap().is_none());
        row.restore_checkpoint_v1(&before.checkpoint_v1()).unwrap();
        assert_eq!(row, before);
    }
}
