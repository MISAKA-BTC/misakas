//! Structural R pairing only. Actual native restore must separately resolve
//! accepted ancestry, original Params/scope/bond, normal transaction signature,
//! fee burn, current/rollback custody and account binding history.
use super::*;
use crate::palw_positive_assignment_funds_v2::{
    case_relay_rotation_reservation_v2, check_initial_assignment_relay_target_v2, check_initial_case_relay_target_v2,
    relay_origin_precedes_v2,
};
use misaka_palw_positive_contract::assignment::{funding_relay::*, funding_relay_rows::*};
impl Reader<'_> {
    fn no_other_relay_purpose(&mut self, funding: NativeOutpoint, purpose: RelayFundingPurposeV2) -> Result<(), PositiveRowErrorV2> {
        for other in [RelayFundingPurposeV2::PosterA, RelayFundingPurposeV2::CaseC, RelayFundingPurposeV2::ProverJoin] {
            if other == purpose {
                continue;
            }
            for key in [
                NativeRowKey::FundingRelayAuthorization { funding, purpose: other },
                NativeRowKey::FundingRelayCustody { funding, purpose: other },
            ] {
                add(&mut self.report.reference_reads, 1)?;
                if self.aux.contains_key(&(KERNEL_AUX_TABLE, key.encode())) {
                    return inconsistent();
                }
            }
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn validate_funding_relay_authorization_row(
        &mut self,
        row: &FundingRelayAuthorizationRowV2,
    ) -> Result<(), PositiveRowErrorV2> {
        let NativeRowBody::FundingRelayCustody(custody) = self
            .required(NativeRowKey::FundingRelayCustody { funding: row.terms.funding_outpoint, purpose: row.terms.funding_purpose })?
        else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        custody.check_pair(row)?;
        row.terms.authenticate_purpose(&row.payer_key, &row.purpose_signature)?;
        add(&mut self.report.signature_checks, 1)?;
        self.no_other_relay_purpose(row.terms.funding_outpoint, row.terms.funding_purpose)
    }
    #[inline(never)]
    pub(super) fn validate_funding_relay_custody_row(&mut self, row: &FundingRelayCustodyRowV2) -> Result<(), PositiveRowErrorV2> {
        let NativeRowBody::FundingRelayAuthorization(auth) =
            self.required(NativeRowKey::FundingRelayAuthorization { funding: row.funding_outpoint, purpose: row.purpose })?
        else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        row.check_pair(&auth)?;
        if let Some(binding) = row.binding {
            match auth.terms.target {
                RelayTargetV2::InitialAssignment { .. } => self.validate_initial_assignment_relay_binding(&auth, binding)?,
                RelayTargetV2::InitialCase { .. } => self.validate_initial_case_relay_binding(&auth, binding)?,
                RelayTargetV2::Continuation { .. } if row.purpose == RelayFundingPurposeV2::CaseC => {
                    self.validate_case_relay_rotation(&auth, row, binding)?;
                }
                _ => {}
            }
        }
        self.no_other_relay_purpose(row.funding_outpoint, row.purpose)
    }

    fn validate_initial_assignment_relay_binding(
        &mut self,
        auth: &FundingRelayAuthorizationRowV2,
        binding: FundingRelayBindingV2,
    ) -> Result<(), PositiveRowErrorV2> {
        let id = auth.terms.assignment;
        let NativeRowBody::Assignment(assignment) = self.required(NativeRowKey::Assignment(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::AssignmentFunding(account) = self.required(NativeRowKey::AssignmentFunding(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        check_initial_assignment_relay_target_v2(
            &auth.terms,
            &assignment.bundle,
            account.original_outpoint,
            account.initial_locked,
            binding.origin.accepted_daa,
        )?;
        // Original F and its initial amount remain stable across partial and
        // terminal settlement. Its live cursor is validated by its own owner.
        if assignment.origin != binding.origin
            || account.funding_origin != binding.origin
            || account.payer_key != auth.payer_key
            || binding.reservation != assignment.reservation
            || binding.reservation != account.reservation
            || auth
                .terms
                .custody_until_daa
                .checked_sub(auth.policy.rollback_margin_daa)
                .is_none_or(|service_cutoff| service_cutoff < assignment.bundle.terms.reserve_until_daa)
        {
            return inconsistent();
        }
        Ok(())
    }
    fn validate_initial_case_relay_binding(
        &mut self,
        auth: &FundingRelayAuthorizationRowV2,
        binding: FundingRelayBindingV2,
    ) -> Result<(), PositiveRowErrorV2> {
        let id = auth.terms.assignment;
        let NativeRowBody::AssignmentUse(used) = self.required(NativeRowKey::AssignmentUse(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::Assignment(assignment) = self.required(NativeRowKey::Assignment(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let result = self.result(used.result)?;
        let case = self.case(used.case)?;
        let NativeRowBody::Funding(account) = self.required(NativeRowKey::Funding(used.case))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        check_initial_case_relay_target_v2(
            &auth.terms,
            &assignment.bundle,
            result.result.id(),
            &account.consent,
            account.original_outpoint,
            account.initial_locked,
            binding.origin.accepted_daa,
        )?;
        // Preserve the original source of funds after any cursor changes.
        // AssignmentUse is validated separately against its Case and journal.
        if used.origin != binding.origin
            || result.origin != binding.origin
            || account.funding_origin != binding.origin
            || account.payer_key != auth.payer_key
            || account.consent.scope != result.funding_scope
            || binding.original_contract != account.consent.id().0
            || binding.reservation != case.reservation
        {
            return inconsistent();
        }
        // The initial package can remain during rollback after rotation. It
        // cannot stand in for custody of the Case's current live output.
        if let Some(current) = account.current_outpoint.filter(|point| *point != account.original_outpoint) {
            let NativeRowBody::FundingRelayCustody(current_custody) =
                self.required(NativeRowKey::FundingRelayCustody { funding: current, purpose: RelayFundingPurposeV2::CaseC })?
            else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if current_custody.binding.is_none_or(|current_binding| {
                current_binding.original_funding != account.original_outpoint
                    || current_binding.original_contract != account.consent.id().0
                    || account.last_settlement.is_none_or(|last| current_binding.origin != last.origin)
            }) {
                return inconsistent();
            }
        }
        Ok(())
    }

    fn validate_case_relay_rotation(
        &mut self,
        auth: &FundingRelayAuthorizationRowV2,
        custody: &FundingRelayCustodyRowV2,
        binding: FundingRelayBindingV2,
    ) -> Result<(), PositiveRowErrorV2> {
        let terms = &auth.terms;
        let NativeRowBody::Assignment(assignment) = self.required(NativeRowKey::Assignment(terms.assignment))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let AssignmentStatusV2::Consumed { case, result } = assignment.status else {
            return inconsistent();
        };
        let contract = &assignment.bundle.terms;
        if terms.contract_network != contract.network
            || terms.contract_ruleset != contract.ruleset
            || terms.contract_anchor != contract.branch_anchor
            || terms.payer_bond != contract.poster
            || terms.payer != assignment.bundle.future_case.payer
            || terms.refund_payload != assignment.bundle.future_case.refund_payload
        {
            return inconsistent();
        }
        let NativeRowBody::AssignmentUse(used) = self.required(NativeRowKey::AssignmentUse(terms.assignment))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::Funding(account) = self.required(NativeRowKey::Funding(case))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let case_row = self.case(case)?;
        let Some(last) = account.last_settlement else {
            return inconsistent();
        };
        if used.case != case
            || used.result != result
            || case_row.result != result
            || used.origin != account.funding_origin
            || terms.payer != account.consent.payer
            || terms.refund_payload != account.consent.refund_payload
            || auth.payer_key != account.payer_key
            || binding.original_funding != account.original_outpoint
            || binding.original_contract != account.consent.id().0
            || !relay_origin_precedes_v2(account.funding_origin, binding.origin)
            || (binding.origin != last.origin && !relay_origin_precedes_v2(binding.origin, last.origin))
            || binding.reservation != case_relay_rotation_reservation_v2(binding.origin, case_row.reservation, custody.reservation)
            || terms.expected_amount > account.initial_locked
        {
            return inconsistent();
        }
        check_case_relay_rotation_cursor(&account, custody.funding_outpoint, terms.expected_amount, binding.origin, last.origin)
    }
}

/// A retained relay can be older than the current cursor. Only the relay
/// produced by the last settlement must describe the exact remaining funds.
fn check_case_relay_rotation_cursor(
    account: &FundingRow,
    output: NativeOutpoint,
    amount: u64,
    binding: AcceptedOrigin,
    last: AcceptedOrigin,
) -> Result<(), PositiveRowErrorV2> {
    if account.current_outpoint == Some(output) {
        let remaining = [account.fee.remaining, account.capital.remaining, account.surplus.remaining]
            .into_iter()
            .try_fold(0u64, u64::checked_add)
            .ok_or(ContractError::Budget)?;
        if binding != last || amount != remaining {
            return inconsistent();
        }
    } else if binding == last {
        return inconsistent();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_positive_rows_v2::tests::signed_bundle;

    #[test]
    fn case_relay_restore_matches_the_current_coin_amount_and_last_event() {
        let (_, _, _, mut account, _, _) = signed_bundle();
        let output = NativeOutpoint { transaction: NativeHash64([80; 64]), index: 2 };
        let origin = AcceptedOrigin { carrier_transaction: output.transaction, ..account.funding_origin };
        account.current_outpoint = Some(output);
        account.fee.paid += 1;
        account.fee.remaining -= 1;
        let amount = account.initial_locked - 1;
        assert_eq!(check_case_relay_rotation_cursor(&account, output, amount, origin, origin), Ok(()));
        assert_eq!(
            check_case_relay_rotation_cursor(&account, output, amount + 1, origin, origin),
            Err(PositiveRowErrorV2::InconsistentLinks)
        );
        assert_eq!(
            check_case_relay_rotation_cursor(&account, account.original_outpoint, amount, origin, origin),
            Err(PositiveRowErrorV2::InconsistentLinks)
        );
        let later = AcceptedOrigin { accepted_tx_ordinal: origin.accepted_tx_ordinal + 1, ..origin };
        assert_eq!(
            check_case_relay_rotation_cursor(&account, output, amount, origin, later),
            Err(PositiveRowErrorV2::InconsistentLinks)
        );
        assert_eq!(check_case_relay_rotation_cursor(&account, account.original_outpoint, amount + 1, origin, later), Ok(()));
    }
}
