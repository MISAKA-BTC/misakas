//! New paid initial transport metadata. Old class/proof pricing is unchanged.
use super::*;
pub fn derive_initial_metadata_v31(bytes: u64, parts: u16, start: bool) -> Result<DerivedOrdinaryMetadataV11, String> {
    if bytes == 0 || bytes > 16 << 20 || parts == 0 || parts > 512 {
        return Err("initial31 fixed ordinary tariff bound".into());
    }
    let held = if start { bytes.checked_mul(6).and_then(|n| n.checked_add(parts as u64 * 2048)) } else { bytes.checked_add(4096) }
        .ok_or("initial31 full retained/assembly copies overflow")?;
    let work = (bytes as u128).checked_mul(1024).ok_or("initial31 bounded decode work overflow")?;
    let mut q = action_envelope(work, held, None)?;
    // The same original 21-axis aggregate accounts for a complete buffer, not
    // merely the small Start frame. Consumed metadata is never recredited.
    q.set(ResourceAxisV10::OwnedPeakBytes, held as u128);
    q.set(ResourceAxisV10::CourtScratchBytes, held as u128);
    q.set(ResourceAxisV10::StoredEvidenceBytes, held as u128);
    Ok(DerivedOrdinaryMetadataV11 { quantities: q })
}
