//! Local source projection of A6's complete case journal.
//!
//! A decoded record is data, never accepted-prefix or role authority. The
//! caller must hold A6's actual historical-step guard, authenticate the full
//! record and its canonical preimages, and supply the resolved source event
//! through the mandatory NativeSourceContextV2. This helper neither reads
//! native state nor appends a complete case journal or installs native rows.
use super::*;
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    journal::{CaseJournalEventKind, CaseJournalRecordView},
};

impl NativeSourceLedgerV2 {
    /// Prepare one meaningful op3 from a complete, independently authenticated
    /// case-journal step. Bind/refresh the reconstructed GLOBAL Case row first;
    /// source-only replay cannot replace interleaved Court/funding history.
    /// A6 must include all existing source preparation work/heap in its native
    /// replay quote before calling this method. No new buffer is allocated by
    /// this framing check. The caller owns local apply and committed-row CAS.
    pub fn prepare_journal_request_v2(
        &self,
        geometry: &TirSourceGeometryV2,
        context: &impl NativeSourceContextV2,
        record: CaseJournalRecordView<'_>,
        event: SourceRequestEventV2<'_>,
    ) -> Result<PreparedSourceUpdateV2, SourceErrorV2> {
        self.check_journal_source(record, event.material, event.native_body, 3)?;
        require_meaningful(self.prepare_request(geometry, context, event)?)
    }

    /// Prepare one meaningful op4 under the same historical-step contract as
    /// prepare_journal_request_v2. The provider must come from the actual
    /// carrier/delegation resolver, not from the journal framing or raw G14.
    /// A repeated completed unit remains valid as a delivery but is rejected
    /// as a NEW case-journal event, without changing the first receipt or due.
    pub fn prepare_journal_response_v2(
        &self,
        geometry: &TirSourceGeometryV2,
        context: &impl NativeSourceContextV2,
        record: CaseJournalRecordView<'_>,
        event: SourceResponseEventV2<'_>,
    ) -> Result<PreparedSourceUpdateV2, SourceErrorV2> {
        self.check_journal_source(record, event.material, event.native_body, 4)?;
        require_meaningful(self.prepare_response(geometry, context, event)?)
    }

    fn check_journal_source(
        &self,
        record: CaseJournalRecordView<'_>,
        material: AcceptedMaterial,
        native_body: &[u8],
        operation: u8,
    ) -> Result<(), SourceErrorV2> {
        let case = self.case_basis.as_ref().ok_or(SourceErrorV2::MissingPort(SourceMissingPortV2::CanonicalHistory))?;
        let header = record.header();
        // Sequence 1 must be the actual Result admission, never a source op.
        if header.sequence <= 1 {
            return Err(SourceErrorV2::History);
        }
        let mut prefix = [0u8; 5];
        prefix[..2].copy_from_slice(&CONTRACT_VERSION.to_le_bytes());
        prefix[2..4].copy_from_slice(&CONTRACT_REVISION.to_le_bytes());
        prefix[4] = operation;
        if header.scope != self.result.scope
            || header.case != case.case
            || header.result != case.result
            || header.kind != CaseJournalEventKind::NativeBody
            || record.payload().get(..5) != Some(prefix.as_slice())
            || record.payload() != native_body
            || header.origin != material.origin
            || material.origin.event_ordinal != 0
            || material.digest.0 != material.origin.carrier_transaction.0
        {
            return Err(SourceErrorV2::Binding);
        }
        // record.history() has the FULL-record history domain. In contrast,
        // the source-only material is the actual carrier TxID under A0's
        // op3/op4-specific rule. Neither is a computation-validity argument.
        Ok(())
    }
}

fn require_meaningful(update: PreparedSourceUpdateV2) -> Result<PreparedSourceUpdateV2, SourceErrorV2> {
    if !update.changes_case() {
        return Err(SourceErrorV2::History);
    }
    Ok(update)
}
