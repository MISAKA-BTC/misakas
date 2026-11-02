//! Internal Assignment control journal envelope. Distinct from literal op8–13
//! wire bodies and PC2AOPEN. Native origin/prefix DATA never authorizes a row.
use crate::ContractError;
pub const ASSIGNMENT_CONTROL_MAGIC_V2: [u8; 8] = *b"PC2ACTRL";
pub const ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2: usize = 496;
pub const ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2: usize = 12 + ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2 + 8;
#[derive(Clone, Copy)]
pub struct AssignmentControlViewV2<'a> {
    boundary: &'a [u8; ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2],
    inner: &'a [u8],
}
impl<'a> AssignmentControlViewV2<'a> {
    pub fn decode(raw: &'a [u8], max_payload: u64) -> Result<Self, ContractError> {
        if max_payload == 0 || max_payload == u64::MAX || raw.len() as u64 > max_payload {
            return Err(ContractError::Budget);
        }
        if raw.get(..8) != Some(&ASSIGNMENT_CONTROL_MAGIC_V2) || raw.get(8..12) != Some(&[2, 0, 1, 0]) {
            return Err(ContractError::NonCanonical);
        }
        let end = 12 + ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2;
        let boundary = raw.get(12..end).ok_or(ContractError::NonCanonical)?.try_into().map_err(|_| ContractError::NonCanonical)?;
        let len = u64::from_le_bytes(
            raw.get(end..end + 8).ok_or(ContractError::NonCanonical)?.try_into().map_err(|_| ContractError::NonCanonical)?,
        );
        let expected = len.checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2 as u64).ok_or(ContractError::Budget)?;
        if len == 0 || expected != raw.len() as u64 {
            return Err(ContractError::NonCanonical);
        }
        Ok(Self { boundary, inner: &raw[end + 8..] })
    }
    pub fn boundary(&self) -> &'a [u8; ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2] {
        self.boundary
    }
    pub fn inner(&self) -> &'a [u8] {
        self.inner
    }
}
/// Allocation must be admitted by the concrete native owner before calling.
pub fn encode_assignment_control_v2(
    boundary: &[u8; ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2],
    inner: &[u8],
    max_payload: u64,
) -> Result<Vec<u8>, ContractError> {
    let len = inner.len().checked_add(ASSIGNMENT_CONTROL_OVERHEAD_BYTES_V2).ok_or(ContractError::Budget)?;
    if inner.is_empty() || max_payload == 0 || max_payload == u64::MAX || len as u64 > max_payload {
        return Err(ContractError::Budget);
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len).map_err(|_| ContractError::Budget)?;
    out.extend_from_slice(&ASSIGNMENT_CONTROL_MAGIC_V2);
    out.extend_from_slice(&[2, 0, 1, 0]);
    out.extend_from_slice(boundary);
    out.extend_from_slice(&(inner.len() as u64).to_le_bytes());
    out.extend_from_slice(inner);
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_binds_exact_boundary_and_literal_inner_without_fallback() {
        let boundary = [9; ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2];
        let inner = [2, 0, 1, 0, 12];
        let bytes = encode_assignment_control_v2(&boundary, &inner, 1000).unwrap();
        let view = AssignmentControlViewV2::decode(&bytes, 1000).unwrap();
        assert_eq!(view.boundary(), &boundary);
        assert_eq!(view.inner(), inner);
        assert!(AssignmentControlViewV2::decode(&inner, 1000).is_err());
        assert!(encode_assignment_control_v2(&boundary, &inner, (bytes.len() - 1) as u64).is_err());
        let mut altered = bytes.clone();
        altered.push(0);
        assert!(AssignmentControlViewV2::decode(&altered, 1000).is_err());
        altered = bytes;
        altered[8] = 3;
        assert!(AssignmentControlViewV2::decode(&altered, 1000).is_err());
    }
}
