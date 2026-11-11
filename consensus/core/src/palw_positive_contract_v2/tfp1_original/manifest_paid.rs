//! A rooted receipt for a paid initial request in one physical draw rank.
//! The native preparer resolves the signed request, drawn rank and booked slot.
//! Restore checks the resulting cross-references without treating the service
//! book's supply observation as evidence that this carrier consumed the slot.

use super::{manifest_carrier::ManifestRequestCarrierScopeV1, native_link::Tfp1PublicOriginalLinkV1};
use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::batch::admission::protected::manifest_request_packet_id_v1,
    palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, typed::native_struct, wire::wire_bad},
    palw_positive_contract_v2::{
        PUBLIC_PENDING_MANIFEST_TABLE_V1, PUBLIC_TFP1_LINK_TABLE_V1, check_public_origin_point_v1, require_current_public_origin_v1,
    },
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::{CaseId, Claimant, OriginalId, PendingManifestCaseRecordV1},
    rows::AcceptedOrigin,
};
use std::io::{self, Read};

const VERSION: u16 = 1;

pub(super) fn manifest_request_charge_purpose_v1(original: OriginalId, packet_id: [u8; 64]) -> Result<Hash64, ContractError> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/manifest-request-payment/v1", &(original, packet_id))
        .map_err(|_| ContractError::NonCanonical)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PaidManifestRequestRowV1 {
    version: u16,
    claim: Hash64,
    source: Hash64,
    reservation_scope: Hash64,
    packet_id: [u8; 64],
    original: OriginalId,
    case: CaseId,
    origin: AcceptedOrigin,
    request_digest: Digest,
    charge_purpose: Hash64,
    charged_sompi: u64,
}

native_struct!(PaidManifestRequestRowV1; version, claim, source, reservation_scope, packet_id, original, case, origin,
    request_digest, charge_purpose, charged_sompi);

impl PaidManifestRequestRowV1 {
    pub(crate) const KEY_BYTES: usize = 64;
    pub(crate) const WIRE_BYTES: usize = 2 + 11 * 64 + 2 * 4 + 3 * 8;

    pub(super) fn new_v1(
        scope: ManifestRequestCarrierScopeV1,
        original: OriginalId,
        case: CaseId,
        origin: AcceptedOrigin,
        request_digest: Digest,
        charge_purpose: Hash64,
        charged_sompi: u64,
    ) -> Result<Self, ContractError> {
        let row = Self {
            version: VERSION,
            claim: scope.claim,
            source: scope.source,
            reservation_scope: scope.reservation_scope,
            packet_id: manifest_request_packet_id_v1(scope.claim, scope.reservation_scope).map_err(|_| ContractError::Identity)?,
            original,
            case,
            origin,
            request_digest,
            charge_purpose,
            charged_sompi,
        };
        row.check_shape_v1(&row.packet_id)?;
        Ok(row)
    }

    pub(crate) fn key_v1(&self) -> [u8; Self::KEY_BYTES] {
        self.packet_id
    }

    pub(crate) fn charged_sompi_v1(&self) -> u64 {
        self.charged_sompi
    }

    pub(crate) fn check_insert_at_v1(&self, point: &PalwBlockContextV2) -> Result<(), ContractError> {
        require_current_public_origin_v1(self.origin, point)
    }

    pub(crate) fn decode_row(key: &[u8], bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() != Self::WIRE_BYTES {
            return Err(ContractError::NonCanonical);
        }
        let row: Self = borsh::from_slice(bytes).map_err(|_| ContractError::NonCanonical)?;
        row.check_shape_v1(key)?;
        Ok(row)
    }

    fn check_shape_v1(&self, key: &[u8]) -> Result<(), ContractError> {
        if self.version != VERSION || key != self.packet_id.as_slice() {
            return Err(ContractError::Identity);
        }
        if self.packet_id != manifest_request_packet_id_v1(self.claim, self.reservation_scope).map_err(|_| ContractError::Identity)?
            || self.charge_purpose != manifest_request_charge_purpose_v1(self.original, self.packet_id)?
            || [self.source, self.charge_purpose].contains(&Hash64::default())
            || [self.original.0, self.case.0, self.request_digest].contains(&Digest([0; 64]))
            || [self.origin.accepting_block.0, self.origin.carrier_block.0, self.origin.carrier_transaction.0].contains(&[0; 64])
        {
            return Err(ContractError::Identity);
        }
        if self.charged_sompi == 0 {
            return Err(ContractError::Budget);
        }
        if self.origin.accepted_daa == 0 || self.origin.accepted_blue_score == 0 || self.origin.event_ordinal != 0 {
            return Err(ContractError::Range);
        }
        Ok(())
    }

