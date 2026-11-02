//! Fixed-row RFC-0045 crosslinks. No cached status grants service or credit.
use super::*;
impl Reader<'_> {
    fn assignment(&mut self, id: AssignmentId) -> Result<AssignmentRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::Assignment(id))? {
            NativeRowBody::Assignment(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn assignment_frame(&mut self, id: AssignmentId, ordinal: u64) -> Result<AssignmentJournalFrameRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::AssignmentJournalFrame { assignment: id, ordinal })? {
            NativeRowBody::AssignmentJournalFrame(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn assignment_head(&mut self, id: AssignmentId) -> Result<AssignmentJournalHeadRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::AssignmentJournalHead(id))? {
            NativeRowBody::AssignmentJournalHead(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    #[inline(never)]
    pub(super) fn validate_assignment_row(&mut self, row: &AssignmentRow) -> Result<(), PositiveRowErrorV2> {
        let t = &row.bundle.terms;
        if row.admission_material.origin != row.origin {
            return inconsistent();
        }
        let NativeRowBody::AssignmentNonce(nonce) = self.required(NativeRowKey::AssignmentNonce(t.poster))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        if (!nonce.nonce.exhausted && nonce.nonce.next <= t.poster_nonce) || (nonce.nonce.exhausted && nonce.nonce.next != u64::MAX) {
            return inconsistent();
        }
        let NativeRowBody::AssignmentFunding(funds) = self.required(NativeRowKey::AssignmentFunding(row.id()))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        self.assignment_funding_links(&funds, row)?;
        for (role, bond) in
            [(AssignmentRoleV2::Poster, t.poster), (AssignmentRoleV2::Worker, t.worker), (AssignmentRoleV2::Prover, t.prover)]
        {
            let NativeRowBody::AssignmentRole(auth) =
                self.required(NativeRowKey::AssignmentRole { assignment: row.id(), role, bond })?
            else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if auth.authorization != AssignmentRoleAuthorizationV2::Initial {
                return inconsistent();
            }
            auth.authenticate(row)?;
            add(&mut self.report.signature_checks, 1)?;
        }
        let key = (KERNEL_AUX_TABLE, NativeRowKey::AssignmentUse(row.id()).encode());
        match row.status {
            AssignmentStatusV2::Consumed { case, result } => {
                let NativeRowBody::AssignmentUse(used) = self.required(NativeRowKey::AssignmentUse(row.id()))? else {
                    return Err(PositiveRowErrorV2::WrongRowKind);
                };
                if used.case != case || used.result != result {
                    return inconsistent();
                }
            }
            _ => {
                if self.aux.contains_key(&key) {
                    return inconsistent();
                }
            }
        }
        let head = self.assignment_head(row.id())?;
        if head.retention_until_daa < t.reserve_until_daa || head.origin != row.last_event {
            return inconsistent();
        }
        self.assignment_lifecycle(row, &funds, &head)?;
        let first = self.assignment_frame(row.id(), 0)?;
        if first.header.origin != row.origin || first.header.kind != AssignmentJournalEventKindV2::Open {
            return inconsistent();
        }
        Ok(())
    }

    fn assignment_lifecycle(
        &mut self,
        a: &AssignmentRow,
        funds: &AssignmentFundingRow,
        head: &AssignmentJournalHeadRow,
    ) -> Result<(), PositiveRowErrorV2> {
        let mut prefix = NativeRowKey::AssignmentJournalFrame { assignment: a.id(), ordinal: 0 }.encode();
        prefix.truncate(prefix.len() - 8);
        let (mut frames, mut binds, mut cancels, mut expires) = (0u64, 0u64, 0u64, 0u64);
        let mut terminal = None::<u64>;
        let mut last_join = None::<u64>;
        let aux = self.aux;
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.clone())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            let NativeRowBody::AssignmentJournalFrame(frame) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            add(&mut frames, 1)?;
            match frame.header.kind {
                AssignmentJournalEventKindV2::BindResult => add(&mut binds, 1)?,
                AssignmentJournalEventKindV2::Cancel => {
                    add(&mut cancels, 1)?;
                    terminal = Some(frame.header.ordinal);
                }
                AssignmentJournalEventKindV2::Expire => {
                    add(&mut expires, 1)?;
                    terminal = Some(frame.header.ordinal);
                    if frame.header.origin.accepted_daa < a.bundle.terms.result_before_daa {
                        return inconsistent();
                    }
                }
                AssignmentJournalEventKindV2::JoinProver => {
                    last_join = Some(last_join.map_or(frame.header.ordinal, |n| n.max(frame.header.ordinal)))
                }
                _ => {}
            }
        }
        let expected = match a.status {
            AssignmentStatusV2::Open => (0, 0, 0),
            AssignmentStatusV2::Consumed { .. } => (1, 0, 0),
            AssignmentStatusV2::Cancelled => (0, 1, 0),
            AssignmentStatusV2::ExpiredUnconsumed => (0, 0, 1),
        };
        if frames != head.next_ordinal || (binds, cancels, expires) != expected || terminal.zip(last_join).is_some_and(|(t, j)| j > t)
        {
            return inconsistent();
        }
        let mut prefix = NativeRowKey::AssignmentSettlement {
            assignment: a.id(),
            leg: AssignmentSettlementLegV2::WorkerPayment,
            party: PrincipalId(Digest([0; 64])),
            ordinal: 0,
        }
        .encode();
        prefix.truncate(prefix.len() - 73);
        let (mut worker, mut service, mut returned, mut burned) = (0u64, 0u64, 0u64, 0u64);
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.clone())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            let NativeRowBody::AssignmentSettlement(settlement) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if settlement.funding != AssignmentSettlementFundingV2::Poster {
                continue;
            }
            match settlement.leg {
                AssignmentSettlementLegV2::WorkerPayment => add(&mut worker, settlement.amount)?,
                AssignmentSettlementLegV2::ServiceFee => add(&mut service, settlement.amount)?,
                AssignmentSettlementLegV2::CapitalReturn | AssignmentSettlementLegV2::Refund => add(&mut returned, settlement.amount)?,
                AssignmentSettlementLegV2::Burn => add(&mut burned, settlement.amount)?,
            }
        }
        let legs = [funds.worker, funds.fee, funds.capital, funds.surplus];
        let sum =
            |get: fn(&BalanceLeg) -> u64| legs.iter().try_fold(0u64, |n, leg| n.checked_add(get(leg)).ok_or(ContractError::Budget));
        if worker != funds.worker.paid
            || service != funds.fee.paid
            || sum(|l| l.paid)? != worker.checked_add(service).ok_or(ContractError::Budget)?
            || returned != sum(|l| l.refunded)?
            || burned != sum(|l| l.burned)?
        {
            return inconsistent();
        }
        Ok(())
    }
    pub(super) fn validate_assignment_nonce_row(&self, row: &AssignmentNonceRow) -> Result<(), PositiveRowErrorV2> {
        // Permanent counters may legitimately outlive every retired payload.
        if row.nonce.exhausted && row.nonce.next != u64::MAX {
            return inconsistent();
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_assignment_use_row(&mut self, row: &AssignmentUseRow) -> Result<(), PositiveRowErrorV2> {
        let assignment = self.assignment(row.assignment)?;
        if assignment.status != (AssignmentStatusV2::Consumed { case: row.case, result: row.result })
            || row.origin.accepted_daa >= assignment.bundle.terms.result_before_daa
        {
            return inconsistent();
        }
        let result = self.result(row.result)?;
        let case = self.case(row.case)?;
        if case.result != row.result
            || result.origin != row.origin
            || result.result.job != assignment.job
            || result.result.input != assignment.input
            || result.result.registered_class != assignment.bundle.terms.registered_class
            || result.miner_key != assignment.worker_key
            || result.result.scope.ruleset != assignment.bundle.terms.ruleset
            || result.result.scope.network != assignment.bundle.terms.network
            || result.result.scope.branch.0 != assignment.bundle.terms.branch_anchor.0
            || result.policy != assignment.positive_policy
            || result.fired_fork != assignment.fired_rules
            || result.dependencies != assignment.dependencies
            || result.role_assignment != (AcceptedMaterial { origin: assignment.origin, digest: assignment.id().0 })
        {
            return inconsistent();
        }
        same_block_event_order_v2(&assignment.origin, &row.origin, true)?;
        let prefix = NativeRowKey::AssignmentJournalFrame { assignment: row.assignment, ordinal: 0 }.encode();
        let prefix = &prefix[..prefix.len() - 8];
        let mut matched = false;
        let aux = self.aux;
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.to_vec())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(prefix) {
                break;
            }
            let NativeRowBody::AssignmentJournalFrame(frame) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if frame.header.kind == AssignmentJournalEventKindV2::BindResult {
                if matched || frame.header.id() != row.binding.digest || frame.header.origin != row.origin {
                    return inconsistent();
                }
                matched = true;
            }
        }
        if !matched {
            return inconsistent();
        }
        Ok(())
    }
    fn assignment_funding_links(&mut self, row: &AssignmentFundingRow, a: &AssignmentRow) -> Result<(), PositiveRowErrorV2> {
        if row.authorization != a.bundle.funding
            || row.funding_origin != a.origin
            || row.worker.initial != a.bundle.terms.worker_reward
            || row.reservation != a.reservation
            || row.liability != a.liability
        {
            return inconsistent();
        }
        a.bundle.authenticate_role(AssignmentRoleV2::Poster, &row.payer_key, &row.payer_signature)?;
        add(&mut self.report.signature_checks, 1)?;
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_assignment_funding_row(&mut self, row: &AssignmentFundingRow) -> Result<(), PositiveRowErrorV2> {
        let a = self.assignment(row.assignment)?;
        self.assignment_funding_links(row, &a)
    }
    #[inline(never)]
    pub(super) fn validate_assignment_settlement_row(&mut self, row: &AssignmentSettlementRow) -> Result<(), PositiveRowErrorV2> {
        crate::palw_state_v2::validate_assignment_once_reference_v2(self.aux, row)
            .map_err(|_| PositiveRowErrorV2::InconsistentLinks)?;
        let a = self.assignment(row.assignment)?;
        same_block_event_order_v2(&a.origin, &row.origin, true)?;
        match row.funding {
            AssignmentSettlementFundingV2::Poster => {}
            AssignmentSettlementFundingV2::Prover(prover) => {
                if row.leg == AssignmentSettlementLegV2::WorkerPayment {
                    return inconsistent();
                }
                let NativeRowBody::AssignmentProverFunding(_) =
                    self.required(NativeRowKey::AssignmentProverFunding { assignment: row.assignment, prover })?
                else {
                    return Err(PositiveRowErrorV2::WrongRowKind);
                };
            }
        }
        if row.leg == AssignmentSettlementLegV2::WorkerPayment
            && (!matches!(a.status, AssignmentStatusV2::Consumed { .. })
                || row.party != principal(&a.worker_key)
                || row.amount != a.bundle.terms.worker_reward
                || row.ordinal != 0)
        {
            return inconsistent();
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_assignment_role_row(&mut self, row: &AssignmentRoleRow) -> Result<(), PositiveRowErrorV2> {
        let a = self.assignment(row.assignment)?;
        row.authenticate(&a)?;
        add(&mut self.report.signature_checks, 1)?;
        same_block_event_order_v2(&a.origin, &row.origin, false)?;
        if let AssignmentRoleAuthorizationV2::Joined(join) = row.authorization {
            let NativeRowBody::AssignmentProverFunding(funds) =
                self.required(NativeRowKey::AssignmentProverFunding { assignment: row.assignment, prover: row.bond })?
            else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if funds.consent != join || funds.funding_origin != row.origin {
                return inconsistent();
            }
            if let Some((case, result)) = join.case
                && a.status != (AssignmentStatusV2::Consumed { case, result })
            {
                return inconsistent();
            }
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_assignment_journal_head_row(&mut self, row: &AssignmentJournalHeadRow) -> Result<(), PositiveRowErrorV2> {
        let a = self.assignment(row.assignment)?;
        let tail = self.assignment_frame(row.assignment, row.next_ordinal - 1)?;
        if tail.header.id() != row.last_frame
            || tail.header.origin != row.origin
            || row.origin != a.last_event
            || row.retention_until_daa < a.bundle.terms.reserve_until_daa
        {
            return inconsistent();
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_assignment_journal_frame_row(&mut self, row: &AssignmentJournalFrameRow) -> Result<(), PositiveRowErrorV2> {
        let a = self.assignment(row.header.assignment)?;
        let head = self.assignment_head(a.id())?;
        if row.header.ordinal >= head.next_ordinal || row.header.origin.accepted_daa < a.origin.accepted_daa {
            return inconsistent();
        }
        if row.header.ordinal == 0 {
            if row.header.origin != a.origin {
                return inconsistent();
            }
        } else {
            let prev = self.assignment_frame(a.id(), row.header.ordinal - 1)?;
            if prev.header.id() != row.header.previous {
                return inconsistent();
            }
            same_block_event_order_v2(&prev.header.origin, &row.header.origin, true)?;
        }
        if row.header.ordinal + 1 < head.next_ordinal {
            let next = self.assignment_frame(a.id(), row.header.ordinal + 1)?;
            if next.header.previous != row.header.id() {
                return inconsistent();
            }
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_assignment_prover_funding_row(
        &mut self,
        row: &AssignmentProverFundingRow,
    ) -> Result<(), PositiveRowErrorV2> {
        let a = self.assignment(row.assignment)?;
        row.consent.authenticate(&a.bundle, &row.payer_key, &row.payer_signature, row.funding_origin.accepted_daa)?;
        add(&mut self.report.signature_checks, 1)?;
        let NativeRowBody::AssignmentRole(role) = self.required(NativeRowKey::AssignmentRole {
            assignment: row.assignment,
            role: AssignmentRoleV2::Prover,
            bond: row.consent.prover,
        })?
        else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        if role.authorization != AssignmentRoleAuthorizationV2::Joined(row.consent)
            || role.key != row.payer_key
            || role.signature != row.payer_signature
            || role.origin != row.funding_origin
            || role.lease != row.reservation
            || role.funding.is_none_or(|r| r.digest != row.consent.id() || r.origin != row.funding_origin)
        {
            return inconsistent();
        }
        let mut prefix = NativeRowKey::AssignmentSettlement {
            assignment: row.assignment,
            leg: AssignmentSettlementLegV2::WorkerPayment,
            party: PrincipalId(Digest([0; 64])),
            ordinal: 0,
        }
        .encode();
        prefix.truncate(prefix.len() - 73);
        let (mut paid, mut returned, mut burned) = (0u64, 0u64, 0u64);
        let aux = self.aux;
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.clone())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            let NativeRowBody::AssignmentSettlement(s) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if s.funding != AssignmentSettlementFundingV2::Prover(row.consent.prover) {
                continue;
            }
            match s.leg {
                AssignmentSettlementLegV2::ServiceFee => add(&mut paid, s.amount)?,
                AssignmentSettlementLegV2::CapitalReturn | AssignmentSettlementLegV2::Refund => add(&mut returned, s.amount)?,
                AssignmentSettlementLegV2::Burn => add(&mut burned, s.amount)?,
                AssignmentSettlementLegV2::WorkerPayment => return inconsistent(),
            }
        }
        let sum = |get: fn(&BalanceLeg) -> u64| {
            [row.fee, row.capital, row.surplus].iter().try_fold(0u64, |n, l| n.checked_add(get(l)).ok_or(ContractError::Budget))
        };
        if paid != row.fee.paid || paid != sum(|l| l.paid)? || returned != sum(|l| l.refunded)? || burned != sum(|l| l.burned)? {
            return inconsistent();
        }
        Ok(())
    }
}
