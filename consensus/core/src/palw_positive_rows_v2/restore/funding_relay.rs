//! Structural R pairing only. Actual native restore must separately resolve
//! accepted ancestry, original Params/scope/bond, normal transaction signature,
//! fee burn, current/rollback custody and account binding history.
use super::*;
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
        self.no_other_relay_purpose(row.funding_outpoint, row.purpose)
    }
}
