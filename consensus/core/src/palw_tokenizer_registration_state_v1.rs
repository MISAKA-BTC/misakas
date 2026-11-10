//! RFC-0051 accepted operands and reservation graph. Only the native fold
//! installs these rows; a decoded session is not evidence of a paid Open.
use crate::{
    Hash64,
    palw_court_service_v1::{ServicePacket, ServiceReservations, causal::CausalServicePacketV1},
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_state_v2::{PalwBlockContextV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1},
    palw_tokenizer_registration_v1::{
        TokenizerRegistrationFinishV1, TokenizerRegistrationOpenV1, TokenizerRegistrationPartV1, TokenizerRegistrationPolicyV1,
        TokenizerRegistrationQuoteV1,
    },
    palw_tokenizer_rendering_v2::{
        PalwRootedTokenizerRegistrationRowV1, PalwTokenizerRenderingBudgetV2, PalwVerifiedTokenizerRegistrationErrorV1,
        tokenizer_registration_parent_v1,
    },
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenizerRegistrationStatusV1 {
    Pending,
    Complete { point: PalwBlockContextV2, record: Hash64 },
    Superseded { point: PalwBlockContextV2 },
    Expired { point: PalwBlockContextV2 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationSessionV1 {
    pub(crate) open: TokenizerRegistrationOpenV1,
    pub(crate) policy: TokenizerRegistrationPolicyV1,
    pub(crate) quote: TokenizerRegistrationQuoteV1,
    pub(crate) accepted: PalwBlockContextV2,
    pub(crate) key_hash: Hash64,
    pub(crate) declared_key_events: u64,
    pub(crate) received: BTreeSet<u32>,
    pub(crate) finish_attempts: BTreeSet<u8>,
    pub(crate) status: TokenizerRegistrationStatusV1,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationPartRowV1 {
    pub(crate) part: TokenizerRegistrationPartV1,
    pub(crate) accepted: PalwBlockContextV2,
    pub(crate) material_until_daa: u64,
}

/// A separate capacity partition for the preclaim service. The public ruleset
/// must account this partition alongside court/ordinary block capacity. No
/// network delivery/default assertion is derived from this reservation book.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationBookV1 {
    pub(crate) policy: TokenizerRegistrationPolicyV1,
    pub(crate) book: ServiceReservations,
    pub(crate) sessions: BTreeMap<Hash64, TokenizerRegistrationReservationV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TokenizerRegistrationReservationV1 {
    pub(crate) opener: crate::palw_state_v2::PalwBondKeyV2,
    pub(crate) accepted_daa: u64,
    pub(crate) finish_until_daa: u64,
    pub(crate) material_until_daa: u64,
    pub(crate) quote: TokenizerRegistrationQuoteV1,
    /// Original DAA assignments: indexed Parts, then normal and recovery Finish.
    pub(crate) assigned_slots: Vec<u64>,
    pub(crate) released_at: Option<PalwBlockContextV2>,
}

fn follows(parent: PalwBlockContextV2, at: PalwBlockContextV2) -> bool {
    parent.block != at.block && parent.blue_score < at.blue_score && parent.daa_score <= at.daa_score
}

fn key_hash(key: &[u8]) -> Result<Hash64, String> {
    stream_digest(b"misaka-palw/tokenizer-registration/opener-key/v1", &key)
}

pub(crate) fn packet_id(open: Hash64, kind: u8, index: u32) -> Result<[u8; 64], String> {
    Ok(stream_digest(b"misaka-palw/tokenizer-registration/service/v1", &(open, kind, index))?.as_bytes())
}

impl TokenizerRegistrationSessionV1 {
    pub(crate) fn prepare(
        parent: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        at: PalwBlockContextV2,
        network: Hash64,
        policy: TokenizerRegistrationPolicyV1,
        open: TokenizerRegistrationOpenV1,
    ) -> Result<Self, String> {
        if !parent.last_point().is_some_and(|prior| follows(*prior, at)) || open.body.network != network {
            return Err("tokenizer Open changes its native network or accepted parent".into());
        }
        let b = &open.body;
        let declared_key_events =
            tokenizer_registration_parent_v1(parent, extras, at.daa_score, &b.pins, b.artifact_root, b.statement)?;
        let quote = policy.quote_v1(b, at.daa_score, declared_key_events)?;
        let key = eligible_key(parent, params, b.opener)?;
        open.verify_signature_v1(key)?;
        let row = Self {
            open,
            policy,
            quote,
            accepted: at,
            key_hash: key_hash(key)?,
            declared_key_events,
            received: BTreeSet::new(),
            finish_attempts: BTreeSet::new(),
            status: TokenizerRegistrationStatusV1::Pending,
        };
        row.check(row.id()?)?;
        Ok(row)
    }

    pub(crate) fn id(&self) -> Result<Hash64, String> {
        self.open.body.id_v1()
    }

    pub(crate) fn check(&self, id: Hash64) -> Result<(), String> {
        let b = &self.open.body;
        if self.id()? != id
            || self.key_hash == Hash64::default()
            || self.accepted.block == Hash64::default()
            || self.policy.quote_v1(b, self.accepted.daa_score, self.declared_key_events)? != self.quote
            || self.finish_attempts.iter().any(|slot| *slot > 1)
            || self.received.len() > self.quote.geometry.part_count as usize
            || self.received.iter().any(|&index| index >= self.quote.geometry.part_count)
        {
            return Err("invalid tokenizer registration session identity, quote or part set".into());
        }
        match self.status {
            TokenizerRegistrationStatusV1::Pending => {}
            TokenizerRegistrationStatusV1::Complete { point, record } => {
                if !follows(self.accepted, point)
                    || point.daa_score >= b.finish_until_daa
                    || record == Hash64::default()
                    || self.finish_attempts.is_empty()
                    || self.received.len() != self.quote.geometry.part_count as usize
                {
                    return Err("invalid tokenizer registration completion".into());
                }
            }
            TokenizerRegistrationStatusV1::Superseded { point } => {
                if (point != self.accepted && !follows(self.accepted, point)) || point.daa_score >= b.material_until_daa {
                    return Err("invalid superseded tokenizer registration".into());
                }
            }
            TokenizerRegistrationStatusV1::Expired { point } => {
                if !follows(self.accepted, point) || point.daa_score < b.finish_until_daa {
                    return Err("invalid expired tokenizer registration".into());
                }
            }
        }
        Ok(())
    }

    fn authority<'a>(
        &self,
        state: &'a PalwChainStateV2,
        params: &PalwStateParamsV2,
        at: PalwBlockContextV2,
    ) -> Result<&'a [u8], String> {
        self.check(self.id()?)?;
        if !matches!(self.status, TokenizerRegistrationStatusV1::Pending)
            || !follows(self.accepted, at)
            || at.daa_score >= self.open.body.finish_until_daa
        {
            return Err("tokenizer registration is closed or outside its original clock".into());
        }
        let key = eligible_key(state, params, self.open.body.opener)?;
        if key_hash(key)? != self.key_hash {
            return Err("tokenizer registration opener key changed".into());
        }
        Ok(key)
    }

    pub(crate) fn prepare_part(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        at: PalwBlockContextV2,
        part: TokenizerRegistrationPartV1,
        book: &TokenizerRegistrationBookV1,
    ) -> Result<TokenizerRegistrationPartRowV1, String> {
        let key = self.authority(state, params, at)?;
        part.check_open_v1(&self.open.body)?;
        part.verify_signature_v1(key)?;
        book.check_session(self)?;
        book.check_slot(self.id()?, 0, part.body.index, at.daa_score)?;
        Ok(TokenizerRegistrationPartRowV1 { part, accepted: at, material_until_daa: self.open.body.material_until_daa })
    }

    /// All operands come from the same accepted parent, not a candidate block's
    /// newly inserted parts. The returned record still needs the native writer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn derive_finish(
        &self,
        parent: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        at: PalwBlockContextV2,
        finish: &TokenizerRegistrationFinishV1,
        book: &TokenizerRegistrationBookV1,
        parts: &[&TokenizerRegistrationPartRowV1],
    ) -> Result<
        (u8, Result<PalwRootedTokenizerRegistrationRowV1, PalwVerifiedTokenizerRegistrationErrorV1>),
        PalwVerifiedTokenizerRegistrationErrorV1,
    > {
        use PalwVerifiedTokenizerRegistrationErrorV1::Evidence as E;
        let key = self.authority(parent, params, at).map_err(E)?;
        finish.check_open_v1(&self.open.body, &self.policy).map_err(E)?;
        finish.verify_signature_v1(key).map_err(E)?;
        book.check_session(self).map_err(E)?;
        let id = self.id().map_err(E)?;
        let slot = (0u8..2)
            .find(|slot| book.check_slot(id, 1, u32::from(*slot), at.daa_score).is_ok())
            .ok_or_else(|| E("tokenizer Finish is outside both originally reserved slots".into()))?;
        if self.finish_attempts.contains(&slot) {
            return Err(E("tokenizer Finish service slot has already been consumed".into()));
        }
        let prior = parent.last_point().ok_or_else(|| E("tokenizer Finish has no accepted parent".into()))?;
        if !follows(*prior, at) || parts.len() != self.quote.geometry.part_count as usize || self.received.len() != parts.len() {
            return Err(E("tokenizer Finish requires all parts in the preceding accepted parent".into()));
        }
        let bytes = self.open.body.statement.exact_raw_bytes;
        let _raw_lease =
            kaspa_core::memory_budget::global_memory_budget().try_reserve(kaspa_core::memory_budget::MemoryClass::Native, bytes)?;
        let mut raw = Vec::new();
        raw.try_reserve_exact(bytes as usize).map_err(|error| {
            PalwVerifiedTokenizerRegistrationErrorV1::Capacity(kaspa_core::memory_budget::MemoryBudgetError {
                class: kaspa_core::memory_budget::MemoryClass::Native,
                requested_bytes: bytes,
                available_bytes: 0,
                external_reason: Some(format!("tokenizer raw allocation failed: {error}")),
            })
        })?;
        for (index, part) in parts.iter().enumerate() {
            part.check(self, index as u32).map_err(E)?;
            if !follows(part.accepted, at)
                || part.accepted.blue_score > prior.blue_score
                || part.accepted.daa_score > prior.daa_score
                || (part.accepted.blue_score == prior.blue_score && part.accepted != *prior)
                || !self.received.contains(&(index as u32))
            {
                return Err(E("tokenizer Finish contains a non-parent or missing part".into()));
            }
            raw.extend_from_slice(&part.part.bytes);
        }
        let p = self.policy;
        let result = PalwRootedTokenizerRegistrationRowV1::derive_budgeted_v1(
            parent,
            extras,
            at,
            self.open.body.pins,
            &raw,
            &finish.eog,
            PalwTokenizerRenderingBudgetV2 {
                raw_bytes: p.max_raw_bytes,
                vocab: p.max_vocab,
                key_events: p.max_key_events,
                eog_ids: p.max_eog_ids,
                total_rendering_bytes: p.max_rendering_bytes,
            },
        );
        Ok((slot, result))
    }
}

fn eligible_key<'a>(
    state: &'a PalwChainStateV2,
    params: &PalwStateParamsV2,
    opener: crate::palw_state_v2::PalwBondKeyV2,
) -> Result<&'a [u8], String> {
    let bond = state.bond(&opener).ok_or("tokenizer opener bond is absent")?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < params.min_collateral_sompi() {
        return Err("tokenizer opener is not an eligible active bond".into());
    }
    Ok(&bond.pubkey)
}

