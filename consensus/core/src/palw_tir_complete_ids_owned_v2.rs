//! Complete-slice original ID authentication with separate typed local refusal.
//! No source/claim authority is recovered from serialized IDs.
use super::{PalwTirPublicAuthenticatedIdsV1, PalwTirPublicSourceV1};
use crate::{
    Hash64,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1,
    },
};
type Fault = NativeDecodeErrorV1<String>;
fn overflow() -> Fault {
    Fault::Resource(NativeCoverageV1::overflow::<PalwTirPublicAuthenticatedIdsV1>("complete original IDs"))
}
/// Test observations only: distinguish a real authentication-stage refusal from
/// an earlier builder refusal without injecting data, authority or a fake error.
#[cfg(test)]
thread_local! { static OBSERVED:std::cell::Cell<(u64,u64)>=const{std::cell::Cell::new((0,0))}; }
#[cfg(test)]
pub(crate) fn reset_observed_v2() {
    OBSERVED.with(|c| c.set((0, 0)));
}
#[cfg(test)]
pub(crate) fn observed_v2() -> (u64, u64) {
    OBSERVED.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! { static BORROWED_OBSERVED:std::cell::Cell<(u64,u64)>=const{std::cell::Cell::new((0,0))}; }
#[cfg(test)]
pub(crate) fn reset_borrowed_observed_v2() {
    BORROWED_OBSERVED.with(|c| c.set((0, 0)));
}
#[cfg(test)]
pub(crate) fn borrowed_observed_v2() -> (u64, u64) {
    BORROWED_OBSERVED.with(std::cell::Cell::get)
}

impl PalwTirPublicSourceV1 {
    fn complete_ids_eof_v2(&self, ids: &[u32]) -> Result<(), Fault> {
        self.trace().map_err(Fault::Decode)?;
        if ids.is_empty() || ids.len() != self.decode_count_v1() as usize {
            return Err(Fault::Decode("complete original IDs do not reach exact nonempty EOF".into()));
        }
        Ok(())
    }
    /// Borrowed identity authentication for an actual retained native source.
    /// It supplies no stored context, admission, approval or authority.
    pub fn validate_complete_ids_classified_v2(&self, rows_root: Hash64, ids: &[u32]) -> Result<(), Fault> {
        self.complete_ids_eof_v2(ids)?;
        #[cfg(test)]
        BORROWED_OBSERVED.with(|c| {
            let (a, b) = c.get();
            c.set((a + 1, b));
        });
        let mut scratch = TirNativeRequestsV1::new();
        // tiled_logits_outer_root_v1 materializes exactly4 ID bytes per token.
        let id_bytes = (ids.len() as u64).checked_mul(std::mem::size_of::<u32>() as u64).ok_or_else(overflow)?;
        usize::try_from(id_bytes).map_err(|_| overflow())?;
        scratch.exact_v1::<u8>(id_bytes).map_err(Fault::Resource)?;
        // PalwJobContextV2::context_hash CanonicalWriter: u16 version,
        // u32 network length + network bytes, eleven Hash64 fields (three job
        // identities and eight model/runtime/shape/trace/CU/tokenizer/prompt
        // identities), fixed32 seed, then three u32 counts. Writer starts512;
        // old/new geometric buffers are separate from retained IDs.
        let fixed = std::mem::size_of::<u16>()
            .checked_add(std::mem::size_of::<u32>())
            .and_then(|n| n.checked_add(11 * std::mem::size_of::<Hash64>()))
            .and_then(|n| n.checked_add(32))
            .and_then(|n| n.checked_add(3 * std::mem::size_of::<u32>()))
            .ok_or_else(overflow)?;
        let span = fixed.checked_add(self.header.job_context.network_id.len()).ok_or_else(overflow)?;
        let envelope = span.max(512).checked_next_power_of_two().ok_or_else(overflow)?;
        scratch.exact_v1::<u8>(envelope as u64).map_err(Fault::Resource)?;
        scratch.exact_v1::<u8>(envelope as u64).map_err(Fault::Resource)?;
        let _scratch = reserve_native_requests_v1(scratch.bytes_v1()).map_err(Fault::Resource)?;
        #[cfg(test)]
        BORROWED_OBSERVED.with(|c| {
            let (a, b) = c.get();
            c.set((a, b + 1));
        });
        if crate::palw_step_refute::tiled_logits_outer_root_v1(
            &self.header.job_context,
            u64::from(self.decode_count_v1()),
            &rows_root,
            ids,
        ) != self.header.full_logits_trace_root
        {
            return Err(Fault::Decode("complete IDs do not authenticate the original trace root".into()));
        }
        Ok(())
    }
    /// Same exact EOF/root as the legacy assembler, with one retained owned Vec.
    pub fn retain_complete_ids_classified_v2(
        &self,
        rows_root: Hash64,
        ids: &[u32],
    ) -> Result<NativeRetainedV1<PalwTirPublicAuthenticatedIdsV1>, Fault> {
        self.complete_ids_eof_v2(ids)?;
        #[cfg(test)]
        OBSERVED.with(|c| {
            let (a, b) = c.get();
            c.set((a + 1, b));
        });
        let mut owned = TirNativeRequestsV1::new();
        owned.exact_v1::<u32>(ids.len() as u64).map_err(Fault::Resource)?;
        let persistent = reserve_native_requests_v1(owned.bytes_v1()).map_err(Fault::Resource)?;
        self.validate_complete_ids_classified_v2(rows_root, ids)?;
        #[cfg(test)]
        OBSERVED.with(|c| {
            let (a, b) = c.get();
            c.set((a, b + 1));
        });
        let mut retained = Vec::new();
        retained.try_reserve_exact(ids.len()).map_err(|_| {
            Fault::Resource(NativeCoverageV1::unsupported::<Vec<u32>>("complete original IDs", "owned ID allocation refused"))
        })?;
        retained.extend_from_slice(ids);
        Ok(NativeRetainedV1::from_owner_reserved_v1(
            PalwTirPublicAuthenticatedIdsV1 { source_id: self.source_id, rows_root, ids: retained },
            persistent,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_public_source_v1::{PalwTirPublicIdsAssemblerV1, PalwTirPublicRequestV1, PalwTirPublicResponseV1};
    fn fixture() -> (PalwTirPublicSourceV1, Hash64, Vec<u32>) {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let source = PalwTirPublicSourceV1::new_v1(
            &record,
            &x.binding.class.layout,
            &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding),
            x.binding.committed_execution_root,
            x.binding.full_logits_trace_root,
            1 << 40,
        )
        .unwrap();
        let root = crate::palw_step_refute::tiled_logits_rows_root_v1(&x.binding.job_context, &x.rows).unwrap();
        (source, root, x.generated)
    }
    #[test]
    fn complete_ids_owned_preserves_exact_legacy_eof_trace_and_source() {
        let (source, root, ids) = fixture();
        let old = PalwTirPublicIdsAssemblerV1::default()
            .push_v1(
                &source,
                &PalwTirPublicRequestV1::Ids { offset: 0 },
                &PalwTirPublicResponseV1::Ids { rows_root: root, ids: ids.clone() },
            )
            .unwrap()
            .unwrap();
        let retained = source.retain_complete_ids_classified_v2(root, &ids).unwrap();
        assert_eq!(*retained, old);
        assert_eq!(retained.source_id_v1(), source.source_id_v1());
        assert!(matches!(source.retain_complete_ids_classified_v2(root, &ids[..ids.len() - 1]), Err(Fault::Decode(_))));
        let mut longer = ids.clone();
        longer.push(0);
        assert!(matches!(source.retain_complete_ids_classified_v2(root, &longer), Err(Fault::Decode(_))));
        assert!(matches!(source.retain_complete_ids_classified_v2(Hash64::default(), &ids), Err(Fault::Decode(_))));
        let mut changed = ids.clone();
        changed[0] = changed[0].wrapping_add(1);
        assert!(matches!(source.retain_complete_ids_classified_v2(root, &changed), Err(Fault::Decode(_))));
    }
    #[test]
    fn complete_ids_owned_actual_quota_refusal_retry_and_retained_release() {
        const CHILD: &str = "MISAKA_COMPLETE_IDS_QUOTA_CHILD";
        if std::env::var(CHILD).as_deref() != Ok("1") {
            let full = format!("{}::complete_ids_owned_actual_quota_refusal_retry_and_retained_release", module_path!());
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                    .env(CHILD, "1")
                    .status()
                    .unwrap()
                    .success()
            );
            return;
        }
        let (source, root, ids) = fixture();
        const LOCAL_BYTES: u64 = 1 << 20;
        crate::palw_native_resource_v1::arm_native_logical_limits_v1(Some(
            crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: LOCAL_BYTES, database_bytes: 0 },
        ));
        let blocker = reserve_native_requests_v1(LOCAL_BYTES).unwrap();
        reset_observed_v2();
        assert!(matches!(source.retain_complete_ids_classified_v2(root, &ids), Err(Fault::Resource(_))));
        assert_eq!(observed_v2(), (1, 0));
        drop(blocker);
        let retained = source.retain_complete_ids_classified_v2(root, &ids).unwrap();
        assert_eq!(retained.ids_v1(), ids);
        // Only persistent Vec<u32> storage survives; hash/context scratch has
        // already returned its actual native demand before the method returns.
        let persistent = (ids.len() * std::mem::size_of::<u32>()) as u64;
        let rest = reserve_native_requests_v1(LOCAL_BYTES - persistent).unwrap();
        assert!(reserve_native_requests_v1(1).is_err());
        drop(retained);
        let released = reserve_native_requests_v1(persistent).unwrap();
        drop(released);
        drop(rest);
        assert!(matches!(source.retain_complete_ids_classified_v2(Hash64::default(), &ids), Err(Fault::Decode(_))));
        assert!(reserve_native_requests_v1(LOCAL_BYTES).is_ok(), "semantic failure returns persistent and scratch tickets");
    }
}