    pub(crate) fn check_state_at_v1(&self, state: &PalwChainStateV2, point: &PalwBlockContextV2) -> Result<(), ContractError> {
        self.check_shape_v1(&self.packet_id)?;
        check_public_origin_point_v1(self.origin, point)?;
        let route = state.kernel_route_state_v1().ok_or(ContractError::Scope)?;
        let link_key = self.claim.as_bytes();
        let link_bytes = route.aux.get(&(PUBLIC_TFP1_LINK_TABLE_V1, link_key.to_vec())).ok_or(ContractError::Scope)?;
        let link = Tfp1PublicOriginalLinkV1::decode_row(&link_key, link_bytes)?;
        link.check_state_at_v1(state, point)?;
        if link.source_v1() != self.source || link.original_v1() != self.original {
            return Err(ContractError::Scope);
        }
        let key = PendingManifestCaseRecordV1::key(self.original, self.case);
        let bytes = route.aux.get(&(PUBLIC_PENDING_MANIFEST_TABLE_V1, key.to_vec())).ok_or(ContractError::Scope)?;
        let pending = PendingManifestCaseRecordV1::decode_row(&key, bytes)?;
        state.check_public_pending_manifest_record_at_v1(&pending, point)?;
        if pending.origin() != self.origin
            || pending.request_digest() != self.request_digest
            || !matches!(pending.claimant()?, Claimant::DrawnBond(_))
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    /// Fixed-width wire preflight can check the external map key before
    /// retaining any native aux buffers.
    pub(crate) fn scan_row_v1(key: &[u8], reader: &mut NativeWireReaderV1<impl Read>) -> io::Result<NativeWireShapeV1> {
        if key.len() != Self::KEY_BYTES || reader.remaining() != Self::WIRE_BYTES as u64 {
            return Err(wire_bad("invalid paid ManifestRequest row width"));
        }
        let row = Self::read_wire_v1(reader)?;
        row.check_shape_v1(key).map_err(|_| wire_bad("invalid paid ManifestRequest row"))?;
        Ok(NativeWireShapeV1::default())
    }

    fn read_wire_v1(reader: &mut NativeWireReaderV1<impl Read>) -> io::Result<Self> {
        reader.step()?;
        let mut bytes = [0; Self::WIRE_BYTES];
        reader.read_exact(&mut bytes)?;
        borsh::from_slice(&bytes)
    }
}

impl NativeWireScanV1 for PaidManifestRequestRowV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        let row = Self::read_wire_v1(reader)?;
        row.check_shape_v1(&row.packet_id).map_err(|_| wire_bad("invalid paid ManifestRequest row"))?;
        Ok(NativeWireShapeV1::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::rows::NativeHash64;

    #[test]
    fn paid_manifest_receipt_has_exact_native_width_and_canonical_rank_key() {
        let h = Hash64::from_u64_word;
        let scope = ManifestRequestCarrierScopeV1 { claim: h(1), source: h(2), reservation_scope: h(3) };
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64(h(4).as_bytes()),
            carrier_block: NativeHash64(h(5).as_bytes()),
            carrier_transaction: NativeHash64(h(6).as_bytes()),
            accepted_tx_ordinal: 0,
            event_ordinal: 0,
            accepted_daa: 7,
            accepted_blue_score: 8,
        };
        let row = PaidManifestRequestRowV1::new_v1(
            scope,
            OriginalId(Digest(h(9).as_bytes())),
            CaseId(Digest(h(10).as_bytes())),
            origin,
            Digest(h(11).as_bytes()),
            manifest_request_charge_purpose_v1(
                OriginalId(Digest(h(9).as_bytes())),
                manifest_request_packet_id_v1(scope.claim, scope.reservation_scope).unwrap(),
            )
            .unwrap(),
            13,
        )
        .unwrap();
        let key = row.key_v1();
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(bytes.len(), PaidManifestRequestRowV1::WIRE_BYTES);
        assert_eq!(PaidManifestRequestRowV1::decode_row(&key, &bytes), Ok(row));
        let preflight = |key: &[u8], bytes: &[u8]| {
            let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
            PaidManifestRequestRowV1::scan_row_v1(key, &mut reader)
        };
        assert!(preflight(&key, &bytes).is_ok());
        assert!(preflight(&h(13).as_bytes(), &bytes).is_err());
        assert!(preflight(&key, &bytes[1..]).is_err());
        assert!(preflight(&key, &[bytes.as_slice(), &[0]].concat()).is_err());

        for wrong in [
            PaidManifestRequestRowV1 { version: 2, ..row },
            PaidManifestRequestRowV1 { reservation_scope: h(14), ..row },
            PaidManifestRequestRowV1 { origin: AcceptedOrigin { carrier_transaction: NativeHash64([0; 64]), ..origin }, ..row },
            PaidManifestRequestRowV1 { charged_sompi: 0, ..row },
            PaidManifestRequestRowV1 { charge_purpose: h(12), ..row },
        ] {
            let bytes = borsh::to_vec(&wrong).unwrap();
            assert!(PaidManifestRequestRowV1::decode_row(&key, &bytes).is_err());
            assert!(preflight(&key, &bytes).is_err());
        }
        let at = PalwBlockContextV2 { block: h(4), daa_score: 7, blue_score: 8, subsidy: 0 };
        assert_eq!(row.check_insert_at_v1(&at), Ok(()));
        assert!(row.check_insert_at_v1(&PalwBlockContextV2 { blue_score: 9, ..at }).is_err());
    }
}
