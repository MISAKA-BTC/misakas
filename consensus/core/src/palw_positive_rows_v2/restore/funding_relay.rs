//! Structural R pairing only. Actual native restore must separately resolve
//! accepted ancestry, original Params/scope/bond, normal transaction signature,
//! fee burn, current/rollback custody and account binding history.
use super::*;
use crate::palw_positive_assignment_funds_v2::check_initial_assignment_relay_target_v2;
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
        if let Some(binding) = row.binding
            && matches!(auth.terms.target, RelayTargetV2::InitialAssignment { .. })
        {
            self.validate_initial_assignment_relay_binding(&auth, binding)?;
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
        if assignment.origin != binding.origin || account.funding_origin != binding.origin || account.payer_key != auth.payer_key {
            return inconsistent();
        }
        Ok(())
    }
}
