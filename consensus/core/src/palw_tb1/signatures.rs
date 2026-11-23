//! Signing preimages for the isolated TB1 integration profile. RoundRef is the
//! owning profile's canonical byte string. These domains have no native wire
//! tag or activation. Acceptance positions come from the ledger, not the signer.
use super::{qualification::Tb1Ready, releases::Tb1HolderRelease};
use libcrux_ml_dsa::ml_dsa_65::{MLDSA65Signature, MLDSA65VerificationKey, portable};

pub const READY_CONTEXT: &[u8] = b"MISAKA/private/TB1/Ready/v1";
pub const RELEASE_CONTEXT: &[u8] = b"MISAKA/private/TB1/ShareRelease/v1";

// Both signed forms have two 64-byte digests and two u32 fields after RoundRef.
const FIXED_SUFFIX: usize = 2 * 64 + 2 * 4;
const SHARE_BYTES: usize = 4 + 66;

fn preimage(round: &[u8], suffix_size: usize, byte_limit: usize) -> Option<Vec<u8>> {
    let len = u32::try_from(round.len()).ok()?;
    let size = 4usize.checked_add(round.len())?.checked_add(suffix_size)?;
    if round.is_empty() || size > byte_limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(round);
    Some(bytes)
}

pub fn ready_message(row: &Tb1Ready<'_>, byte_limit: usize) -> Option<Vec<u8>> {
    let mut bytes = preimage(row.round_ref, FIXED_SUFFIX, byte_limit)?;
    bytes.extend_from_slice(&u32::try_from(row.dealer_slot).ok()?.to_be_bytes());
    bytes.extend_from_slice(&u32::try_from(row.holder_slot).ok()?.to_be_bytes());
    bytes.extend_from_slice(&row.end.transcript_root);
    bytes.extend_from_slice(&row.end.end_anchor);
    Some(bytes)
}

pub(super) fn release_message_size(row: &Tb1HolderRelease<'_>) -> Option<usize> {
    (4 + FIXED_SUFFIX).checked_add(row.context.round_ref.len())?.checked_add(SHARE_BYTES.checked_mul(row.shares.len())?)
}

pub fn release_message(row: &Tb1HolderRelease<'_>, byte_limit: usize) -> Option<Vec<u8>> {
    let count = u32::try_from(row.shares.len()).ok()?;
    let mut bytes =
        preimage(row.context.round_ref, FIXED_SUFFIX.checked_add(SHARE_BYTES.checked_mul(row.shares.len())?)?, byte_limit)?;
    bytes.extend_from_slice(&row.context.qual_root);
    bytes.extend_from_slice(&row.context.freeze_id);
    bytes.extend_from_slice(&u32::try_from(row.holder_slot).ok()?.to_be_bytes());
    bytes.extend_from_slice(&count.to_be_bytes());
    let mut previous = None;
    for &(slot, value) in row.shares {
        if previous.is_some_and(|p| p >= slot) {
            return None;
        }
        previous = Some(slot);
        bytes.extend_from_slice(&u32::try_from(slot).ok()?.to_be_bytes());
        bytes.extend_from_slice(&value.to_bytes());
    }
    Some(bytes)
}

pub(super) fn verify(key: &MLDSA65VerificationKey, signature: &[u8], message: &[u8], domain: &[u8]) -> bool {
    let Ok(raw) = signature.try_into() else { return false };
    portable::verify(key, message, domain, &MLDSA65Signature::new(raw)).is_ok()
}
