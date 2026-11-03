//! Current rooted assignment DTO reader. Structural/signature checks are not
//! accepted-prefix, lock, lease, computation or selected-verifier authority.
use super::*;
use misaka_palw_positive_contract::assignment::{AssignmentId, journal::ASSIGNMENT_JOURNAL_HEADER_BYTES_V2, rows::*};

/// Exact byte extents committed by the current rooted Assignment history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssignmentJournalExtentV2 {
    pub record_bytes: u64,
    pub largest_record_bytes: u64,
}

fn measure_assignment_journal_v2(
    head: &AssignmentJournalHeadRow,
    open_origin: AcceptedOrigin,
    maximum_frames: u64,
    maximum_record_bytes: u64,
    maximum_total_bytes: u64,
    mut read: impl FnMut(u64) -> Result<AssignmentJournalFrameRow, PositiveRowErrorV2>,
) -> Result<AssignmentJournalExtentV2, PositiveRowErrorV2> {
    if head.next_ordinal == 0 {
        return Err(PositiveRowErrorV2::InconsistentLinks);
    }
    if head.next_ordinal > maximum_frames {
        return Err(ContractError::Budget.into());
    }
    let mut extent = AssignmentJournalExtentV2::default();
    let mut prior = None;
    for ordinal in 0..head.next_ordinal {
        let frame = read(ordinal)?;
        if frame.header.assignment != head.assignment
            || frame.header.ordinal != ordinal
            || (ordinal == 0 && frame.header.origin != open_origin)
        {
            return Err(PositiveRowErrorV2::InconsistentLinks);
        }
        let bytes = u64::from(frame.header.payload_bytes) + ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
        extent.record_bytes = extent
            .record_bytes
            .checked_add(bytes)
            .filter(|n| *n <= maximum_total_bytes && bytes <= maximum_record_bytes)
            .ok_or(ContractError::Budget)?;
        extent.largest_record_bytes = extent.largest_record_bytes.max(bytes);
        prior = Some(frame.append_to(prior.as_ref(), head.retention_until_daa)?);
    }
    if prior.as_ref() != Some(head) {
        return Err(PositiveRowErrorV2::InconsistentLinks);
    }
    Ok(extent)
}

pub struct PositiveAssignmentRowsV2 {
    pub assignment: AssignmentRow,
    pub funding: AssignmentFundingRow,
    pub use_row: Option<AssignmentUseRow>,
    pub journal: AssignmentJournalHeadRow,
}
impl PositiveRowsV2<'_> {
    /// Walk at most the original frame budget using exact native keys. This
    /// prices rooted metadata only; the owner still checks every literal byte.
    pub fn assignment_journal_extent_v2(
        &self,
        head: &AssignmentJournalHeadRow,
        open_origin: AcceptedOrigin,
        maximum_frames: u64,
        maximum_record_bytes: u64,
        maximum_total_bytes: u64,
    ) -> Result<AssignmentJournalExtentV2, PositiveRowErrorV2> {
        measure_assignment_journal_v2(head, open_origin, maximum_frames, maximum_record_bytes, maximum_total_bytes, |ordinal| {
            let key = NativeRowKey::AssignmentJournalFrame { assignment: head.assignment, ordinal };
            match self.required(key)? {
                NativeRowBody::AssignmentJournalFrame(frame) => Ok(frame),
                _ => Err(PositiveRowErrorV2::WrongRowKind),
            }
        })
    }

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

#[cfg(test)]
mod assignment_journal_extent_tests {
    use super::*;
    use misaka_palw_positive_contract::assignment::journal::{AssignmentJournalEventKindV2, AssignmentJournalHeaderV2};

    fn frame(ordinal: u64, previous: Digest, kind: AssignmentJournalEventKindV2, payload_bytes: u32) -> AssignmentJournalFrameRow {
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([1; 64]),
            carrier_block: NativeHash64([2; 64]),
            carrier_transaction: NativeHash64([3; 64]),
            accepted_tx_ordinal: ordinal as u32,
            event_ordinal: 0,
            accepted_daa: 10,
            accepted_blue_score: 11,
        };
        let payload_digest = Digest([4; 64]);
        AssignmentJournalFrameRow {
            header: AssignmentJournalHeaderV2 {
                version: 2,
                revision: 1,
                assignment: AssignmentId(Digest([5; 64])),
                ordinal,
                previous,
                kind,
                origin,
                payload_digest,
                payload_bytes,
            },
            payload: AcceptedMaterial { origin, digest: payload_digest },
        }
    }

    #[test]
    fn exact_history_sizes_and_links_bound_the_rooted_walk() {
        let open = frame(0, Digest([0; 64]), AssignmentJournalEventKindV2::Open, 100);
        let bind = frame(1, open.header.id(), AssignmentJournalEventKindV2::BindResult, 200);
        let first = open.append_to(None, 100).unwrap();
        let head = bind.append_to(Some(&first), 100).unwrap();
        let header_bytes = ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64;
        let total = 2 * header_bytes + 300;
        let largest = header_bytes + 200;
        let mut reads = 0;
        let actual = measure_assignment_journal_v2(&head, open.header.origin, 2, largest, total, |n| {
            reads += 1;
            Ok([open, bind][n as usize])
        })
        .unwrap();
        assert_eq!(actual, AssignmentJournalExtentV2 { record_bytes: total, largest_record_bytes: largest });
        assert_eq!(reads, 2);
        assert!(measure_assignment_journal_v2(&head, open.header.origin, 1, largest, total, |_| unreachable!()).is_err());
        assert!(
            measure_assignment_journal_v2(&head, open.header.origin, 2, largest, total - 1, |n| Ok([open, bind][n as usize])).is_err()
        );
        assert!(
            measure_assignment_journal_v2(&head, open.header.origin, 2, largest - 1, total, |n| Ok([open, bind][n as usize])).is_err()
        );
        let mut detached = bind;
        detached.header.previous = Digest([9; 64]);
        assert!(
            measure_assignment_journal_v2(&head, open.header.origin, 2, largest, total, |n| Ok([open, detached][n as usize])).is_err()
        );
        assert!(
            measure_assignment_journal_v2(&head, open.header.origin, 2, largest, total, |n| {
                if n == 0 { Ok(open) } else { Err(PositiveRowErrorV2::MissingRequiredRow) }
            })
            .is_err()
        );
    }
}
