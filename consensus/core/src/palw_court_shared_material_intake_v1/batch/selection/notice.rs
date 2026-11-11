//! Fixed inner wire for the row and comparison lane chosen by one protected
//! Selection case. A signature does not establish paid service or acceptance.

use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::batch::admission::plain_selection::PlainSelectionBranchV3,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1},
    palw_positive_contract_v2::tfp1_original::native_link::RootedTfp1DrawnBatchCaseV1,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io;

/// Signature and geometry checked against one borrowed native prefix. This
/// is deliberately not an accepted notice or permission to spend a slot.
pub(crate) struct AuthenticatedSelectionNoticeV3<'a> {
    notice: &'a SignedSelectionNoticeV3,
    branch: &'a PlainSelectionBranchV3,
    source: &'a PalwTirPublicSourceV1,
    _state: &'a PalwChainStateV2,
}

impl AuthenticatedSelectionNoticeV3<'_> {
    /// Resolve a supplier's reserved ordinal using the signed position, never
    /// a second row supplied by the material carrier. Equal range IDs for two
    /// roles cannot borrow the other role's ordinal.
    pub(crate) fn project_demand_v3(
        &self,
        role: u64,
        index: usize,
        ordinal: u64,
        ranges: &[crate::palw_batch_court_v1::BatchRangeV1],
    ) -> Result<NativeRetainedV1<super::delivery::SelectionBatchRowV2>, NativeDecodeErrorV1<String>> {
        self.branch.project_checked_demand_v3(self.source, self.notice.position, role, index, ordinal, ranges)
    }
}

pub(crate) const SELECTION_NOTICE_VERSION_V3: u16 = 3;
pub(crate) const SELECTION_NOTICE_SIGNATURE_CONTEXT_V3: &[u8] = b"misaka-palw/selection-notice/v3/mldsa87";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct SignedSelectionNoticeV3 {
    pub version: u16,
    pub network: Hash64,
    pub claim: Hash64,
    pub source: Hash64,
    pub branch: Hash64,
    pub case_scope: Hash64,
    pub reservation_scope: Hash64,
    pub position: u32,
    pub beat: u32,
    pub filer: PalwBondKeyV2,
    pub signature: [u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
}

impl SignedSelectionNoticeV3 {
    pub(crate) const BYTES: u64 = 2 + 6 * 64 + 2 * 4 + 64 + 4 + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;

    pub(crate) fn check_shape_v3(&self) -> Result<(), &'static str> {
        if self.version != SELECTION_NOTICE_VERSION_V3
            || [
                self.network,
                self.claim,
                self.source,
                self.branch,
                self.case_scope,
                self.reservation_scope,
                self.filer.0.transaction_id,
            ]
            .contains(&Hash64::default())
            || self.case_scope == self.reservation_scope
        {
            return Err("invalid Selection notice version or scope");
        }
        Ok(())
    }

    pub(crate) fn signing_message_v3(&self) -> Result<Hash64, String> {
        self.check_shape_v3()?;
        stream_digest(
            b"misaka-palw/selection-notice-signing/v3",
            &(
                self.version,
                self.network,
                self.claim,
                self.source,
                self.branch,
                self.case_scope,
                self.reservation_scope,
                self.position,
                self.beat,
                self.filer,
            ),
        )
    }

    fn verify_signature_v3(&self, pubkey: &[u8]) -> Result<(), String> {
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
            pubkey,
            self.signing_message_v3()?.as_byte_slice(),
            &self.signature,
            SELECTION_NOTICE_SIGNATURE_CONTEXT_V3,
        ) {
            return Err("invalid Selection notice signature".into());
        }
        Ok(())
    }

    /// The caller rederives the typed case from the executing prefix before
    /// calling this method. Standing, actual fee, assigned slot and the
    /// immutable one-notice-per-rank write belong to the accepting writer.
    fn verify_for_case_v3(
        &self,
        expected_network: Hash64,
        case: &RootedTfp1DrawnBatchCaseV1,
        branch: &PlainSelectionBranchV3,
        source: &PalwTirPublicSourceV1,
        state: &PalwChainStateV2,
    ) -> Result<(), String> {
        self.check_shape_v3()?;
        let drawn = case.case_v1().opportunity_v1().bond_v1();
        let expected_filer = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes(drawn.transaction.0), drawn.index));
        if self.network != expected_network
            || self.claim != case.claim_v1()
            || self.claim != branch.claim_id_v3()
            || self.source != case.source_v1()
            || self.source != branch.source_v3()
            || self.source != source.source_id_v1()
            || self.case_scope != case.case_scope_v1()
            || self.reservation_scope != case.reservation_scope_v1()
            || self.branch != branch.identity_v3().map_err(|why| format!("invalid Selection branch: {why:?}"))?
            || self.filer != expected_filer
            || self.position >= source.decode_count_v1()
            || self.beat >= source.vocab_v1()
        {
            return Err("Selection notice differs from its native case, Source, branch or drawn filer".into());
        }
        let bond = state.bond(&expected_filer).ok_or("Selection notice drawn bond is absent")?;
        let key = bond.pubkey.as_slice().try_into().map_err(|_| "Selection notice drawn key has incorrect width")?;
        if misaka_palw_positive_contract::identity::principal(key) != case.case_v1().opportunity_v1().principal_v1() {
            return Err("Selection notice native key differs from its accepted draw principal".into());
        }
        self.verify_signature_v3(&bond.pubkey)
    }

    pub(crate) fn authenticate_for_case_v3<'a>(
        &'a self,
        expected_network: Hash64,
        case: &RootedTfp1DrawnBatchCaseV1,
        branch: &'a PlainSelectionBranchV3,
        source: &'a PalwTirPublicSourceV1,
        state: &'a PalwChainStateV2,
    ) -> Result<AuthenticatedSelectionNoticeV3<'a>, String> {
        self.verify_for_case_v3(expected_network, case, branch, source, state)?;
        Ok(AuthenticatedSelectionNoticeV3 { notice: self, branch, source, _state: state })
    }

    pub(crate) fn encode_v3(&self) -> io::Result<[u8; Self::BYTES as usize]> {
        self.check_shape_v3().map_err(invalid)?;
        let mut bytes = [0; Self::BYTES as usize];
        let mut out = bytes.as_mut_slice();
        self.serialize(&mut out)?;
        if !out.is_empty() {
            return Err(invalid("Selection notice encoding differs from fixed width"));
        }
        Ok(bytes)
    }

    pub(crate) fn decode_v3(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 != Self::BYTES {
            return Err(invalid("Selection notice has incorrect fixed width"));
        }
        let notice: Self = borsh::from_slice(bytes)?;
        notice.check_shape_v3().map_err(invalid)?;
        Ok(notice)
    }
}

