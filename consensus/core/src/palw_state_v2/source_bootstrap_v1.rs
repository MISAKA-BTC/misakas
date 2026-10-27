//! Native journal for a finite first-source epoch. Node-provided own-chain
//! context is distinct from carrier statements. No RPC bytes establish stake,
//! anchor, settlement or quorum authority here.
use super::*;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, retain_native_value_decode_classified_v1,
};
use crate::palw_source_bootstrap_v1::*;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::bootstrap_epoch_v1::BootstrapSeatV1;

fn refuse(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(s.into())
}
// Retained readers may report either Borsh IO errors or typed provenance text.
// Preserve resource exhaustion as Resource independently of the decode payload.
fn decode_error<E: std::fmt::Display>(e: NativeDecodeErrorV1<E>) -> PalwStateV2Error {
    match e {
        NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        NativeDecodeErrorV1::Decode(e) => refuse(e.to_string()),
    }
}
fn raw<'a>(state: &'a PalwChainStateV2, k: &[u8]) -> Result<Option<&'a [u8]>, PalwStateV2Error> {
    let Some(route) = state.kernel_route() else { return Ok(None) };
    let _key = crate::palw_native_resource_v1::reserve_native_requests_v1(k.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let mut owned = Vec::new();
    owned.try_reserve_exact(k.len()).map_err(|why| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Vec<u8>>("source fixed key", &why.to_string()))
    })?;
    owned.extend_from_slice(k);
    Ok(route.aux.get(&(PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1, owned)).map(Vec::as_slice))
}
fn key(prefix: u8, id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = prefix;
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn read<T: crate::palw_native_resource_v1::NativeWireScanV1 + BorshDeserialize>(
    state: &PalwChainStateV2,
    k: &[u8],
) -> Result<Option<NativeRetainedV1<T>>, PalwStateV2Error> {
    raw(state, k)?.map(|b| retain_native_value_decode_classified_v1(b, || borsh::from_slice(b)).map_err(decode_error)).transpose()
}

impl PalwChainStateV2 {
    pub(crate) fn paid_source_event_for_route_v6(
        route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
        epoch: &PalwSourceBootstrapEpochRowV1,
        id: Hash64,
        now: u64,
        consumer_liability_until: u64,
        liability_slack: u64,
    ) -> Result<Option<misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1>, PalwStateV2Error> {
        current_paid_source_event_v6(route, ledger, epoch, id, now, consumer_liability_until, liability_slack)
    }
    pub fn source_bootstrap_check_epoch_authority_v1(&self) -> Result<(), PalwStateV2Error> {
        let epoch = self.source_bootstrap_epoch_v1()?.ok_or_else(|| refuse("bootstrap own epoch absent"))?;
        validate_cohort(&epoch)
    }

    /// A prospective source statement comes from the actual own native claim.
    /// It opens no journal, grants no approval and transfers no credit.
    pub fn source_bootstrap_prepare_work_v1(&self, claim: Hash64) -> Result<PalwSourceBootstrapWorkV1, PalwStateV2Error> {
        let epoch = self.source_bootstrap_epoch_v1()?.ok_or_else(|| refuse("bootstrap own epoch absent"))?;
        validate_cohort(&epoch)?;
        if epoch.draw_refused {
            return Err(refuse("bootstrap fixed epoch is unavailable"));
        }
        let ledger = self.source_bootstrap_checker_ledger_v1()?;
        derive_native_work(&epoch, &ledger, claim)
    }
    /// Revalidate the checker subject against this same native ledger, not
    /// just the decoded journal's shape. Approval bytes alone are insufficient.
    pub fn source_bootstrap_check_work_authority_v1(&self, id: Hash64) -> Result<(), PalwStateV2Error> {
        let epoch = self.source_bootstrap_epoch_v1()?.ok_or_else(|| refuse("bootstrap checker epoch absent"))?;
        validate_cohort(&epoch)?;
        let row = self.source_bootstrap_work_v1(id)?.ok_or_else(|| refuse("bootstrap checker work absent"))?;
        validate_work(&epoch, &row)?;
        if let Some(route) = self.kernel_route() {
            crate::palw_source_verifier_economics_v2::validate_source_economic_provenance_v2(route, &epoch, &row)
                .map_err(decode_error)?;
        }
        if row.work.id() != id {
            return Err(refuse("bootstrap checker work key differs from statement"));
        }
        let ledger = self.source_bootstrap_checker_ledger_v1()?;
        if derive_native_work(&epoch, &ledger, row.work.claim)? != row.work {
            return Err(refuse("bootstrap checker statement differs from its native claim/material"));
        }
        Ok(())
    }

    /// Own native projection for a real downstream claim, with unresolved
    /// transitive lineage kept unavailable. This never returns a seed or a
    /// Locked collector result, even when the source itself is approved.
    pub fn source_bootstrap_consumer_observation_v1(
        &self,
        source_work: Hash64,
        consumer_claim: Hash64,
    ) -> Result<crate::palw_source_bootstrap_v1::PalwSourceBootstrapConsumerObservationV1, PalwStateV2Error> {
        use crate::palw_source_bootstrap_v1::{
            PalwSourceBootstrapConsumerObservationV1, PalwSourceBootstrapConsumerUnavailableV1 as U,
        };
        let source = self.source_bootstrap_event_v1(source_work)?;
        let ledger = self.source_bootstrap_checker_ledger_v1()?;
        let exists = self.claim(&consumer_claim).is_some() || ledger.claims.contains_key(&consumer_claim.as_bytes());
        Ok(PalwSourceBootstrapConsumerObservationV1 {
            source,
            unavailable: if exists { U::VersionedLineageUnimplemented } else { U::NativeConsumerAbsent },
        })
    }

    /// Core-owned operational ledger projection for the selected source worker.
    /// Callers must obtain this state from their own coherent native getter;
    /// this method does not authenticate an externally supplied snapshot.
    pub fn source_bootstrap_checker_ledger_v1(
        &self,
    ) -> Result<crate::palw_native_resource_v1::ReservedKernelLedgerV1, PalwStateV2Error> {
        self.kernel_route()
            .ok_or_else(|| refuse("bootstrap checker kernel route absent"))?
            .ledger_native_projection_checked_v9()
            .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refuse))
    }

    /// Fixed projection from own accepted rows. Remote event bytes cannot call
    /// this method in place of authenticating the source native state.
    pub fn source_bootstrap_event_v1(
        &self,
        id: Hash64,
    ) -> Result<Option<misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1>, PalwStateV2Error> {
        let Some(epoch) = self.source_bootstrap_epoch_v1()? else { return Ok(None) };
        let Some(row) = self.source_bootstrap_work_v1(id)? else { return Ok(None) };
        validate_cohort(&epoch)?;
        validate_work(&epoch, &row)?;
        if let Some(route) = self.kernel_route() {
            crate::palw_source_verifier_economics_v2::validate_source_economic_provenance_v2(route, &epoch, &row)
                .map_err(decode_error)?;
        }
        let Some((final_position, settlement_position)) = row.settlement else { return Ok(None) };
        let Some((anchor, _)) = epoch.context.anchor else { return Ok(None) };
        Ok(Some(misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1 {
            version: 1,
            network: epoch.context.policy.network.as_bytes(),
            bootstrap_epoch: row.work.epoch.as_bytes(),
            bootstrap_policy: row.work.policy.as_bytes(),
            frozen_population: epoch.context.population_root().map_err(refuse)?,
            frozen_block: epoch.context.frozen_block.as_bytes(),
            anchor: anchor.as_bytes(),
            anchor_position: epoch.context.policy.anchor_cutoff_daa,
            source_profile: row.work.source_profile.as_bytes(),
            checker_scope: row.work.checker_scope.as_bytes(),
            claim: row.work.claim.as_bytes(),
            canonical_work: row.work.canonical_work.as_bytes(),
            statement: row.work.id().as_bytes(),
            material_scope: row.work.material_scope.as_bytes(),
            execution_commitment: row.work.execution_commitment.as_bytes(),
            approval_root: hash_serialized(b"MISAKA/PALW/SOURCE-APPROVAL-ROOT/V1", &row.approvals),
            producer: row.work.producer.as_bytes(),
            consumer: row.work.consumer.as_bytes(),
            accepted_position: row.work.accepted_daa,
            final_position,
            settlement_position,
            approved_slots: row.approvals.len() as u16,
            required_slots: epoch.context.policy.selection.quorum,
        }))
    }
    pub fn source_bootstrap_epoch_v1(&self) -> Result<Option<NativeRetainedV1<PalwSourceBootstrapEpochRowV1>>, PalwStateV2Error> {
        read(self, &[0])
    }
    pub fn source_bootstrap_work_v1(
        &self,
        id: Hash64,
    ) -> Result<Option<NativeRetainedV1<PalwSourceBootstrapWorkRowV1>>, PalwStateV2Error> {
        read(self, &key(1, id))
    }
    /// A missing context does not unlock collateral during the configured
    /// freeze/approval interval. In particular local pruning/resource refusal
    /// cannot create an unbonding race before the journal has been installed.
    pub fn source_bootstrap_bond_exit_held_v1(&self, params: &PalwStateParamsV2, now: u64) -> bool {
        params
            .selected_source_bootstrap_v1
            .as_ref()
            .is_some_and(|(at, p)| now >= *at && now > p.freeze_cutoff_daa && now <= p.stake_release_daa)
    }
    pub(crate) fn assert_source_bootstrap_consistency_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let epoch = self.source_bootstrap_epoch_v1()?;
        if let Some(e) = &epoch {
            e.context.validate_shape().map_err(refuse)?;
            if params.selected_source_bootstrap_v1.as_ref().is_none_or(|(_, p)| *p != e.context.policy) {
                return Err(refuse("bootstrap restored policy is not the native configured policy"));
            }
            validate_cohort(e)?;
            if e.source_works > e.context.policy.selection.maximum_source_works {
                return Err(refuse("bootstrap work count exceeds cap"));
            }
        }
        let mut works = 0u32;
        for ((t, k), _) in &route.aux {
            if *t != PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1 {
                continue;
            }
            if k.as_slice() == [0] {
                continue;
            }
            let e = epoch.as_ref().ok_or_else(|| refuse("bootstrap journal rows lack epoch"))?;
            if k.len() != 65 {
                return Err(refuse("unknown bootstrap row key"));
            }
            let id = Hash64::from_bytes(k[1..].try_into().expect("64-byte key"));
            if k[0] == 1 {
                let row = self.source_bootstrap_work_v1(id)?.ok_or_else(|| refuse("bootstrap work vanished"))?;
                validate_work(e, &row)?;
                crate::palw_source_verifier_economics_v2::validate_source_economic_provenance_v2(route, e, &row)
                    .map_err(decode_error)?;
                if row.work.id() != id || raw(self, &key(2, row.work.canonical_work))? != Some(id.as_byte_slice()) {
                    return Err(refuse("bootstrap work identity/canonical-work index mismatch"));
                }
                works = works.checked_add(1).ok_or(PalwStateV2Error::Overflow("bootstrap work count"))?;
            } else if k[0] == 2 {
                let b = raw(self, k)?.ok_or_else(|| refuse("bootstrap canonical index vanished"))?;
                if b.len() != 64 {
                    return Err(refuse("bootstrap canonical-work index size"));
                }
                let work = Hash64::from_bytes(b.try_into().expect("64 bytes"));
                let row = self.source_bootstrap_work_v1(work)?.ok_or_else(|| refuse("bootstrap canonical index orphan"))?;
                if row.work.canonical_work != id {
                    return Err(refuse("bootstrap canonical index points to another work"));
                }
            } else {
                return Err(refuse("unknown bootstrap row family"));
            }
        }
        if epoch.as_ref().is_some_and(|e| e.source_works != works) {
            return Err(refuse("bootstrap work count does not match actual journal"));
        }
        Ok(())
    }
}