impl TokenizerRegistrationPartRowV1 {
    pub(crate) fn check_stored(&self, open: Hash64, index: u32) -> Result<(), String> {
        self.part.signing_message_v1()?;
        if self.part.body.open_id != open
            || self.part.body.index != index
            || self.accepted.block == Hash64::default()
            || self.accepted.daa_score >= self.material_until_daa
        {
            return Err("invalid tokenizer part storage identity or retention".into());
        }
        Ok(())
    }
    pub(crate) fn check(&self, session: &TokenizerRegistrationSessionV1, index: u32) -> Result<(), String> {
        self.part.check_open_v1(&session.open.body)?;
        if self.part.body.index != index
            || self.material_until_daa != session.open.body.material_until_daa
            || !follows(session.accepted, self.accepted)
            || self.accepted.daa_score >= session.open.body.finish_until_daa
        {
            return Err("tokenizer retained part differs from its original scope or clock".into());
        }
        Ok(())
    }
}

impl TokenizerRegistrationReservationV1 {
    fn assigned_slot(&self, kind: u8, index: u32) -> Option<u64> {
        let parts = self.quote.geometry.part_count;
        let position = match kind {
            0 if index < parts => index,
            1 if index < 2 => parts.checked_add(index)?,
            _ => return None,
        };
        self.assigned_slots.get(position as usize).copied()
    }

