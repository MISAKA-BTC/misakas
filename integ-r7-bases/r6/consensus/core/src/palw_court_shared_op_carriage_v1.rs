//! **G14 lane r5-f02-wire (2026-10-22): the wire carriage of RFC-0029's shared court lifecycle** — written, not compiled.
//!
//! Lifecycle object tag **178** (`PalwConsensusObjectV2::SharedCourtOpV1`), owned by `palw_tir_public_conviction_v1` (the fold
//! additionally requires `palw_court_outsider_lane_v1`; see `palw_court_service_lifecycle_fold_v1::apply_shared_court_op_v1`).
//! Below the fence it is the live build's undecodable payload and rides unjudged (A-2).
//!
//! The carriage only names WHICH shared operation is offered. It carries no verdict, no "complete/paid" flag and no default
//! permission: request intents are bond-signed ML-DSA-87 statements (checked at the acceptance layer and again by the fold),
//! bodies / complete IDs are authenticated by content against the original's committed roots, and numerical transport bundles are
//! decoded and their signed controls checked by the fold. Every variable-length field is bounded BEFORE allocation.
use crate::Hash64;
use crate::palw_court_numerical_transport_v1::{NumericalManifestV1, NumericalPartV1};
use crate::palw_court_shared_tir_frontier_v1::SignedSharedRequestV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

/// Upper bound of one shared body (the existing public control ceiling; a unit's own bound is the fold's `unit_bound_v1`).
pub const PALW_SHARED_COURT_BODY_MAX_BYTES_V1: usize = crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1 as usize;
/// Upper bound of one complete-ID vector (the same byte ceiling at four bytes per id).
pub const PALW_SHARED_COURT_MAX_IDS_V1: usize = PALW_SHARED_COURT_BODY_MAX_BYTES_V1 / 4;
/// Upper bound of numerical parts in one object (parts are ≤ `NUMERICAL_PART_BYTES_V1` = 16 KiB each, so ≤ ~80 KiB in total).
pub const PALW_SHARED_COURT_MAX_PARTS_V1: usize = 5;

/// One shared-court operation as it rides the wire. Variant order is part of the borsh layout (pinned by tag 178's wire pin).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PalwSharedCourtOpV1 {
    /// A shared body for the request at `ordinal` (data identity only; authenticated by content).
    Body { claim: Hash64, ordinal: u64, bytes: Vec<u8> },
    /// The original's complete decode IDs (authenticated against the outer trace root).
    Ids { claim: Hash64, rows_root: Hash64, ids: Vec<u32> },
    /// A bond-signed request intent (the claim's native notice past the RFC-0031 intake).
    Request { claim: Hash64, intent: SignedSharedRequestV1 },
    /// A complete numerical transport bundle (root / round / bottom; signed controls inside).
    Numerical { manifest: NumericalManifestV1, parts: Vec<NumericalPartV1> },
    /// RFC-0026 F02 during the RFC08 post-Final liability hold: ask the fold to decide the held text29 (29) / funded v11 (11)
    /// Final of `job`. Carries no verdict and no silence claim: the fold decides from the recorded supply, the duty clocks and the
    /// family's own answer record, and refuses anything undecided or a no-slash hold.
    NativeHeldDefault { family: u8, job: [u8; 64] },
}

impl BorshDeserialize for PalwSharedCourtOpV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let invalid = |why: &'static str| io::Error::new(io::ErrorKind::InvalidData, why);
        match u8::deserialize_reader(r)? {
            0 => {
                let claim = Hash64::deserialize_reader(r)?;
                let ordinal = u64::deserialize_reader(r)?;
                let bytes = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_SHARED_COURT_BODY_MAX_BYTES_V1)?;
                if bytes.is_empty() {
                    return Err(invalid("empty shared body"));
                }
                Ok(Self::Body { claim, ordinal, bytes })
            }
            1 => {
                let claim = Hash64::deserialize_reader(r)?;
                let rows_root = Hash64::deserialize_reader(r)?;
                let n = u32::deserialize_reader(r)? as usize;
                if n > PALW_SHARED_COURT_MAX_IDS_V1 {
                    return Err(invalid("shared ID vector exceeds its bound before allocation"));
                }
                let mut ids = Vec::new();
                ids.try_reserve_exact(n).map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "bounded shared ID allocation"))?;
                for _ in 0..n {
                    ids.push(u32::deserialize_reader(r)?);
                }
                Ok(Self::Ids { claim, rows_root, ids })
            }
            2 => {
                let claim = Hash64::deserialize_reader(r)?;
                let intent = SignedSharedRequestV1::deserialize_reader(r)?;
                Ok(Self::Request { claim, intent })
            }
            3 => {
                let manifest = NumericalManifestV1::deserialize_reader(r)?;
                let n = u32::deserialize_reader(r)? as usize;
                if n == 0 || n > PALW_SHARED_COURT_MAX_PARTS_V1 {
                    return Err(invalid("numerical part count outside its bound"));
                }
                let mut parts = Vec::with_capacity(n);
                for _ in 0..n {
                    parts.push(NumericalPartV1::deserialize_reader(r)?);
                }
                Ok(Self::Numerical { manifest, parts })
            }
            4 => {
                let family = u8::deserialize_reader(r)?;
                let job = <[u8; 64]>::deserialize_reader(r)?;
                Ok(Self::NativeHeldDefault { family, job })
            }
            _ => Err(invalid("unknown shared court operation")),
        }
    }
}