fn select_seat(context: &PalwSourceBootstrapOwnContextV1, slot: u16) -> Result<BootstrapSeatV1, PalwStateV2Error> {
    let epoch = context.epoch().map_err(refuse)?;
    epoch.validate(&context.policy.selection).map_err(|_| refuse("bootstrap epoch contract"))?;
    if slot >= context.policy.selection.seats {
        return Err(refuse("bootstrap slot past policy"));
    }
    let total = context
        .population
        .iter()
        .try_fold(0u128, |n, m| n.checked_add(u128::from(m.locked_weight)))
        .ok_or(PalwStateV2Error::Overflow("bootstrap frozen locked weight"))?;
    if total == 0 {
        return Err(refuse("bootstrap frozen population is empty"));
    }
    let id = epoch.id().map_err(|_| refuse("bootstrap epoch identity"))?;
    let threshold = total.wrapping_neg() % total;
    for attempt in 0..context.policy.selection.maximum_draw_attempts {
        let mut input = [0; 68];
        input[..64].copy_from_slice(&id);
        input[64..66].copy_from_slice(&slot.to_le_bytes());
        input[66..].copy_from_slice(&attempt.to_le_bytes());
        let digest = misaka_palw_challenge::hash::h(b"MISAKA/PALW/BOOTSTRAP-TICKET/V1", &input);
        let ticket = u128::from_le_bytes(digest[..16].try_into().expect("16 bytes"));
        if ticket < threshold {
            continue;
        }
        let mut remaining = ticket % total;
        for (i, m) in context.population.iter().enumerate() {
            if remaining < u128::from(m.locked_weight) {
                return Ok(BootstrapSeatV1 { candidate_index: i as u32, bond: m.candidate().bond, locked_weight: m.locked_weight });
            }
            remaining -= u128::from(m.locked_weight);
        }
        return Err(refuse("bootstrap ticket exceeds native frozen population"));
    }
    Err(refuse("fixed bootstrap draw exhausted its rejection budget; no anchor fallback"))
}
fn validate_cohort(e: &PalwSourceBootstrapEpochRowV1) -> Result<(), PalwStateV2Error> {
    if e.context.anchor.is_none() {
        if !e.cohort.is_empty()
            || e.source_works != 0
            || e.draw_refused != (e.context.population_refused || e.context.population.is_empty())
        {
            return Err(refuse("unanchored bootstrap epoch has a cohort/work or an unexplained refusal"));
        }
    } else if e.draw_refused {
        if !e.cohort.is_empty()
            || e.source_works != 0
            || (0..e.context.policy.selection.seats).all(|slot| select_seat(&e.context, slot).is_ok())
        {
            return Err(refuse("bootstrap refused draw does not match its fixed anchor/rejection unavailability"));
        }
    } else {
        if e.cohort.len() != usize::from(e.context.policy.selection.seats) {
            return Err(refuse("bootstrap cohort slot count"));
        }
        for (i, seat) in e.cohort.iter().enumerate() {
            if *seat != select_seat(&e.context, i as u16)? {
                return Err(refuse("bootstrap stored cohort differs from its frozen draw"));
            }
        }
    }
    Ok(())
}
fn validate_work(e: &PalwSourceBootstrapEpochRowV1, row: &PalwSourceBootstrapWorkRowV1) -> Result<(), PalwStateV2Error> {
    let p = e.context.policy;
    let epoch = Hash64::from_bytes(e.context.epoch().map_err(refuse)?.id().map_err(|_| refuse("bootstrap epoch id"))?);
    if row.work.policy != p.id()
        || row.work.epoch != epoch
        || row.work.source_profile != p.source_profile
        || row.work.checker_scope != p.checker_scope
        || row.work.class_binding != p.source_profile
        || [
            row.work.claim,
            row.work.canonical_work,
            row.work.material_scope,
            row.work.execution_commitment,
            row.work.producer,
            row.work.consumer,
        ]
        .contains(&Hash64::default())
        || row.work.accepted_daa < p.starts_at_daa
        || row.work.accepted_daa >= p.ends_at_daa
        || row.work.deadline_daa != p.ends_at_daa
        || row.approvals.len() > usize::from(p.selection.seats)
        || row.approvals.windows(2).any(|w| w[0].slot >= w[1].slot)
    {
        return Err(refuse("bootstrap work policy/epoch/scope/slots mismatch"));
    }
    for a in &row.approvals {
        let seat = e.cohort.get(usize::from(a.slot)).ok_or_else(|| refuse("bootstrap approval slot is absent"))?;
        if seat.bond != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&a.signer)
            || a.signed_daa < row.work.accepted_daa
            || a.signed_daa > a.accepted_daa
            || a.accepted_daa > row.work.deadline_daa
        {
            return Err(refuse("bootstrap approval does not bind its drawn slot/work interval"));
        }
    }
    if let Some((final_daa, settled_daa)) = row.settlement {
        if row.approvals.len() < usize::from(p.selection.quorum)
            || final_daa < row.work.accepted_daa
            || settled_daa <= final_daa
            || settled_daa > p.stake_release_daa
            || row.approvals.iter().any(|a| a.accepted_daa > settled_daa)
        {
            return Err(refuse("bootstrap source settlement lacks selected quorum/historical ordering"));
        }
    }
    Ok(())
}

