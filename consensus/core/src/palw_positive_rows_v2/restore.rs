//! Structural restore checks for the canonical aux151 schemas. Decode and
//! cross-reference success is NOT accepted-origin ancestry, arithmetic replay,
//! receipt eligibility, a live funding lock or consensus-credit authority.
//!
//! Values contain fixed arrays only. Keep one current row and a small constant
//! number of referenced rows in scratch; never clone the entire decoded enum
//! into a map. The only index is one fixed (case, attempt) key per Witness, to
//! reject ordinal reuse without quadratic scans. Native restore admission must
//! reserve that BTreeSet's bounded node storage separately from encoded rows.
use super::PositiveRowErrorV2;
use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    messages::{ClaimStatus, MinerDataStatus, SourceKind},
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::*,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Included, Unbounded};

pub type PositiveAuxRowsV2 = BTreeMap<(u8, Vec<u8>), Vec<u8>>;

/// Diagnostic/charging DTO only. It grants no native acceptance or credit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PositiveRestoreReportV2 {
    pub rows: u64,
    pub cases: u64,
    pub witnesses: u64,
    pub decoded_rows: u64,
    pub decoded_row_bytes: u64,
    pub reference_reads: u64,
    pub signature_checks: u64,
    pub witness_attempt_entries: u64,
}
fn add(value: &mut u64, n: u64) -> Result<(), PositiveRowErrorV2> {
    *value = value.checked_add(n).ok_or(ContractError::Budget)?;
    Ok(())
}
fn inconsistent<T>() -> Result<T, PositiveRowErrorV2> {
    Err(PositiveRowErrorV2::InconsistentLinks)
}

/// A local contradiction check, never a substitute for accepted-history
/// ancestry. Transaction/event ordinals are comparable only inside the same
/// accepting block; its accepted clock is invariant across all its events.
fn same_block_event_order_v2(before: &AcceptedOrigin, after: &AcceptedOrigin, strict: bool) -> Result<(), PositiveRowErrorV2> {
    if before.accepting_block != after.accepting_block {
        return Ok(());
    }
    let first = (before.accepted_tx_ordinal, before.event_ordinal);
    let second = (after.accepted_tx_ordinal, after.event_ordinal);
    if before.accepted_daa != after.accepted_daa
        || before.accepted_blue_score != after.accepted_blue_score
        || first > second
        || (strict && first == second)
    {
        return inconsistent();
    }
    Ok(())
}

/// Invoke BEFORE a carriage's typed Vec is collected into a BTreeMap. Map
/// decoding must never erase a repeated Positive row with last-wins behavior.
/// This index borrows the encoded keys; it does not copy decoded row bodies.
/// Existing non-Positive aux semantics are left to their own restore owners.
pub fn validate_positive_aux_uncollected_v2(rows: &[(u8, Vec<u8>, Vec<u8>)]) -> Result<(), PositiveRowErrorV2> {
    let mut seen = BTreeSet::<&[u8]>::new();
    for (table, key, value) in rows {
        if *table != KERNEL_AUX_TABLE {
            continue;
        }
        NativeRowValue::decode(key, value)?;
        if !seen.insert(key.as_slice()) {
            return inconsistent();
        }
    }
    Ok(())
}