    fn check_history(&self) -> Result<(), String> {
        use crate::palw_tokenizer_registration_v1::TokenizerRegistrationGeometryV1;
        let geometry = self.quote.geometry;
        if TokenizerRegistrationGeometryV1::new(geometry.raw_bytes)? != geometry
            || self.assigned_slots.len() != geometry.part_count as usize + 2
            || self.finish_until_daa >= self.material_until_daa
            || self.assigned_slots.iter().any(|&daa| daa <= self.accepted_daa || daa >= self.finish_until_daa)
            || self.released_at.is_some_and(|point| {
                point.block == Hash64::default() || point.daa_score < self.accepted_daa || point.daa_score >= self.material_until_daa
            })
        {
            return Err("invalid tokenizer immutable slot history or release point".into());
        }
        let split = geometry.part_count as usize;
        let (parts, finish) = self.assigned_slots.split_at(split);
        if parts.iter().any(|&daa| daa >= finish[0]) || finish[0] >= finish[1] {
            return Err("tokenizer slot history violates Part/Finish causal order".into());
        }
        Ok(())
    }

    fn graph(&self, open: Hash64) -> Result<Vec<CausalServicePacketV1>, String> {
        let release = self.accepted_daa.checked_add(1).ok_or("tokenizer slot clock overflow")?;
        let mut graph = Vec::new();
        let mut part_ids = Vec::new();
        for index in 0..self.quote.geometry.part_count {
            let id = packet_id(open, 0, index)?;
            part_ids.push(id);
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket { id, release_daa: release, service_before: self.finish_until_daa, cost: self.quote.part_cost },
                predecessors: vec![],
            });
        }
        for index in 0..2 {
            let mut predecessors = part_ids.clone();
            if index == 1 {
                predecessors.push(packet_id(open, 1, 0)?);
            }
            graph.push(CausalServicePacketV1 {
                packet: ServicePacket {
                    id: packet_id(open, 1, index)?,
                    release_daa: release,
                    service_before: self.finish_until_daa,
                    cost: self.quote.finish_cost,
                },
                predecessors,
            });
        }
        Ok(graph)
    }
}