impl TransitionBuilder<'_> {
    fn source_bootstrap_write_v1<T: BorshSerialize>(&mut self, k: &[u8], row: &T) -> Result<(), PalwStateV2Error> {
        if self.state.kernel_route().is_none() {
            return Err(refuse("bootstrap needs native kernel route"));
        }
        let n = borsh::object_length(row).map_err(|_| refuse("bootstrap journal row length"))?;
        self.native_reservation
            .grow_v1((n as u64).checked_add(k.len() as u64).ok_or(PalwStateV2Error::Overflow("bootstrap encoded buffers"))?)
            .map_err(PalwStateV2Error::NativeResource)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(n).map_err(|_| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<T>("bootstrap row encoding", "allocator refusal"))
        })?;
        borsh::to_writer(&mut bytes, row).map_err(|_| refuse("bootstrap row encoding"))?;
        let absent = raw(&self.state, k)?.is_none();
        let k = (PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1, k.to_vec());
        let bytes = Some(bytes);
        self.reserve_map_journal_v1(&k, &bytes, absent, "bootstrap native journal")?;
        self.write_kernel_row(k.0, k.1, bytes)?;
        Ok(())
    }
}

/// Called before object application. None is only a deterministic pre-boundary
/// state. Local context preparation faults hold the whole transition, never
/// silently omitting a journal row which another node would have applied.
pub(super) fn prepare(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if let Some(fault) = &builder.extras.selected_source_bootstrap_fault_v1 {
        return Err(match fault {
            PalwSourceBootstrapContextFaultV1::Capacity(e) => PalwStateV2Error::NativeResource(e.clone()),
            other => PalwStateV2Error::NativeContextUnavailable(other.clone()),
        });
    }
    let Some(own) = builder.extras.selected_source_bootstrap_v1.as_ref() else {
        if builder
            .params
            .selected_source_bootstrap_v1
            .as_ref()
            .is_some_and(|(at, p)| ctx.daa_score >= *at && ctx.daa_score > p.freeze_cutoff_daa)
        {
            return Err(PalwStateV2Error::NativeContextUnavailable(PalwSourceBootstrapContextFaultV1::HistoryUnavailable));
        }
        return Ok(());
    };
    own.validate_shape().map_err(refuse)?;
    if ctx.daa_score <= own.policy.freeze_cutoff_daa {
        return Ok(());
    }
    if builder.params.selected_source_bootstrap_v1.as_ref().is_none_or(|(at, p)| ctx.daa_score < *at || *p != own.policy) {
        return Err(refuse("bootstrap own context differs from configured native policy"));
    }
    let previous = builder.state.source_bootstrap_epoch_v1()?;
    if let Some(old) = &previous {
        if old.context.policy != own.policy
            || old.context.frozen_block != own.frozen_block
            || old.context.frozen_state_root != own.frozen_state_root
            || old.context.frozen_block_daa != own.frozen_block_daa
            || old.context.population != own.population
            || old.context.population_refused != own.population_refused
            || (old.context.anchor.is_some() && old.context.anchor != own.anchor)
        {
            return Err(refuse("bootstrap epoch cannot replace its frozen roster/anchor"));
        }
        if old.context.anchor == own.anchor {
            return super::palw_source_verifier_economics_fold_v2::prepare(builder, ctx);
        }
    }
    use crate::palw_native_resource_v1::NativeHeapV1;
    let amount = own.native_heap_v1("bootstrap epoch context copy").bytes().ok_or_else(|| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwSourceBootstrapOwnContextV1>("bootstrap context"))
    })?;
    let unavailable_population = own.population_refused || own.population.is_empty();
    let seats = if own.anchor.is_some() && !unavailable_population { usize::from(own.policy.selection.seats) } else { 0 };
    builder
        .native_reservation
        .grow_v1(
            amount
                .checked_add((seats * std::mem::size_of::<BootstrapSeatV1>()) as u64)
                .ok_or(PalwStateV2Error::Overflow("bootstrap context and cohort"))?,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut cohort = Vec::new();
    cohort.try_reserve_exact(seats).map_err(|_| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Vec<BootstrapSeatV1>>(
            "bootstrap cohort",
            "allocator refusal",
        ))
    })?;
    let mut draw_refused = unavailable_population;
    for slot in 0..seats {
        match select_seat(own, slot as u16) {
            Ok(seat) => cohort.push(seat),
            Err(_) => {
                cohort.clear();
                draw_refused = true;
                break;
            }
        }
    }
    let row = PalwSourceBootstrapEpochRowV1 {
        context: (***own).clone(),
        cohort,
        draw_refused,
        source_works: previous.as_ref().map(|r| r.source_works).unwrap_or(0),
    };
    builder.source_bootstrap_write_v1(&[0], &row)?;
    super::palw_source_verifier_economics_fold_v2::prepare(builder, ctx)
}

pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    actor: PalwBondKeyV2,
    action: &PalwSourceBootstrapActionV1,
) -> Result<(), PalwStateV2Error> {
    prepare(builder, ctx)?;
    let epoch = builder.state.source_bootstrap_epoch_v1()?.ok_or_else(|| refuse("bootstrap own-chain epoch context unavailable"))?;
    let p = epoch.context.policy;
    validate_cohort(&epoch)?;
    if epoch.draw_refused {
        return Err(refuse("bootstrap fixed epoch draw is unavailable; no retry anchor"));
    }
    if ctx.daa_score < p.starts_at_daa || ctx.daa_score > p.ends_at_daa {
        return Err(refuse("bootstrap is outside its fixed approval interval"));
    }
    let record = builder.state.bond(&actor).ok_or(PalwStateV2Error::MissingBond(actor))?;
    if !matches!(record.status, PalwBondStatusV2::Active) || record.collateral == 0 {
        return Err(refuse("bootstrap signer needs native active slashable collateral"));
    }
    match *action {
        PalwSourceBootstrapActionV1::Open { policy, epoch: number, claim } => {
            if policy != p.id() || number != p.epoch {
                return Err(refuse("bootstrap source opening names another fixed policy/epoch"));
            }
            let route = builder.state.kernel_route().ok_or_else(|| refuse("bootstrap kernel route absent"))?;
            let ledger = route
                .ledger_native_projection_checked_v9()
                .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refuse))?;
            let work = derive_native_work(&epoch, &ledger, claim)?;
            if work.producer != Hash64::from_bytes(crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&actor)) {
                return Err(refuse("only the native source producer opens its work"));
            }
            let id = work.id();
            if let Some(previous) = builder.state.source_bootstrap_work_v1(id)? {
                if previous.work == work {
                    return Ok(());
                }
                return Err(refuse("bootstrap work identity collision"));
            }
            if raw(&builder.state, &key(2, work.canonical_work))?.is_some() {
                return Err(refuse("bootstrap canonical work was already consumed; rewrapping cannot add a source"));
            }
            if epoch.source_works >= p.selection.maximum_source_works {
                return Err(refuse("fixed bootstrap cohort source-work cap exhausted"));
            }
            let row = PalwSourceBootstrapWorkRowV1 { work, approvals: Vec::new(), settlement: None };
            validate_work(&epoch, &row)?;
            if let Some(route) = builder.state.kernel_route() {
                crate::palw_source_verifier_economics_v2::validate_source_economic_provenance_v2(route, &epoch, &row)
                    .map_err(decode_error)?;
            }
            builder.source_bootstrap_write_v1(&key(1, id), &row)?;
            builder.source_bootstrap_write_v1(&key(2, work.canonical_work), &id)?;
            use crate::palw_native_resource_v1::NativeHeapV1;
            builder
                .native_reservation
                .grow_v1(
                    epoch
                        .native_heap_v1("bootstrap epoch counter clone")
                        .bytes()
                        .ok_or_else(|| refuse("bootstrap epoch clone coverage"))?,
                )
                .map_err(PalwStateV2Error::NativeResource)?;
            let mut next = (*epoch).clone();
            next.source_works = next.source_works.checked_add(1).ok_or(PalwStateV2Error::Overflow("bootstrap source works"))?;
            builder.source_bootstrap_write_v1(&[0], &next)?;
        }
        PalwSourceBootstrapActionV1::FundVerifierEconomicsV2 { terms } => {
            return super::palw_source_verifier_economics_fold_v2::fund(builder, ctx, actor, &epoch, &terms);
        }
        PalwSourceBootstrapActionV1::Approve { work, slot, statement, signed_daa }
        | PalwSourceBootstrapActionV1::ApproveWithLiabilityV2 { work, slot, statement, signed_daa, .. } => {
            let row =
                builder.state.source_bootstrap_work_v1(work)?.ok_or_else(|| refuse("bootstrap approval names no native work"))?;
            if row.settlement.is_some() {
                return Err(refuse("settled bootstrap source takes no further approval slots"));
            }
            if statement != row.work.id()
                || statement != work
                || signed_daa < row.work.accepted_daa
                || signed_daa > ctx.daa_score
                || signed_daa > row.work.deadline_daa
            {
                return Err(refuse("bootstrap approval statement/deadline mismatch"));
            }
            let drawn = epoch.cohort.get(usize::from(slot)).ok_or_else(|| refuse("bootstrap approval slot absent"))?;
            if drawn.bond != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&actor) {
                return Err(refuse("bootstrap signer was not drawn for this fixed slot"));
            }
            if row.approvals.iter().any(|a| a.slot == slot) {
                return Err(refuse("bootstrap approval slot already consumed"));
            }
            if let PalwSourceBootstrapActionV1::ApproveWithLiabilityV2 { funding_id, .. } = action {
                super::palw_source_verifier_economics_fold_v2::approve(builder, ctx, actor, &epoch, &row, *funding_id)?;
            } else {
                super::palw_source_verifier_economics_fold_v2::legacy_approval_allowed(builder, ctx, &epoch)?;
            }
            use crate::palw_native_resource_v1::NativeHeapV1;
            let heap =
                row.native_heap_v1("bootstrap receipt row clone").bytes().ok_or_else(|| refuse("bootstrap receipt clone coverage"))?;
            builder
                .native_reservation
                .grow_v1(
                    heap.checked_add(
                        (row.approvals.len() + 1)
                            .checked_mul(std::mem::size_of::<PalwSourceBootstrapApprovalRowV1>())
                            .ok_or(PalwStateV2Error::Overflow("bootstrap receipts"))? as u64,
                    )
                    .ok_or(PalwStateV2Error::Overflow("bootstrap receipt clone"))?,
                )
                .map_err(PalwStateV2Error::NativeResource)?;
            let mut next = (*row).clone();
            next.approvals.try_reserve_exact(1).map_err(|_| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwSourceBootstrapWorkRowV1>(
                    "bootstrap receipt vector",
                    "allocator refusal",
                ))
            })?;
            next.approvals.push(PalwSourceBootstrapApprovalRowV1 { slot, signer: actor, signed_daa, accepted_daa: ctx.daa_score });
            next.approvals.sort_unstable_by_key(|a| a.slot);
            validate_work(&epoch, &next)?;
            builder.source_bootstrap_write_v1(&key(1, work), &next)?;
            // No payment is created by a slot. Multiple weighted slots owned by
            // one key never multiply the payment for a single actual check.
        }
    }
    Ok(())
}