fn invalid(why: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_notice_v3_fixed_codec_binds_every_signed_coordinate() {
        let h = Hash64::from_u64_word;
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([55; 32]);
        let mut notice = SignedSelectionNoticeV3 {
            version: SELECTION_NOTICE_VERSION_V3,
            network: h(1),
            claim: h(2),
            source: h(3),
            branch: h(4),
            case_scope: h(5),
            reservation_scope: h(6),
            position: 7,
            beat: 8,
            filer: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(9), 1)),
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let digest = notice.signing_message_v3().unwrap();
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, digest.as_byte_slice(), SELECTION_NOTICE_SIGNATURE_CONTEXT_V3, [56; 32])
                .unwrap();
        notice.signature = *signature.as_ref();
        let encoded = notice.encode_v3().unwrap();
        assert_eq!(encoded.as_slice(), borsh::to_vec(&notice).unwrap().as_slice());
        let decoded = SignedSelectionNoticeV3::decode_v3(&encoded).unwrap();
        assert_eq!(decoded, notice);
        decoded.verify_signature_v3(key.verification_key.as_ref()).unwrap();

        for coordinate in 0..10 {
            let mut altered = decoded.clone();
            match coordinate {
                0 => altered.network = h(20),
                1 => altered.claim = h(21),
                2 => altered.source = h(22),
                3 => altered.branch = h(23),
                4 => altered.case_scope = h(24),
                5 => altered.reservation_scope = h(25),
                6 => altered.position += 1,
                7 => altered.beat += 1,
                8 => altered.filer.0.transaction_id = h(26),
                9 => altered.filer.0.index += 1,
                _ => unreachable!(),
            }
            assert!(altered.verify_signature_v3(key.verification_key.as_ref()).is_err());
        }
        let mut wrong_signature = decoded.clone();
        wrong_signature.signature[0] ^= 1;
        assert!(wrong_signature.verify_signature_v3(key.verification_key.as_ref()).is_err());
        assert!(SignedSelectionNoticeV3::decode_v3(&encoded[..encoded.len() - 1]).is_err());
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert!(SignedSelectionNoticeV3::decode_v3(&trailing).is_err());
        let mut wrong_version = encoded;
        wrong_version[..2].copy_from_slice(&4u16.to_le_bytes());
        assert!(SignedSelectionNoticeV3::decode_v3(&wrong_version).is_err());
        let empty_claim = SignedSelectionNoticeV3 { claim: Hash64::default(), ..notice };
        assert!(empty_claim.encode_v3().is_err());
        assert!(SignedSelectionNoticeV3::decode_v3(&borsh::to_vec(&empty_claim).unwrap()).is_err());
    }
}