impl TokenizerRegistrationBookV1 {
    pub(crate) fn new(policy: TokenizerRegistrationPolicyV1) -> Result<Self, String> {
        let budget = policy.effective_reservation_budget_v1()?;
        Ok(Self {
            policy,
            book: ServiceReservations::new(budget).map_err(|e| format!("tokenizer service: {e:?}"))?,
            sessions: BTreeMap::new(),
        })
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        let budget = self.policy.effective_reservation_budget_v1()?;
        self.book.check()?;
        if self.book.budget != budget
            || !self.book.supply.is_empty()
            || self.book.last_observed_daa.is_some()
            || self.sessions.len() > self.policy.max_pending_total as usize
        {
            return Err("invalid tokenizer reservation book policy or scope".into());
        }
        let mut retained = 0u64;
        let mut owners = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for (id, reservation) in &self.sessions {
            retained = retained.checked_add(reservation.quote.retained_bytes).ok_or("tokenizer retention overflow")?;
            let count = owners.entry(reservation.opener).or_insert(0u32);
            *count = count.checked_add(1).ok_or("tokenizer opener count overflow")?;
            if *count > self.policy.max_pending_per_opener {
                return Err("tokenizer opener reservation quota exceeded".into());
            }
            reservation.check_history()?;
            if reservation.released_at.is_some() {
                let parts = (0..reservation.quote.geometry.part_count).map(|index| (0, index));
                for (kind, index) in parts.chain([(1, 0), (1, 1)]) {
                    let packet = packet_id(*id, kind, index)?;
                    if self.book.packets.contains_key(&packet) || self.book.assignments.contains_key(&packet) {
                        return Err("released tokenizer reservation still occupies active capacity".into());
                    }
                }
            } else {
                let graph = reservation.graph(*id)?;
                self.book.check_causal_v1(&graph).map_err(|e| format!("tokenizer causal reservation: {e:?}"))?;
                for (node, daa) in graph.iter().zip(&reservation.assigned_slots) {
                    if self.book.assigned_daa(&node.packet.id) != Some(*daa) || !ids.insert(node.packet.id) {
                        return Err("active tokenizer reservation changes immutable slot history".into());
                    }
                }
            }
        }
        if retained > self.policy.max_retained_total_bytes
            || ids.len() != self.book.packets.len()
            || self.book.packets.keys().any(|id| !ids.contains(id))
        {
            return Err("tokenizer book retains unowned packets or excessive storage".into());
        }
        Ok(())
    }