/// Called after native kernel closing. The source must have both the new
/// selected approvals and actual historical settlement. The OPV state is used
/// solely for the latter; it never stands in for an approval.
pub(super) fn settle(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(epoch) = builder.state.source_bootstrap_epoch_v1()? else { return Ok(()) };
    let route = builder.state.kernel_route().ok_or_else(|| refuse("bootstrap source journal lost its kernel route"))?;
    let ledger = route
        .ledger_native_projection_checked_v9()
        .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refuse))?;
    let mut after: Option<Hash64> = None;
    for _ in 0..epoch.context.policy.selection.maximum_source_works {
        let next = builder.state.kernel_route().and_then(|route| {
            route.aux.iter().find_map(|((t, k), _)| {
                if *t != PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1 || k.len() != 65 || k[0] != 1 {
                    return None;
                }
                let id = Hash64::from_bytes(k[1..].try_into().expect("64 bytes"));
                after.is_none_or(|previous| id > previous).then_some(id)
            })
        });
        let Some(id) = next else { break };
        after = Some(id);
        let row =
            builder.state.source_bootstrap_work_v1(id)?.ok_or_else(|| refuse("bootstrap source work vanished during settlement"))?;
        if row.settlement.is_some() || row.approvals.len() < usize::from(epoch.context.policy.selection.quorum) {
            continue;
        }
        let Some(source) = ledger.claims.get(&row.work.claim.as_bytes()) else { continue };
        if ctx.daa_score > epoch.context.policy.stake_release_daa
            || source.liability_until.is_some_and(|until| until >= epoch.context.policy.stake_release_daa)
        {
            // The native lifecycle can establish its liability horizon after
            // Open. A horizon outside this fixed committee's collateral lock
            // withholds this source; it must not halt ordinary chain progress.
            continue;
        }
        if source.convicted
            || matches!(
                source.life.state,
                misaka_palw_kernel::lifecycle::ClaimStateV1::Unavailable { .. }
                    | misaka_palw_kernel::lifecycle::ClaimStateV1::TimedOut { .. }
                    | misaka_palw_kernel::lifecycle::ClaimStateV1::Convicted { .. }
            )
        {
            continue;
        }
        let native = derive_native_work(&epoch, &ledger, row.work.claim)?;
        if native != row.work {
            return Err(refuse("bootstrap selected approvals refer to another native material/claim statement"));
        }
        let Some(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Settled { final_daa, settled_from_daa }) =
            crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(&ledger, &row.work.claim.as_bytes(), ctx.daa_score)
        else {
            continue;
        };
        if ctx.daa_score < settled_from_daa {
            continue;
        }
        use crate::palw_native_resource_v1::NativeHeapV1;
        let bytes = row.native_heap_v1("bootstrap historical settlement clone").bytes().ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwSourceBootstrapWorkRowV1>("bootstrap settlement clone"))
        })?;
        builder.native_reservation.grow_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
        let mut next = (*row).clone();
        next.settlement = Some((final_daa, ctx.daa_score));
        validate_work(&epoch, &next)?;
        builder.source_bootstrap_write_v1(&key(1, id), &next)?;
    }
    super::palw_source_verifier_economics_fold_v2::sweep(builder, ctx)
}

