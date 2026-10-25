//! RFC-0006 caller workspace of the Gate C certification sweep (RFC-0018 §7).
//!
//! `sweep_security_certification_v1` snapshots the whole kernel-route aux 136 range — a growing
//! `Vec<(Vec<u8>, Vec<u8>)>` with one deep key clone and one deep row-bytes clone per row — before
//! its first write, and keeps that snapshot until the loop ends. The quote below is computed from the
//! actual borrowed rows BEFORE that snapshot is constructed and is admitted into the transition's
//! construction owner, which holds it past the snapshot's last use (until the builder drops).
//! A refusal is the typed retryable local `NativeResource`, never a semantic certification outcome.
//!
//! Not covered here (kept explicit): per-row transients inside one loop iteration (the decoded row's
//! `draws`/`attestations`, the claim clone, the draw pool) and the writers' own budgets (those writers
//! quote themselves).
use super::super::native_writers::NativeWriteQuoteV1;
use super::*;

pub(super) fn quote(state: &PalwChainStateV2) -> Result<NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = NativeWriteQuoteV1::new("Gate C certification sweep aux-136 row snapshot");
    let Some(route) = state.kernel_route.as_ref() else { return Ok(q) };
    let table = PALW_SECURITY_CERTIFICATION_TABLE_V1;
    let mut rows = 0u64;
    for ((_, key), bytes) in route.aux.range((table, Vec::new())..(table + 1, Vec::new())) {
        rows = rows.checked_add(1).ok_or_else(|| {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Vec<u8>>(
                "Gate C certification sweep row count",
            ))
        })?;
        // `Vec<u8>::clone` allocates exactly its length.
        q.exact::<u8>(key.len() as u64)?.exact::<u8>(bytes.len() as u64)?;
    }
    // The collected outer Vec grows from an unknown size hint: old and replacement buffers coexist.
    q.grown::<(Vec<u8>, Vec<u8>)>(rows)?.grown::<(Vec<u8>, Vec<u8>)>(rows)?;
    Ok(q)
}