    pub(crate) fn reserve(&mut self, row: &TokenizerRegistrationSessionV1) -> Result<(), String> {
        self.check()?;
        let id = row.id()?;
        row.check(id)?;
        if row.policy != self.policy || self.sessions.contains_key(&id) {
            return Err("tokenizer Open changes policy or repeats a live reservation".into());
        }
        let mut reservation = TokenizerRegistrationReservationV1 {
            opener: row.open.body.opener,
            accepted_daa: row.accepted.daa_score,
            finish_until_daa: row.open.body.finish_until_daa,
            material_until_daa: row.open.body.material_until_daa,
            quote: row.quote,
            assigned_slots: Vec::new(),
            released_at: None,
        };
        let graph = reservation.graph(id)?;
        let mut next = self.clone();
        next.book
            .reserve_causal_v1(&graph, reservation.finish_until_daa)
            .map_err(|e| format!("tokenizer registration capacity: {e:?}"))?;
        reservation.assigned_slots = graph
            .iter()
            .map(|node| next.book.assigned_daa(&node.packet.id).ok_or("tokenizer slot was not assigned"))
            .collect::<Result<_, _>>()?;
        next.sessions.insert(id, reservation);
        next.check()?;
        *self = next;
        Ok(())
    }

    /// Release executable capacity but retain the immutable paid slots and raw
    /// retention charge until GC. Other sessions retain their original slots.
    pub(crate) fn release(&mut self, id: Hash64, point: PalwBlockContextV2) -> Result<(), String> {
        self.check()?;
        let reservation = self.sessions.get(&id).ok_or("missing tokenizer release reservation")?;
        if reservation.released_at.is_some()
            || point.daa_score < reservation.accepted_daa
            || point.daa_score >= reservation.material_until_daa
            || point.block == Hash64::default()
        {
            return Err("invalid tokenizer capacity release".into());
        }
        let mut next = self.clone();
        next.detach_active_slots(id, reservation)?;
        next.sessions.get_mut(&id).expect("cloned reservation exists").released_at = Some(point);
        next.check()?;
        *self = next;
        Ok(())
    }

    pub(crate) fn expire(&mut self, id: Hash64, daa: u64) -> Result<(), String> {
        self.check()?;
        let reservation = self.sessions.get(&id).ok_or("missing tokenizer expiration reservation")?;
        if daa < reservation.material_until_daa {
            return Err("premature tokenizer material expiry".into());
        }
        let mut next = self.clone();
        if reservation.released_at.is_none() {
            next.detach_active_slots(id, reservation)?;
        }
        next.sessions.remove(&id);
        next.check()?;
        *self = next;
        Ok(())
    }

    fn detach_active_slots(&mut self, id: Hash64, reservation: &TokenizerRegistrationReservationV1) -> Result<(), String> {
        let parts = (0..reservation.quote.geometry.part_count).map(|index| (0, index));
        for (kind, index) in parts.chain([(1, 0), (1, 1)]) {
            let packet = packet_id(id, kind, index)?;
            self.book.packets.remove(&packet);
            self.book.assignments.remove(&packet);
        }
        self.book.remaining.clear();
        for (packet_id, packet) in &self.book.packets {
            let daa = *self.book.assignments.get(packet_id).ok_or("missing tokenizer service assignment")?;
            let left = self.book.remaining.entry(daa).or_insert(self.book.budget);
            left.bytes = left.bytes.checked_sub(packet.cost.bytes).ok_or("tokenizer byte overbooking")?;
            left.work = left.work.checked_sub(packet.cost.work).ok_or("tokenizer work overbooking")?;
            left.io = left.io.checked_sub(packet.cost.io).ok_or("tokenizer I/O overbooking")?;
            left.controls = left.controls.checked_sub(packet.cost.controls).ok_or("tokenizer control overbooking")?;
        }
        Ok(())
    }