impl PalwSharedCourtOpV1 {
    /// The claim this operation names on its face (a native held default names a job; its claim is the held row's).
    pub fn claim_v1(&self) -> Option<Hash64> {
        match self {
            Self::Body { claim, .. } | Self::Ids { claim, .. } | Self::Request { claim, .. } => Some(*claim),
            Self::Numerical { manifest, .. } => Some(manifest.claim),
            Self::NativeHeldDefault { .. } => None,
        }
    }

    /// The literal signer whose CURRENT authority the acceptance layer checks (only a request names one on the outer object; the
    /// numerical controls' signers are checked by the fold after decoding).
    pub fn signer_v1(&self) -> Option<PalwBondKeyV2> {
        match self {
            Self::Request { intent, .. } => Some(intent.signer),
            _ => None,
        }
    }

    /// Stateless shape (the may-ride gate past the fence): bounded already by decode; a request must name the claim's scope
    /// signer and carry a signature of the exact ML-DSA-87 length (the decoder enforces it, re-checked for constructed values).
    pub fn validate_shape_v1(&self) -> Result<(), &'static str> {
        match self {
            Self::Body { bytes, .. } if bytes.is_empty() || bytes.len() > PALW_SHARED_COURT_BODY_MAX_BYTES_V1 => {
                Err("shared body outside its bound")
            }
            Self::Ids { ids, .. } if ids.len() > PALW_SHARED_COURT_MAX_IDS_V1 => Err("shared IDs outside their bound"),
            Self::Request { intent, .. } if intent.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN => {
                Err("shared request signature shape")
            }
            Self::Numerical { parts, .. } if parts.is_empty() || parts.len() > PALW_SHARED_COURT_MAX_PARTS_V1 => {
                Err("numerical part count outside its bound")
            }
            Self::NativeHeldDefault { family, .. }
                if !matches!(
                    *family,
                    crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_TEXT_V29
                        | crate::palw_native_liability_hold_v1::PALW_NATIVE_FAMILY_FUNDED_V11
                ) =>
            {
                Err("post-Final default names a family without a held Final default")
            }
            _ => Ok(()),
        }
    }

    /// **Acceptance-layer authority (fence already checked by the caller):** a request's signer must be a CURRENT bond — present,
    /// `Active` and at the collateral floor — and the ML-DSA-87 signature must verify under that bond's registered key over the
    /// network-bound intent message. A signature alone grants no standing: scope/coordinate binding, the RFC-0031 notice window
    /// and dedupe are the fold's.
    pub fn validate_authority_v1(&self, network: Hash64, state: &PalwChainStateV2, floor: u64) -> Result<(), String> {
        self.validate_shape_v1().map_err(String::from)?;
        let Self::Request { intent, .. } = self else { return Ok(()) };
        let bond = state.bond(&intent.signer).ok_or("shared requester bond is absent")?;
        if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < floor {
            return Err("shared requester is not a current Active bond at the floor".into());
        }
        intent.validate_bond_signature(network, state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }
    fn signer() -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(9), 1))
    }

    fn samples() -> Vec<PalwSharedCourtOpV1> {
        vec![
            PalwSharedCourtOpV1::Body { claim: h(1), ordinal: 3, bytes: vec![7; 33] },
            PalwSharedCourtOpV1::Ids { claim: h(2), rows_root: h(3), ids: vec![1, 2, 3] },
            PalwSharedCourtOpV1::Request {
                claim: h(4),
                intent: SignedSharedRequestV1 {
                    version: 1,
                    scope: h(5),
                    ordinal: 0,
                    request_id: h(6),
                    signer: signer(),
                    signature: vec![9; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
                },
            },
            PalwSharedCourtOpV1::Numerical {
                manifest: NumericalManifestV1 {
                    version: 1,
                    network: h(7),
                    scope: h(8),
                    claim: h(9),
                    leaf: 2,
                    kind: crate::palw_court_numerical_transport_v1::NumericalKindV1::Bottom,
                    total_bytes: 4,
                    body_hash: h(10),
                },
                parts: vec![NumericalPartV1 { version: 1, stream_id: h(11), offset: 0, bytes: vec![1, 2, 3, 4] }],
            },
            PalwSharedCourtOpV1::NativeHeldDefault { family: 29, job: [5; 64] },
        ]
    }

    #[test]
    fn r5_f02_wire_shared_op_borsh_round_trips_every_variant() {
        for op in samples() {
            let bytes = borsh::to_vec(&op).unwrap();
            assert_eq!(PalwSharedCourtOpV1::try_from_slice(&bytes).unwrap(), op);
            assert!(op.validate_shape_v1().is_ok());
        }
    }

    #[test]
    fn r5_f02_wire_shared_op_object_tag_is_178_and_round_trips() {
        for op in samples() {
            let object = crate::palw_state_v2::PalwConsensusObjectV2::SharedCourtOpV1 { op: Box::new(op) };
            let bytes = borsh::to_vec(&object).unwrap();
            assert_eq!(bytes[0], 178);
            assert_eq!(crate::palw_state_v2::PalwConsensusObjectV2::try_from_slice(&bytes).unwrap(), object);
        }
    }

    #[test]
    fn r5_f02_wire_shared_op_boundary_decode() {
        // Body: empty refused; exactly the bound accepted; one past refused before allocation.
        let body = |n: usize| {
            let mut v = vec![0u8];
            v.extend(h(1).as_byte_slice());
            v.extend(0u64.to_le_bytes());
            v.extend((n as u32).to_le_bytes());
            v.extend(vec![1u8; n.min(PALW_SHARED_COURT_BODY_MAX_BYTES_V1)]);
            v
        };
        assert!(PalwSharedCourtOpV1::try_from_slice(&body(0)).is_err());
        assert!(PalwSharedCourtOpV1::try_from_slice(&body(PALW_SHARED_COURT_BODY_MAX_BYTES_V1)).is_ok());
        assert!(PalwSharedCourtOpV1::try_from_slice(&body(PALW_SHARED_COURT_BODY_MAX_BYTES_V1 + 1)).is_err());
        // A length prefix of u32::MAX is refused before allocation (ids).
        let mut ids = vec![1u8];
        ids.extend(h(1).as_byte_slice());
        ids.extend(h(2).as_byte_slice());
        ids.extend(u32::MAX.to_le_bytes());
        assert!(PalwSharedCourtOpV1::try_from_slice(&ids).is_err());
        // Unknown variant, trailing bytes, truncated input.
        assert!(PalwSharedCourtOpV1::try_from_slice(&[5u8]).is_err());
        assert!(PalwSharedCourtOpV1::NativeHeldDefault { family: 39, job: [0; 64] }.validate_shape_v1().is_err());
        let mut trailing = borsh::to_vec(&samples()[0]).unwrap();
        trailing.push(0);
        assert!(PalwSharedCourtOpV1::try_from_slice(&trailing).is_err());
        let full = borsh::to_vec(&samples()[2]).unwrap();
        assert!(PalwSharedCourtOpV1::try_from_slice(&full[..full.len() - 1]).is_err());
        // Numerical: zero parts and one past the bound are refused.
        let PalwSharedCourtOpV1::Numerical { manifest, parts } = samples().remove(3) else { unreachable!() };
        for n in [0usize, PALW_SHARED_COURT_MAX_PARTS_V1 + 1] {
            let op = PalwSharedCourtOpV1::Numerical { manifest: manifest.clone(), parts: vec![parts[0].clone(); n] };
            assert!(PalwSharedCourtOpV1::try_from_slice(&borsh::to_vec(&op).unwrap()).is_err());
            assert!(op.validate_shape_v1().is_err());
        }
        // A request whose signature is not ML-DSA-87 sized is refused at decode.
        let PalwSharedCourtOpV1::Request { claim, mut intent } = samples().remove(2) else { unreachable!() };
        intent.signature.pop();
        let op = PalwSharedCourtOpV1::Request { claim, intent };
        assert!(PalwSharedCourtOpV1::try_from_slice(&borsh::to_vec(&op).unwrap()).is_err());
        assert!(op.validate_shape_v1().is_err());
    }

    #[test]
    fn r5_f02_wire_shared_op_authority_refuses_an_absent_bond() {
        let state = PalwChainStateV2::genesis();
        let op = samples().remove(2);
        assert!(op.validate_authority_v1(h(0), &state, 0).is_err());
        // Data-identity operations name no signer and pass the outer authority check (the fold authenticates content).
        assert!(samples()[0].validate_authority_v1(h(0), &state, 0).is_ok());
    }
}