fn derive_native_work(
    epoch: &PalwSourceBootstrapEpochRowV1,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    claim: Hash64,
) -> Result<PalwSourceBootstrapWorkV1, PalwStateV2Error> {
    use misaka_palw_kernel::ledger::ClaimBodyV1;
    let p = epoch.context.policy;
    let row = ledger.claims.get(&claim.as_bytes()).ok_or_else(|| refuse("bootstrap source claim missing from own native ledger"))?;
    // The first-source profile is direct program work. Pipeline/EXEC/receipt
    // source adapters need their own coverage/lineage before being accepted.
    let ClaimBodyV1::Program { claim: _, evidence, .. } = &row.body else {
        return Err(refuse("bootstrap source requires the configured direct-program checker"));
    };
    if row.class_binding_id != p.source_profile.as_bytes()
        || row.convicted
        || ledger.paid_claim_consumers_v2.contains_key(&claim.as_bytes())
        || row.committed_daa < p.starts_at_daa
        || row.committed_daa >= p.ends_at_daa
        || row.liability_until.is_some_and(|until| until >= p.stake_release_daa)
        || ledger.mode_of_claim(&claim.as_bytes()) != Some(misaka_palw_kernel::mode::VerificationModeV1::OptimisticPublicVerification)
    {
        return Err(refuse("bootstrap source profile/commitment/independent lifecycle mismatch"));
    }
    let class = ledger.classes.get(&row.class_binding_id).ok_or_else(|| refuse("bootstrap source class absent"))?;
    let header = misaka_palw_kernel::evidence::EvidenceHeaderV1 {
        network_domain: class.network_domain,
        ruleset_digest: class.ruleset_digest,
        class_binding_id: row.class_binding_id,
        program_root: misaka_palw_kernel::public::program_root_v1(&class.program_bytes),
        artifact_root: class.param_commitments.root(),
        plan_root: hash_serialized(misaka_palw_kernel::plan::PLAN_ROOT_DOMAIN_V1, &class.plan),
    };
    if Hash64::from_bytes(hash_serialized(b"MISAKA/PALW/SOURCE-COMPLETE-CHECKER/V1", &header)) != p.checker_scope {
        return Err(refuse("bootstrap source checker scope differs from its fixed native class"));
    }
    let bytes = borsh::object_length(&row.body).map_err(|_| refuse("bootstrap source claim size"))? as u64;
    let mut schedule = std::iter::once(class.program.schedule.pre)
        .chain(class.program.schedule.layers.iter().copied())
        .chain(std::iter::once(class.program.schedule.post));
    let unit = schedule
        .try_fold(0u64, |sum, block| {
            class.program.blocks[block as usize].nodes.iter().enumerate().try_fold(sum, |n, (i, _)| {
                let c = misaka_palw_tir::admit::node_cost(&class.program, block as usize, i, u64::from(class.program.history_bound));
                n.checked_add(c.macs)?.checked_add(c.elementwise)?.checked_add(c.transcendentals)
            })
        })
        .ok_or(PalwStateV2Error::Overflow("bootstrap full-check work"))?;
    if bytes > p.maximum_claim_bytes || unit.checked_mul(u64::from(evidence.positions)).is_none_or(|n| n > p.maximum_check_work) {
        return Err(refuse("bootstrap actual source exceeds its full-check resource contract"));
    }
    let consumer = ledger.job_poster(&row.job_id).ok_or_else(|| refuse("bootstrap source has no native job payer attribution"))?;
    Ok(PalwSourceBootstrapWorkV1 {
        epoch: Hash64::from_bytes(epoch.context.epoch().map_err(refuse)?.id().map_err(|_| refuse("bootstrap epoch encoding"))?),
        policy: p.id(),
        claim,
        producer: Hash64::from_bytes(row.producer),
        consumer: Hash64::from_bytes(consumer),
        canonical_work: Hash64::from_bytes(hash_serialized(
            b"MISAKA/PALW/SOURCE-CANONICAL-INPUT/V1",
            &(row.class_binding_id, evidence.job_input_root, evidence.initial_state_root, evidence.positions),
        )),
        source_profile: p.source_profile,
        checker_scope: p.checker_scope,
        class_binding: Hash64::from_bytes(row.class_binding_id),
        execution_commitment: Hash64::from_bytes(hash_serialized(
            misaka_palw_kernel::evidence::VERIFICATION_EVIDENCE_DOMAIN_V1,
            evidence,
        )),
        material_scope: Hash64::from_bytes(hash_serialized(b"MISAKA/PALW/SOURCE-PUBLIC-MATERIAL/V1", &row.body)),
        accepted_daa: row.committed_daa,
        deadline_daa: p.ends_at_daa,
    })
}