    pub(crate) fn assigned_slot(&self, open: Hash64, kind: u8, index: u32) -> Option<u64> {
        self.sessions.get(&open)?.assigned_slot(kind, index)
    }

    pub(crate) fn check_session(&self, row: &TokenizerRegistrationSessionV1) -> Result<(), String> {
        let id = row.id()?;
        row.check(id)?;
        let r = self.sessions.get(&id).ok_or("tokenizer session has no native reservation")?;
        if r.released_at.is_some()
            || self.policy != row.policy
            || r.opener != row.open.body.opener
            || r.accepted_daa != row.accepted.daa_score
            || r.finish_until_daa != row.open.body.finish_until_daa
            || r.material_until_daa != row.open.body.material_until_daa
            || r.quote != row.quote
        {
            return Err("tokenizer reservation differs from accepted Open".into());
        }
        Ok(())
    }

    fn check_slot(&self, open: Hash64, kind: u8, index: u32, daa: u64) -> Result<(), String> {
        let reservation = self.sessions.get(&open).ok_or("tokenizer action has no service reservation")?;
        if reservation.released_at.is_some()
            || reservation.assigned_slot(kind, index) != Some(daa)
            || self.book.assigned_daa(&packet_id(open, kind, index)?) != Some(daa)
        {
            return Err("tokenizer action is outside its paid service slot".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tokenizer_registration_v1::{TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1, TokenizerRegistrationFinishBodyV1};

    fn session() -> TokenizerRegistrationSessionV1 {
        let h = Hash64::from_u64_word;
        let (policy, body) = crate::palw_tokenizer_registration_v1::tests::fixture();
        let accepted = PalwBlockContextV2 { block: h(90), daa_score: 20, blue_score: 20, subsidy: 0 };
        TokenizerRegistrationSessionV1 {
            open: TokenizerRegistrationOpenV1 { body, signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] },
            policy,
            quote: policy.quote_v1(&body, accepted.daa_score, 10_000).unwrap(),
            accepted,
            key_hash: h(91),
            declared_key_events: 10_000,
            received: BTreeSet::new(),
            finish_attempts: BTreeSet::new(),
            status: TokenizerRegistrationStatusV1::Pending,
        }
    }

    #[test]
    fn reservation_rejects_corruption_and_expiry_preserves_other_slots() {
        let h = Hash64::from_u64_word;
        let first = session();
        let mut later = first.clone();
        later.open.body.nonce = h(101);
        later.open.body.finish_until_daa += 10;
        later.open.body.material_until_daa += 10;
        later.quote = later.policy.quote_v1(&later.open.body, later.accepted.daa_score, later.declared_key_events).unwrap();
        let first_id = first.id().unwrap();
        let later_id = later.id().unwrap();
        let mut book = TokenizerRegistrationBookV1::new(first.policy).unwrap();
        assert_eq!(book.book.budget.work + first.policy.bookkeeping_work_v1().unwrap().sweep, first.policy.service_budget.work);
        let mut corrupt = book.clone();
        corrupt.book.budget = first.policy.service_budget;
        assert!(corrupt.check().is_err());
        book.reserve(&first).unwrap();
        book.reserve(&later).unwrap();
        book.check_session(&later).unwrap();
        let original = book.clone();
        assert!(book.reserve(&first).is_err());
        assert!(book.expire(first_id, first.open.body.material_until_daa - 1).is_err());
        assert_eq!(book, original);

        let mut corrupt = original.clone();
        corrupt.sessions.remove(&first_id);
        assert!(corrupt.check().is_err());
        let mut corrupt = original.clone();
        corrupt.sessions.get_mut(&first_id).unwrap().quote.charge += 1;
        assert!(corrupt.check_session(&first).is_err());
        let other_finish = packet_id(later_id, 1, 1).unwrap();
        let normal = book.assigned_slot(first_id, 1, 0).unwrap();
        let release = PalwBlockContextV2 { block: h(102), daa_score: normal, blue_score: 50, subsidy: 0 };
        book.release(first_id, release).unwrap();
        assert_eq!(book.assigned_slot(first_id, 1, 0), Some(normal));
        assert!(book.book.assigned_daa(&packet_id(first_id, 1, 0).unwrap()).is_none());
        assert!(book.check_slot(first_id, 1, 0, normal).is_err());
        assert_eq!(book.book.assigned_daa(&other_finish), original.book.assigned_daa(&other_finish));
        let released = book.clone();
        assert!(book.release(first_id, release).is_err());
        assert_eq!(book, released);
        book.expire(first_id, first.open.body.material_until_daa).unwrap();
        assert_eq!(book.book.assigned_daa(&other_finish), original.book.assigned_daa(&other_finish));
        assert!(book.book.assigned_daa(&packet_id(first_id, 1, 0).unwrap()).is_none());
        assert!(book.check_session(&first).is_err());
        assert!(book.assigned_slot(first_id, 1, 0).is_none());
        book.check_session(&later).unwrap();
    }

    #[test]
    fn signed_finish_cannot_reuse_an_attempt_consumed_by_a_failed_parse() {
        let h = Hash64::from_u64_word;
        let mut session = session();
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([121; 32]);
        let pubkey = key.verification_key.as_ref().to_vec();
        session.key_hash = key_hash(&pubkey).unwrap();
        let table = crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries: vec![Some(vec![b'a']), Some(vec![b'b'])],
            eog_token_ids: vec![1],
            tokenizer_id: session.open.body.statement.tokenizer_id,
        };
        let eog = table.eog_opening().unwrap();
        session.open.body.statement.vocab = table.vocab();
        session.open.body.statement.table_root = table.root();
        session.open.body.eog_digest = TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap();
        session.quote = session.policy.quote_v1(&session.open.body, session.accepted.daa_score, session.declared_key_events).unwrap();
        let id = session.id().unwrap();
        let mut book = TokenizerRegistrationBookV1::new(session.policy).unwrap();
        book.reserve(&session).unwrap();
        let normal_daa = book.book.assigned_daa(&packet_id(id, 1, 0).unwrap()).unwrap();
        let recovery_daa = book.book.assigned_daa(&packet_id(id, 1, 1).unwrap()).unwrap();
        assert!(normal_daa < recovery_daa);
        let mut finish = TokenizerRegistrationFinishV1 {
            body: TokenizerRegistrationFinishBodyV1 {
                version: 1,
                network: session.open.body.network,
                policy: session.open.body.policy,
                open_id: id,
                opener: session.open.body.opener,
                eog_digest: session.open.body.eog_digest,
                finish_until_daa: session.open.body.finish_until_daa,
            },
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            eog,
        };
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            finish.signing_message_v1().unwrap().as_byte_slice(),
            TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1,
            [122; 32],
        )
        .unwrap();
        finish.signature.copy_from_slice(signature.as_ref());
        let params = PalwStateParamsV2::new(100, 1, 1, 1, 1, 1, h(102), 2, 1000, 100, 1, 1).unwrap();
        let mut state = PalwChainStateV2::genesis();
        state.set_last_point_for_tests(Some(PalwBlockContextV2 { block: h(103), daa_score: normal_daa, blue_score: 100, subsidy: 0 }));
        state.set_bond_for_tests(
            session.open.body.opener,
            Some(crate::palw_state_v2::PalwBondStateV2 {
                pubkey,
                operator_id: h(104),
                collateral: 100,
                slashed: 0,
                status: PalwBondStatusV2::Active,
                registered_daa: 1,
                payout_payload: h(105),
                capable_classes: BTreeSet::new(),
            }),
        );
        session.received.extend(0..session.quote.geometry.part_count);
        session.finish_attempts.insert(0); // persisted by an earlier signed, failed parser invocation
        let replay = |daa_score| {
            let at = PalwBlockContextV2 { block: h(106), daa_score, blue_score: 101, subsidy: 0 };
            session
                .derive_finish(&state, &params, &PalwTransitionExtrasV1::default(), at, &finish, &book, &[])
                .unwrap_err()
                .to_string()
        };
        assert!(replay(normal_daa).contains("already been consumed"));
        assert!(replay(recovery_daa).contains("all parts"));
    }
}
