//! Current rooted assignment DTO reader. Structural/signature checks are not
//! accepted-prefix, lock, lease, computation or selected-verifier authority.
use super::*;
use misaka_palw_positive_contract::assignment::{AssignmentId, rows::*};
pub struct PositiveAssignmentRowsV2 {
    pub assignment: AssignmentRow,
    pub funding: AssignmentFundingRow,
    pub use_row: Option<AssignmentUseRow>,
    pub journal: AssignmentJournalHeadRow,
}
impl PositiveRowsV2<'_> {
    pub fn assignment(&self, id: AssignmentId) -> Result<PositiveAssignmentRowsV2, PositiveRowErrorV2> {
        let NativeRowBody::Assignment(assignment) = self.required(NativeRowKey::Assignment(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::AssignmentFunding(funding) = self.required(NativeRowKey::AssignmentFunding(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::AssignmentJournalHead(journal) = self.required(NativeRowKey::AssignmentJournalHead(id))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let use_row = match self.read(NativeRowKey::AssignmentUse(id))?.map(|r| r.body) {
            None => None,
            Some(NativeRowBody::AssignmentUse(row)) => Some(row),
            _ => return Err(PositiveRowErrorV2::WrongRowKind),
        };
        assignment.validate_shape()?;
        funding.validate_shape()?;
        if funding.authorization != assignment.bundle.funding
            || funding.funding_origin != assignment.origin
            || journal.retention_until_daa < assignment.bundle.terms.reserve_until_daa
        {
            return Err(PositiveRowErrorV2::InconsistentLinks);
        }
        match (assignment.status, use_row) {
            (AssignmentStatusV2::Consumed { case, result }, Some(row)) if row.case == case && row.result == result => {}
            (AssignmentStatusV2::Consumed { .. }, _) | (_, Some(_)) => return Err(PositiveRowErrorV2::InconsistentLinks),
            _ => {}
        }
        Ok(PositiveAssignmentRowsV2 { assignment, funding, use_row, journal })
    }
}