/// V6 downstream consumers use the current own native source, not just a
/// historical journal flag. Keeping this in the bootstrap implementation also
/// keeps cohort/work derivation identical to approval admission. A source
/// which disappears, defaults, changes material or depends on paid checking
/// supplies no event; a local resource failure remains a typed local failure.
pub(crate) fn current_paid_source_event_v6(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    epoch: &PalwSourceBootstrapEpochRowV1,
    id: Hash64,
    now: u64,
    consumer_liability_until: u64,
    liability_slack: u64,
) -> Result<Option<misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1>, PalwStateV2Error> {
    use crate::palw_native_resource_v1::NativeHeapV1;
    let epoch_overlap = epoch
        .native_heap_v1("V6 source cohort validation overlap")
        .bytes()
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(65536))
        .ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwSourceBootstrapEpochRowV1>("V6 epoch validation"))
        })?;
    let _epoch_overlap =
        crate::palw_native_resource_v1::reserve_native_requests_v1(epoch_overlap).map_err(PalwStateV2Error::NativeResource)?;
    validate_cohort(epoch)?;
    if epoch.draw_refused
        || epoch.context.anchor.is_none()
        || consumer_liability_until.checked_add(liability_slack).is_none_or(|until| until > epoch.context.policy.stake_release_daa)
    {
        return Ok(None);
    }
    let bytes = crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(
        route,
        PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1,
        &key(1, id),
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let Some(bytes) = bytes else { return Ok(None) };
    let row = retain_native_value_decode_classified_v1::<PalwSourceBootstrapWorkRowV1, _>(bytes, || borsh::from_slice(bytes))
        .map_err(decode_error)?;
    validate_work(epoch, &row)?;
    crate::palw_source_verifier_economics_v2::validate_source_economic_provenance_v2(route, epoch, &row).map_err(decode_error)?;
    if row.work.id() != id {
        return Err(refuse("V6 bootstrap source key differs from its own statement"));
    }
    let Some((final_position, settlement_position)) = row.settlement else { return Ok(None) };
    let Some(source) = ledger.claims.get(&row.work.claim.as_bytes()) else { return Ok(None) };
    let source_overlap = source
        .body
        .native_heap_v1("V6 direct source identity serialization overlap")
        .bytes()
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add((row.approvals.len() as u64).checked_mul(512)?))
        .ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwSourceBootstrapWorkRowV1>("V6 source validation"))
        })?;
    let _source_overlap =
        crate::palw_native_resource_v1::reserve_native_requests_v1(source_overlap).map_err(PalwStateV2Error::NativeResource)?;
    let Some(until) = source.liability_until else { return Ok(None) };
    if source.convicted
        || until >= epoch.context.policy.stake_release_daa
        || ledger.paid_claim_consumers_v2.contains_key(&row.work.claim.as_bytes())
        || now < settlement_position
    {
        return Ok(None);
    }
    let native = match derive_native_work(epoch, ledger, row.work.claim) {
        Ok(native) => native,
        Err(PalwStateV2Error::NativeResource(error)) => return Err(PalwStateV2Error::NativeResource(error)),
        Err(_) => return Ok(None),
    };
    if native != row.work {
        return Ok(None);
    }
    let Some(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Settled { final_daa, settled_from_daa }) =
        crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(ledger, &row.work.claim.as_bytes(), now)
    else {
        return Ok(None);
    };
    if final_daa != final_position
        || settled_from_daa > settlement_position
        || settlement_position > epoch.context.policy.stake_release_daa
    {
        return Ok(None);
    }
    let anchor = epoch.context.anchor.expect("checked anchor").0;
    Ok(Some(misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1 {
        version: 1,
        network: epoch.context.policy.network.as_bytes(),
        bootstrap_epoch: row.work.epoch.as_bytes(),
        bootstrap_policy: row.work.policy.as_bytes(),
        frozen_population: epoch.context.population_root().map_err(refuse)?,
        frozen_block: epoch.context.frozen_block.as_bytes(),
        anchor: anchor.as_bytes(),
        anchor_position: epoch.context.policy.anchor_cutoff_daa,
        source_profile: row.work.source_profile.as_bytes(),
        checker_scope: row.work.checker_scope.as_bytes(),
        claim: row.work.claim.as_bytes(),
        canonical_work: row.work.canonical_work.as_bytes(),
        statement: id.as_bytes(),
        material_scope: row.work.material_scope.as_bytes(),
        execution_commitment: row.work.execution_commitment.as_bytes(),
        approval_root: hash_serialized(b"MISAKA/PALW/SOURCE-APPROVAL-ROOT/V1", &row.approvals),
        producer: row.work.producer.as_bytes(),
        consumer: row.work.consumer.as_bytes(),
        accepted_position: row.work.accepted_daa,
        final_position,
        settlement_position,
        approved_slots: row.approvals.len() as u16,
        required_slots: epoch.context.policy.selection.quorum,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> PalwSourceBootstrapOwnContextV1 {
        // Mechanics fixture, not an accepted native epoch or reviewed values.
        let h = |n| Hash64::from_bytes([n; 64]);
        let member = |n, w| PalwSourceBootstrapMemberV1 {
            bond: PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_bytes([n; 64]),
                index: 0,
            }),
            locked_weight: w,
        };
        let mut population = vec![member(1, 1), member(2, 3)];
        population.sort_unstable_by_key(|m| m.candidate().bond);
        PalwSourceBootstrapOwnContextV1 {
            policy: PalwSelectedSourceBootstrapPolicyV1 {
                version: 1,
                chain_genesis: h(3),
                network: h(4),
                ruleset: h(5),
                epoch: 1,
                freeze_cutoff_daa: 10,
                anchor_cutoff_daa: 20,
                starts_at_daa: 30,
                ends_at_daa: 40,
                stake_release_daa: 60,
                selection: misaka_palw_challenge::bootstrap_epoch_v1::BootstrapPolicyV1 {
                    version: 1,
                    seats: 2,
                    quorum: 2,
                    maximum_population: 2,
                    maximum_draw_attempts: 2,
                    maximum_source_works: 2,
                },
                source_profile: h(6),
                checker_scope: h(7),
                maximum_claim_bytes: 100,
                maximum_check_work: 100,
                maximum_history_headers: 100,
                security_review: h(8),
                economic_review: h(9),
            },
            frozen_block: h(10),
            frozen_state_root: h(11),
            frozen_block_daa: 10,
            population,
            population_refused: false,
            anchor: Some((h(12), 20)),
        }
    }
    #[test]
    fn native_draw_matches_shared_epoch_contract_without_work_input_or_roster_clone() {
        let c = context();
        let epoch = c.epoch().unwrap();
        let population = c.population.iter().map(|m| m.candidate()).collect::<Vec<_>>();
        assert_eq!(
            c.population_root().unwrap(),
            misaka_palw_challenge::bootstrap_epoch_v1::bootstrap_population_root_v1(&population).unwrap()
        );
        let committee =
            misaka_palw_challenge::bootstrap_epoch_v1::select_bootstrap_committee_v1(&c.policy.selection, &epoch, &population, |_| {
                Ok::<_, ()>(())
            })
            .unwrap();
        for (slot, seat) in committee.seats().iter().enumerate() {
            assert_eq!(*seat, select_seat(&c, slot as u16).unwrap());
        }
        let row =
            PalwSourceBootstrapEpochRowV1 { context: c, cohort: committee.seats().to_vec(), draw_refused: false, source_works: 0 };
        validate_cohort(&row).unwrap();
        let mut forged = row;
        forged.draw_refused = true;
        forged.cohort.clear();
        assert!(validate_cohort(&forged).is_err(), "a successful fixed draw cannot be relabelled unavailable");
    }
    #[test]
    fn unanchored_epoch_has_no_cohort_or_synthetic_approval() {
        let mut c = context();
        c.anchor = None;
        let mut row = PalwSourceBootstrapEpochRowV1 { context: c, cohort: vec![], draw_refused: false, source_works: 0 };
        validate_cohort(&row).unwrap();
        row.source_works = 1;
        assert!(validate_cohort(&row).is_err());
        row.source_works = 0;
        row.draw_refused = true;
        assert!(validate_cohort(&row).is_err());
    }
    #[test]
    fn a_daa_jump_past_both_cutoffs_closes_the_epoch_without_halting_the_chain_or_substituting_an_anchor() {
        let mut c = context();
        c.anchor = Some((c.frozen_block, c.frozen_block_daa));
        c.validate_shape().unwrap();
        assert!(c.epoch().is_err());
        let row = PalwSourceBootstrapEpochRowV1 { context: c, cohort: vec![], draw_refused: true, source_works: 0 };
        validate_cohort(&row).unwrap();
    }
    #[test]
    fn an_empty_frozen_stake_population_closes_the_epoch_without_a_later_roster_substitute() {
        let mut c = context();
        c.population.clear();
        c.validate_shape().unwrap();
        assert!(c.epoch().is_err());
        let row = PalwSourceBootstrapEpochRowV1 { context: c, cohort: vec![], draw_refused: true, source_works: 0 };
        validate_cohort(&row).unwrap();
    }
    #[test]
    fn missing_own_context_cannot_silently_accept_an_empty_postfreeze_block() {
        let mut p = PalwStateParamsV2::new(0, 1, 1, 1, 1, 10, Hash64::from_bytes([33; 64]), 2, 1000, 1, 1000, 0).unwrap();
        p.selected_source_bootstrap_v1 = Some((1, context().policy));
        let parent = PalwChainStateV2::genesis();
        let before = parent.state_root();
        let ctx = PalwBlockContextV2 { block: Hash64::from_bytes([34; 64]), daa_score: 31, blue_score: 1, subsidy: 0 };
        let extras = PalwTransitionExtrasV1::default();
        let result = apply_palw_transition_v7(
            &parent,
            &p,
            None,
            &ctx,
            &[],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &extras,
        );
        assert!(matches!(
            result,
            Err(PalwStateV2Error::NativeContextUnavailable(PalwSourceBootstrapContextFaultV1::HistoryUnavailable))
        ));
        assert_eq!(parent.state_root(), before, "unknown native history must not publish an alternate post-state");
    }
}