struct Reader<'a> {
    aux: &'a PositiveAuxRowsV2,
    report: PositiveRestoreReportV2,
}
impl Reader<'_> {
    fn decode(&mut self, key: &[u8], bytes: &[u8]) -> Result<NativeRowBody, PositiveRowErrorV2> {
        add(&mut self.report.decoded_rows, 1)?;
        add(&mut self.report.decoded_row_bytes, bytes.len() as u64)?;
        Ok(NativeRowValue::decode(key, bytes)?.body)
    }
    fn required(&mut self, key: NativeRowKey) -> Result<NativeRowBody, PositiveRowErrorV2> {
        add(&mut self.report.reference_reads, 1)?;
        let key = key.encode();
        let aux = self.aux;
        let bytes = aux.get(&(KERNEL_AUX_TABLE, key.clone())).ok_or(PositiveRowErrorV2::MissingRequiredRow)?;
        self.decode(&key, bytes)
    }
    fn result(&mut self, id: ResultId) -> Result<ResultRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::Result(id))? {
            NativeRowBody::Result(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn index(&mut self, id: ClaimId) -> Result<ClaimIndexRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::ClaimIndex(id))? {
            NativeRowBody::ClaimIndex(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn case(&mut self, id: CaseId) -> Result<CaseRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::Case(id))? {
            NativeRowBody::Case(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn witness(&mut self, id: WitnessId) -> Result<WitnessRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::Witness(id))? {
            NativeRowBody::Witness(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn coverage(&mut self, id: WitnessId) -> Result<CoverageRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::Coverage(id))? {
            NativeRowBody::Coverage(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn funding(&mut self, id: CaseId) -> Result<FundingRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::Funding(id))? {
            NativeRowBody::Funding(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn union(&mut self, case: CaseId, kind: SourceKind, source: Digest) -> Result<SourceUnionRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::SourceUnion { case, kind, source })? {
            NativeRowBody::SourceUnion(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn journal(&mut self, id: CaseId) -> Result<JournalHeadRow, PositiveRowErrorV2> {
        match self.required(NativeRowKey::JournalHead(id))? {
            NativeRowBody::JournalHead(r) => Ok(r),
            _ => Err(PositiveRowErrorV2::WrongRowKind),
        }
    }
    fn journal_links(&self, head: &JournalHeadRow, case: &CaseRow, result: &ResultRow) -> Result<(), PositiveRowErrorV2> {
        // Native sequence includes ResultAdmission, hence its initial value 1.
        // Private replay archives' initial records=0 has a DIFFERENT meaning.
        if head.case != case.case
            || head.sequence == 0
            || head.history.origin != case.last_event
            || (head.sequence == 1 && (head.previous != Digest([0; 64]) || head.history.origin != result.origin))
        {
            return inconsistent();
        }
        // head/history.digest domains and accepted preimages are resolved by
        // the actual accepted-history backend. Do not alias one to the other.
        Ok(())
    }
    fn case_union_status(&mut self, case: &CaseRow) -> Result<(), PositiveRowErrorV2> {
        let mut prefix =
            NativeRowKey::SourceUnion { case: case.case, kind: SourceKind::ArtifactInventory, source: Digest([0; 64]) }.encode();
        // Kind and source follow the case. Both SourceKind variants are in this
        // one lexicographic prefix; no source-catalog membership is inferred.
        prefix.truncate(prefix.len() - 65);
        let mut requested = false;
        let mut complete = true;
        let aux = self.aux;
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.clone())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            add(&mut self.report.reference_reads, 1)?;
            let NativeRowBody::SourceUnion(union) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if union.case != case.case {
                return inconsistent();
            }
            requested |= union.requested_units != 0;
            complete &= union.completed_units == union.requested_units;
        }
        if match case.miner_data {
            MinerDataStatus::NotRequested => requested,
            MinerDataStatus::Pending => !requested || complete,
            MinerDataStatus::Complete => !requested || !complete,
        } {
            return inconsistent();
        }
        Ok(())
    }
    fn union_receipts(&mut self, union: &SourceUnionRow) -> Result<(), PositiveRowErrorV2> {
        let mut prefix = NativeRowKey::ServiceReceipt { case: union.case, kind: union.kind, source: union.source, unit: 0 }.encode();
        prefix.truncate(prefix.len() - 8);
        let mut completed = 0u64;
        let aux = self.aux;
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.clone())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            add(&mut self.report.reference_reads, 1)?;
            let NativeRowBody::ServiceReceipt(receipt) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            if receipt.case != union.case
                || receipt.kind != union.kind
                || receipt.source != union.source
                || receipt.unit >= union.source_units
            {
                return inconsistent();
            }
            add(&mut completed, 1)?;
        }
        // Native v2 has no receipt-only pruning rule. Cache counts cannot
        // survive while the literal once-only receipts have disappeared.
        if completed != union.completed_units {
            return inconsistent();
        }
        Ok(())
    }
    // Keep each kind's large fixed-array temporaries in its own frame. In
    // particular a legacy restore with no aux151 must never reserve all ten
    // branches' debug-build stack slots merely by calling this checker.
    #[inline(never)]
    fn validate_row(&mut self, row: &NativeRowBody, attempts: &mut BTreeSet<(CaseId, u32)>) -> Result<(), PositiveRowErrorV2> {
        match row {
            NativeRowBody::Result(row) => self.validate_result_row(row),
            NativeRowBody::ClaimIndex(row) => self.validate_claim_index_row(row),
            NativeRowBody::Case(row) => self.validate_case_row(row),
            NativeRowBody::Witness(row) => self.validate_witness_row(row, attempts),
            NativeRowBody::Coverage(row) => self.validate_coverage_row(row),
            NativeRowBody::SourceUnion(row) => self.validate_source_union_row(row),
            NativeRowBody::ServiceReceipt(row) => self.validate_service_receipt_row(row),
            NativeRowBody::Funding(row) => self.validate_funding_row(row),
            NativeRowBody::PacketReceipt(row) => self.validate_packet_receipt_row(row),
            NativeRowBody::JournalHead(row) => self.validate_journal_head_row(row),
        }
    }
    #[inline(never)]
    fn validate_result_row(&mut self, row: &ResultRow) -> Result<(), PositiveRowErrorV2> {
        let index = self.index(row.result.claim_id())?;
        let case = self.case(row.result.case_id())?;
        if index.result != row.result.id() || index.case != case.case || case.result != row.result.id() {
            return inconsistent();
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_claim_index_row(&mut self, row: &ClaimIndexRow) -> Result<(), PositiveRowErrorV2> {
        let result = self.result(row.result)?;
        let case = self.case(row.case)?;
        if row.claim != result.result.claim_id() || row.case != result.result.case_id() || case.result != row.result {
            return inconsistent();
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_case_row(&mut self, row: &CaseRow) -> Result<(), PositiveRowErrorV2> {
        add(&mut self.report.cases, 1)?;
        let result = self.result(row.result)?;
        let index = self.index(result.result.claim_id())?;
        let funding = self.funding(row.case)?;
        let head = self.journal(row.case)?;
        validate_bundle(&index, &result, row, &funding)?;
        add(&mut self.report.signature_checks, 2)?;
        self.journal_links(&head, row, &result)?;
        self.case_union_status(row)?;
        if let Some(id) = row.positive_witness {
            let witness = self.witness(id)?;
            let coverage = self.coverage(id)?;
            if witness.witness.case != row.case
                || witness.witness.result != row.result
                || coverage.case != row.case
                || coverage.result != row.result
                || (row.status == ClaimStatus::PositivePassed
                    && (coverage.state != CoverageState::Matched || witness.witness.candidate_output != result.result.output))
            {
                return inconsistent();
            }
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_witness_row(&mut self, row: &WitnessRow, attempts: &mut BTreeSet<(CaseId, u32)>) -> Result<(), PositiveRowErrorV2> {
        add(&mut self.report.witnesses, 1)?;
        let case = self.case(row.witness.case)?;
        let result = self.result(row.witness.result)?;
        let coverage = self.coverage(row.witness.id())?;
        same_block_event_order_v2(&result.origin, &row.origin, true)?;
        validate_witness_bundle(&result, &case, row, &coverage)?;
        add(&mut self.report.signature_checks, 1)?;
        if row.witness.attempt >= case.usage.witness_attempts || !attempts.insert((case.case, row.witness.attempt)) {
            return inconsistent();
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_coverage_row(&mut self, row: &CoverageRow) -> Result<(), PositiveRowErrorV2> {
        let witness = self.witness(row.witness)?;
        let case = self.case(row.case)?;
        if row.case != witness.witness.case
            || row.result != witness.witness.result
            || row.case != case.case
            || row.result != case.result
            || row.plan != case.coverage.plan
            || row.mandatory_relations != case.coverage.mandatory_relations
        {
            return inconsistent();
        }
        same_block_event_order_v2(&witness.origin, &row.last_event, false)?;
        if row.next_relation > 0 {
            self.coverage_packet_order(row)?;
        }
        Ok(())
    }
    #[inline(never)]
    fn coverage_packet_order(&mut self, coverage: &CoverageRow) -> Result<(), PositiveRowErrorV2> {
        let mut prefix = NativeRowKey::PacketReceipt { witness: coverage.witness, first_relation: 0 }.encode();
        prefix.truncate(prefix.len() - 8);
        let aux = self.aux;
        // Each Coverage owns a disjoint fixed witness prefix. Across the
        // restore, each literal packet is visited once more, without a decoded
        // row map or an inference that the cached cursor grants authority.
        for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, prefix.clone())), Unbounded)) {
            if *table != KERNEL_AUX_TABLE || !key.starts_with(&prefix) {
                break;
            }
            add(&mut self.report.reference_reads, 1)?;
            let NativeRowBody::PacketReceipt(packet) = self.decode(key, value)? else {
                return Err(PositiveRowErrorV2::WrongRowKind);
            };
            same_block_event_order_v2(&packet.origin, &coverage.last_event, false)?;
        }
        // Missing packet/history preimages and inter-block ordering remain the
        // canonical accepted-history backend's responsibility, not a cursor
        // or this local ordinal check's authorization.
        Ok(())
    }
    #[inline(never)]
    fn validate_source_union_row(&mut self, row: &SourceUnionRow) -> Result<(), PositiveRowErrorV2> {
        self.case(row.case)?;
        self.union_receipts(row)?;

        Ok(())
    }
    #[inline(never)]
    fn validate_service_receipt_row(&mut self, row: &ServiceReceiptRow) -> Result<(), PositiveRowErrorV2> {
        same_block_event_order_v2(&row.duty.origin, &row.response.origin, true)?;
        if let Some(settlement) = &row.settlement {
            same_block_event_order_v2(&row.response.origin, &settlement.origin, false)?;
        }
        let case = self.case(row.case)?;
        let union = self.union(row.case, row.kind, row.source)?;
        if row.result != case.result || row.unit >= union.source_units || union.completed_units == 0 {
            return inconsistent();
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_funding_row(&mut self, row: &FundingRow) -> Result<(), PositiveRowErrorV2> {
        let case = self.case(row.consent.case)?;
        if case.service.funding_consent != row.consent.id().0 {
            return inconsistent();
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_packet_receipt_row(&mut self, row: &PacketReceiptRow) -> Result<(), PositiveRowErrorV2> {
        let witness = self.witness(row.witness)?;
        same_block_event_order_v2(&witness.origin, &row.origin, true)?;
        let coverage = self.coverage(row.witness)?;
        if row.author != witness.witness.author || row.first_relation >= coverage.mandatory_relations {
            return inconsistent();
        }

        Ok(())
    }
    #[inline(never)]
    fn validate_journal_head_row(&mut self, row: &JournalHeadRow) -> Result<(), PositiveRowErrorV2> {
        let case = self.case(row.case)?;
        let result = self.result(case.result)?;
        self.journal_links(row, &case, &result)?;

        Ok(())
    }
}

/// Validate every aux151 row and both directions of all stored identity links.
/// AcceptedMaterial references and ancestry still require the actual native
/// backend; cached Matched/usage/union summaries cannot mint any authority here.
pub fn validate_positive_aux_restore_v2(aux: &PositiveAuxRowsV2) -> Result<PositiveRestoreReportV2, PositiveRowErrorV2> {
    let first = aux.range((Included((KERNEL_AUX_TABLE, Vec::new())), Unbounded)).next();
    if !first.is_some_and(|((table, _), _)| *table == KERNEL_AUX_TABLE) {
        return Ok(PositiveRestoreReportV2::default());
    }
    validate_positive_aux_present_v2(aux)
}

#[inline(never)]
fn validate_positive_aux_present_v2(aux: &PositiveAuxRowsV2) -> Result<PositiveRestoreReportV2, PositiveRowErrorV2> {
    let mut reader = Reader { aux, report: PositiveRestoreReportV2::default() };
    let mut attempts = BTreeSet::<(CaseId, u32)>::new();
    for ((table, key), value) in aux.range((Included((KERNEL_AUX_TABLE, Vec::new())), Unbounded)) {
        if *table != KERNEL_AUX_TABLE {
            break;
        }
        add(&mut reader.report.rows, 1)?;
        let row = reader.decode(key, value)?;
        reader.validate_row(&row, &mut attempts)?;
    }
    reader.report.witness_attempt_entries = attempts.len() as u64;
    Ok(reader.report)
}

#[cfg(test)]
mod accepted_order_tests;
#[cfg(test)]
mod tests;
