//! Internal retained Open payload, DISTINCT from the literal native op8 body.
//! The header payload digest binds this wrapper. Actual accepted transaction
//! lookup must compare `native_body()` byte-for-byte; this is not a carrier.
use crate::{CONTRACT_REVISION, CONTRACT_VERSION, ContractError};
pub const ASSIGNMENT_OPEN_MAGIC_V2: [u8; 8] = *b"PC2AOPEN";
pub const ASSIGNMENT_OPEN_PREIMAGES_V2: usize = 2;
/// Fixed semantic slots; an arbitrary collection cannot stand in for native
/// policy/custody owners. Their actual native-domain checks remain mandatory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum AssignmentOpenPreimageKindV2 {
    ReservationCustody = 0,
    LiabilityCustody = 1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentOpenLimitsV2 {
    pub max_payload_bytes: u64,
    pub max_material_bytes: u64,
    pub max_native_body_bytes: u64,
    pub max_preimage_bytes: u64,
    pub max_total_preimage_bytes: u64,
}
impl AssignmentOpenLimitsV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if [
            self.max_payload_bytes,
            self.max_material_bytes,
            self.max_native_body_bytes,
            self.max_preimage_bytes,
            self.max_total_preimage_bytes,
        ]
        .into_iter()
        .any(|n| n == 0 || n == u64::MAX)
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
    pub fn within_payload(max: u64) -> Self {
        Self {
            max_payload_bytes: max,
            max_material_bytes: max,
            max_native_body_bytes: max,
            max_preimage_bytes: max,
            max_total_preimage_bytes: max,
        }
    }
}
pub struct AssignmentOpenViewV2<'a> {
    material: &'a [u8],
    native_body: &'a [u8],
    preimages: [&'a [u8]; ASSIGNMENT_OPEN_PREIMAGES_V2],
}
fn blob<'a>(raw: &'a [u8], at: &mut usize, max: u64) -> Result<&'a [u8], ContractError> {
    let len_end = at.checked_add(4).ok_or(ContractError::Budget)?;
    let len = u32::from_le_bytes(
        raw.get(*at..len_end).ok_or(ContractError::NonCanonical)?.try_into().map_err(|_| ContractError::NonCanonical)?,
    ) as usize;
    if len == 0 || len as u64 > max {
        return Err(ContractError::Budget);
    }
    let end = len_end.checked_add(len).ok_or(ContractError::Budget)?;
    let bytes = raw.get(len_end..end).ok_or(ContractError::NonCanonical)?;
    *at = end;
    Ok(bytes)
}
impl<'a> AssignmentOpenViewV2<'a> {
    /// Borrowed, length-first parser. No allocating Deserialize or fallback
    /// from a missing preimage to an empty/default policy is provided.
    pub fn decode(raw: &'a [u8], limits: AssignmentOpenLimitsV2) -> Result<Self, ContractError> {
        limits.validate()?;
        if raw.len() as u64 > limits.max_payload_bytes {
            return Err(ContractError::Budget);
        }
        if raw.get(..8) != Some(&ASSIGNMENT_OPEN_MAGIC_V2) || raw.get(8..12) != Some(&[2, 0, 1, 0]) {
            return Err(ContractError::NonCanonical);
        }
        let mut at = 12;
        let material = blob(raw, &mut at, limits.max_material_bytes)?;
        let native_body = blob(raw, &mut at, limits.max_native_body_bytes)?;
        if native_body.get(..5) != Some(&[2, 0, 1, 0, 8]) {
            return Err(ContractError::NonCanonical);
        }
        let mut total = 0u64;
        let mut preimages = [&[][..]; ASSIGNMENT_OPEN_PREIMAGES_V2];
        for bytes in &mut preimages {
            *bytes = blob(raw, &mut at, limits.max_preimage_bytes)?;
            total = total
                .checked_add(bytes.len() as u64)
                .filter(|n| *n <= limits.max_total_preimage_bytes)
                .ok_or(ContractError::Budget)?;
        }
        if at != raw.len() {
            return Err(ContractError::NonCanonical);
        }
        Ok(Self { material, native_body, preimages })
    }
    pub fn material(&self) -> &'a [u8] {
        self.material
    }
    pub fn native_body(&self) -> &'a [u8] {
        self.native_body
    }
    pub fn preimage(&self, kind: AssignmentOpenPreimageKindV2) -> &'a [u8] {
        self.preimages[kind as usize]
    }
    pub fn preimages(&self) -> &[&'a [u8]; ASSIGNMENT_OPEN_PREIMAGES_V2] {
        &self.preimages
    }
}
/// Caller reserves the complete owned payload before invoking this encoder.
/// Native body and all original owner preimages must already be available.
pub fn encode_assignment_open_v2(
    material: &[u8],
    native_body: &[u8],
    preimages: [&[u8]; ASSIGNMENT_OPEN_PREIMAGES_V2],
    limits: AssignmentOpenLimitsV2,
) -> Result<Vec<u8>, ContractError> {
    limits.validate()?;
    if material.is_empty()
        || material.len() as u64 > limits.max_material_bytes
        || native_body.is_empty()
        || native_body.len() as u64 > limits.max_native_body_bytes
        || native_body.get(..5) != Some(&[2, 0, 1, 0, 8])
    {
        return Err(ContractError::Budget);
    }
    let mut total = 0u64;
    for b in preimages {
        if b.is_empty() || b.len() as u64 > limits.max_preimage_bytes {
            return Err(ContractError::Budget);
        }
        total = total.checked_add(b.len() as u64).ok_or(ContractError::Budget)?;
    }
    if total > limits.max_total_preimage_bytes {
        return Err(ContractError::Budget);
    }
    let len = [material, native_body].into_iter().chain(preimages).try_fold(12usize, |n, b| {
        u32::try_from(b.len()).map_err(|_| ContractError::Budget)?;
        n.checked_add(4).and_then(|n| n.checked_add(b.len())).ok_or(ContractError::Budget)
    })?;
    if len as u64 > limits.max_payload_bytes {
        return Err(ContractError::Budget);
    }
    let mut raw = Vec::new();
    raw.try_reserve_exact(len).map_err(|_| ContractError::Budget)?;
    raw.extend_from_slice(&ASSIGNMENT_OPEN_MAGIC_V2);
    raw.extend_from_slice(&CONTRACT_VERSION.to_le_bytes());
    raw.extend_from_slice(&CONTRACT_REVISION.to_le_bytes());
    for b in [material, native_body].into_iter().chain(preimages) {
        raw.extend_from_slice(&(b.len() as u32).to_le_bytes());
        raw.extend_from_slice(b);
    }
    Ok(raw)
}
