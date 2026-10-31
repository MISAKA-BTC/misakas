//! Private source-row part of the native one-writer transaction. This is not
//! an event acceptor: A4 must resolve the actual accepted prefix, role, live
//! funding, reservation and schedule, then re-prepare using that SAME view.
//! A5 money and A6 complete-case journal updates belong in the outer checkpoint
//! and UTXO batch as well. No public row setter is exported by this module.
use super::*;
use crate::{
    palw_positive_rows_v2::PositiveRowsV2,
    palw_positive_source_v2::{NativeSourceLedgerV2, PreparedSourceUpdateV2, SourceEffectV2},
};
use misaka_palw_positive_contract::{
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{NativeRowBody, NativeRowValue},
};

fn source_refusal(detail: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("Positive source row transition refused: {detail:?}"))
}

impl TransitionBuilder<'_> {
    /// Pure prepared data is insufficient to call this from outside the fold.
    /// The actual caller still owns all native authorization and the combined
    /// source+money+journal transaction. A failure here restores its row delta.
    pub(super) fn apply_positive_source_prepared_v2(
        &mut self,
        ledger: &NativeSourceLedgerV2,
        update: &PreparedSourceUpdateV2,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.kernel_route.is_none() {
            return Err(source_refusal("kernel route header missing"));
        }
        ledger.validate_prepared(update).map_err(source_refusal)?;
        let expected = update.expected_case().ok_or_else(|| source_refusal("actual current Case row binding missing"))?;
        let next = update.case_row().ok_or_else(|| source_refusal("prepared Case row missing"))?;
        let rows = PositiveRowsV2::new(&self.state);
        let bundle = rows.bundle(ledger.result().claim_id()).map_err(source_refusal)?;
        if &bundle.case != expected || bundle.result.result != *ledger.result() || bundle.case.service != *ledger.service() {
            return Err(source_refusal("stale Result/Case/service"));
        }
        // Source events cannot rewrite witness/coverage, admission or liability.
        let mut permitted = *expected;
        permitted.usage = next.usage;
        permitted.miner_data = next.miner_data;
        permitted.last_event = next.last_event;
        if permitted != *next || next.usage != update.usage() {
            return Err(source_refusal("source transition rewrites another role"));
        }
        NativeRowBody::Case(*next).validate_shape().map_err(source_refusal)?;
        {
            let (kind, source) = update.touched_source();
            let key = NativeRowKey::SourceUnion { case: expected.case, kind, source };
            let old = rows.read(key).map_err(source_refusal)?.map(|row| row.body);
            let expected = ledger.source_union(kind, source).map(NativeRowBody::SourceUnion);
            if old != expected {
                return Err(source_refusal("source union differs from complete replay"));
            }
        }
        if let Some((kind, source, unit)) = update.touched_unit() {
            let key = NativeRowKey::ServiceReceipt { case: expected.case, kind, source, unit };
            let old = rows.read(key).map_err(source_refusal)?.map(|row| row.body);
            let expected = ledger.receipt(kind, source, unit).copied().map(NativeRowBody::ServiceReceipt);
            if old != expected {
                return Err(source_refusal("unit receipt differs from complete replay"));
            }
        }
        if update.effect() == SourceEffectV2::AlreadyApplied {
            return Ok(());
        }
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            self.write_positive_source_row_v2(NativeRowBody::Case(*next))?;
            if let Some(union) = update.union() {
                self.write_positive_source_row_v2(NativeRowBody::SourceUnion(*union))?;
            }
            if let Some(receipt) = update.receipt() {
                self.write_positive_source_row_v2(NativeRowBody::ServiceReceipt(*receipt))?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.restore(checkpoint);
        }
        result
    }

    fn write_positive_source_row_v2(&mut self, body: NativeRowBody) -> Result<(), PalwStateV2Error> {
        // Bound and admit the canonical key before its Vec allocation. The
        // existing typed writer admits the value and map/journal copies before
        // serialization. Missing header was rejected at the transaction entry.
        let key = body.key();
        if self.native_writers_tracking_v1() {
            let bytes = borsh::object_length(&(2u16, 1u16, &key)).map_err(source_refusal)? as u64;
            let mut quote = native_writers::NativeWriteQuoteV1::new("Positive source aux151 key");
            quote.exact::<u8>(bytes)?;
            self.admit_native_write_v1(quote)?;
        }
        let key = key.encode();
        let row = NativeRowValue::new(body).map_err(source_refusal)?;
        self.write_kernel_row_encoded_v1(KERNEL_AUX_TABLE, &key, Some(&row))
    }
}
