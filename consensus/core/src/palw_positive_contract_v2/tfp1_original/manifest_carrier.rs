//! Literal protected drawn ManifestRequest transport and local UTXO fee handoff.
//! Consensus must separately authenticate the request, its slot and its lifetime.

pub(crate) mod replay;

use crate::{
    Hash64, TransactionId,
    palw_court_carrier_fee_v1::{CarrierFeeBudgetV1, CarrierFeeRefusalV1},
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, btree_node_envelope_bytes_v1, reserve_native_pending_requests_v1},
    palw_state_v2::PalwBlockContextV2,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::ValidatedTransaction,
};
use misaka_palw_positive_contract::public_one_honest::{Claimant, SignedManifestRequestV1};

pub(crate) const MANIFEST_REQUEST_CARRIER_MAGIC_V1: &[u8; 4] = b"PMR1";
const DRAWN_TAG: u8 = 0;
const HEADER_BYTES: usize = 5 + 3 * 64;
const REQUEST_BYTES: usize = SignedManifestRequestV1::MAX_WIRE_BYTES;
pub(crate) const MANIFEST_REQUEST_CARRIER_BYTES_V1: usize = HEADER_BYTES + REQUEST_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) struct ManifestRequestCarrierScopeV1 {
    pub(crate) claim: Hash64,
    pub(crate) source: Hash64,
    pub(crate) reservation_scope: Hash64,
}

impl ManifestRequestCarrierScopeV1 {
    fn check_v1(&self) -> Result<(), ManifestRequestCarrierErrorV1> {
        if [self.claim, self.source, self.reservation_scope].contains(&Hash64::default()) {
            return Err(ManifestRequestCarrierErrorV1::Decode("zero ManifestRequest carrier scope"));
        }
        Ok(())
    }
}

/// The writer supplies its expected values from accepted order and the
/// protected reservation. Values read from the carrier alone carry no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManifestRequestCarrierContextV1 {
    pub(crate) scope: ManifestRequestCarrierScopeV1,
    pub(crate) at: PalwBlockContextV2,
    pub(crate) carrier_block: Hash64,
    pub(crate) accepted_ordinal: usize,
    pub(crate) carrier: TransactionId,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ManifestRequestCarrierErrorV1 {
    Decode(&'static str),
    Context,
    Charge,
    Fee(CarrierFeeRefusalV1),
    Resource(NativeCoverageV1),
}

/// Decoding checks the fixed framing and drawn request shape. The exact
/// signed bytes survive decoding for the native owner to authenticate.
pub(crate) fn decode_manifest_request_carrier_v1(
    payload: &[u8],
) -> Result<Option<(ManifestRequestCarrierScopeV1, &[u8; REQUEST_BYTES])>, ManifestRequestCarrierErrorV1> {
    use ManifestRequestCarrierErrorV1::Decode;
    if !payload.starts_with(MANIFEST_REQUEST_CARRIER_MAGIC_V1) {
        if payload.starts_with(b"PMR") {
            return Err(Decode("unknown ManifestRequest carrier version"));
        }
        return Ok(None);
    }
    if payload.len() != MANIFEST_REQUEST_CARRIER_BYTES_V1 || payload[4] != DRAWN_TAG {
        return Err(Decode("invalid ManifestRequest carrier tag or width"));
    }
    let hash = |start| Hash64::from_bytes(payload[start..start + 64].try_into().expect("fixed carrier header"));
    let scope = ManifestRequestCarrierScopeV1 { claim: hash(5), source: hash(69), reservation_scope: hash(133) };
    scope.check_v1()?;
    let request = payload[HEADER_BYTES..].try_into().expect("fixed carrier request");
    check_drawn_request_v1(request)?;
    Ok(Some((scope, request)))
}

pub(crate) fn encode_manifest_request_carrier_v1(
    scope: ManifestRequestCarrierScopeV1,
    request: &[u8; REQUEST_BYTES],
) -> Result<[u8; MANIFEST_REQUEST_CARRIER_BYTES_V1], ManifestRequestCarrierErrorV1> {
    scope.check_v1()?;
    check_drawn_request_v1(request)?;
    let mut payload = [0; MANIFEST_REQUEST_CARRIER_BYTES_V1];
    payload[..4].copy_from_slice(MANIFEST_REQUEST_CARRIER_MAGIC_V1);
    payload[4] = DRAWN_TAG;
    payload[5..69].copy_from_slice(&scope.claim.as_bytes());
    payload[69..133].copy_from_slice(&scope.source.as_bytes());
    payload[133..HEADER_BYTES].copy_from_slice(&scope.reservation_scope.as_bytes());
    payload[HEADER_BYTES..].copy_from_slice(request);
    Ok(payload)
}

fn check_drawn_request_v1(bytes: &[u8]) -> Result<(), ManifestRequestCarrierErrorV1> {
    use ManifestRequestCarrierErrorV1::Decode;
    let signed = SignedManifestRequestV1::decode(bytes).map_err(|_| Decode("invalid literal signed ManifestRequest"))?;
    if !matches!(signed.request.claimant, Claimant::DrawnBond(_)) {
        return Err(Decode("protected drawn carrier requires a drawn ManifestRequest"));
    }
    Ok(())
}

/// A local, single-owner fee ledger. Its only dynamically sized field has a
/// native lease. Neither this handoff nor a successful allocation is a
/// consensus acceptance receipt.
#[derive(Debug)]
pub(crate) struct ValidatedManifestRequestCarrierV1 {
    context: ManifestRequestCarrierContextV1,
    request: [u8; REQUEST_BYTES],
    budget: NativeRetainedV1<CarrierFeeBudgetV1>,
    charged: bool,
    charge_purpose: Option<Hash64>,
    replay_parent_root: Option<Hash64>,
}

crate::palw_native_resource_v1::native_struct!(ManifestRequestCarrierScopeV1; claim, source, reservation_scope);
crate::palw_native_resource_v1::native_struct!(ManifestRequestCarrierContextV1; scope, at, carrier_block, accepted_ordinal, carrier);
crate::palw_native_resource_v1::native_struct!(ValidatedManifestRequestCarrierV1; context, request, budget, charged, charge_purpose, replay_parent_root);

impl ValidatedManifestRequestCarrierV1 {
    pub(crate) fn prepare_v1(
        validated: &ValidatedTransaction<'_>,
        preceding_fee: u64,
        carrier_block: Hash64,
        at: PalwBlockContextV2,
        accepted_ordinal: usize,
    ) -> Result<Option<Self>, ManifestRequestCarrierErrorV1> {
        use ManifestRequestCarrierErrorV1::{Decode, Fee, Resource};
        let tx = validated.tx;
        let Some((scope, request)) = decode_manifest_request_carrier_v1(&tx.payload)? else {
            return Ok(None);
        };
        if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
            return Err(Decode("ManifestRequest carrier uses another subnetwork"));
        }
        if carrier_block == Hash64::default() {
            return Err(ManifestRequestCarrierErrorV1::Context);
        }

        let input_set = (tx.inputs.len() as u64)
            .checked_mul(btree_node_envelope_bytes_v1::<(TransactionId, u32), ()>() as u64)
            .ok_or_else(|| Resource(NativeCoverageV1::overflow::<Self>("ManifestRequest fee input set")))?;
        let _input_owner = reserve_native_pending_requests_v1(input_set).map_err(Resource)?;
        let fee_owner = reserve_native_pending_requests_v1(fee_ledger_bytes_v1(u64::from(preceding_fee != 0))).map_err(Resource)?;
        let mut budget = CarrierFeeBudgetV1::from_validated_v1(validated).map_err(Fee)?;
        let carrier = budget.carrier_v1();
        if preceding_fee != 0 {
            let prior = stream_digest(b"misaka-palw/pmr-preceding-fee/v1", &carrier)
                .map_err(|_| Decode("ManifestRequest preceding fee identity"))?;
            budget.allocate_v1(prior.as_bytes(), preceding_fee).map_err(Fee)?;
        }
        Ok(Some(Self {
            context: ManifestRequestCarrierContextV1 { scope, at, carrier_block, accepted_ordinal, carrier },
            request: *request,
            budget: NativeRetainedV1::from_owner_reserved_v1(budget, fee_owner),
            charged: false,
            charge_purpose: None,
            replay_parent_root: None,
        }))
    }

    pub(crate) fn context_v1(&self) -> ManifestRequestCarrierContextV1 {
        self.context
    }

    pub(crate) fn request_at_v1(
        &self,
        expected: &ManifestRequestCarrierContextV1,
    ) -> Result<&[u8; REQUEST_BYTES], ManifestRequestCarrierErrorV1> {
        if self.context != *expected {
            return Err(ManifestRequestCarrierErrorV1::Context);
        }
        Ok(&self.request)
    }

    pub(crate) fn remaining_fee_v1(&self) -> u64 {
        self.budget.remaining_v1()
    }

    /// Allocate the owner's total charge once. The owner must derive purpose
    /// and amount from the admitted profile and verify the booked slot first.
    /// This method can neither choose a slot nor publish a paid row.
    pub(crate) fn allocate_charge_v1(
        &mut self,
        expected: &ManifestRequestCarrierContextV1,
        purpose: Hash64,
        amount: u64,
    ) -> Result<(), ManifestRequestCarrierErrorV1> {
        use ManifestRequestCarrierErrorV1::{Charge, Fee, Resource};
        self.request_at_v1(expected)?;
        if purpose == Hash64::default() || amount == 0 {
            return Err(Charge);
        }
        if self.charged {
            return Err(Fee(CarrierFeeRefusalV1::DuplicateAllocation));
        }
        let identity =
            stream_digest(b"misaka-palw/pmr-charge/v1", &(expected.carrier, expected.scope, purpose)).map_err(|_| Charge)?;
        if amount > self.budget.remaining_v1() {
            return Err(Fee(CarrierFeeRefusalV1::InsufficientFee));
        }
        // At most the preceding allocation and this single native charge.
        // The prior owner's lease stays live while building the new ledger.
        let owner = reserve_native_pending_requests_v1(fee_ledger_bytes_v1(2)).map_err(Resource)?;
        let mut candidate = CarrierFeeBudgetV1::clone(&self.budget);
        candidate.allocate_v1(identity.as_bytes(), amount).map_err(Fee)?;
        self.budget = NativeRetainedV1::from_owner_reserved_v1(candidate, owner);
        self.charged = true;
        self.charge_purpose = Some(purpose);
        Ok(())
    }
}

fn fee_ledger_bytes_v1(maximum_allocations: u64) -> u64 {
    (std::mem::size_of::<NativeRetainedV1<CarrierFeeBudgetV1>>() + std::mem::size_of::<[usize; 3]>()) as u64
        + maximum_allocations * btree_node_envelope_bytes_v1::<[u8; 64], u64>() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{
            PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        },
    };
    use misaka_palw_positive_contract::{
        identity::{Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES},
        public_one_honest::{ManifestRequest, OriginalId, PanelBond},
    };

    fn scope() -> ManifestRequestCarrierScopeV1 {
        ManifestRequestCarrierScopeV1 {
            claim: Hash64::from_u64_word(1),
            source: Hash64::from_u64_word(2),
            reservation_scope: Hash64::from_u64_word(3),
        }
    }

    // This fixture is about literal shape and fee accounting. Signature
    // authentication belongs to the separate native case owner.
    fn request() -> [u8; REQUEST_BYTES] {
        let signed = SignedManifestRequestV1 {
            request: ManifestRequest {
                original: OriginalId(Digest([4; 64])),
                manifest_root: Digest([5; 64]),
                claimant: Claimant::DrawnBond(PanelBond { transaction: Digest([6; 64]), index: 0 }),
                signer: PrincipalId(Digest([7; 64])),
                inspection_scope: Digest([8; 64]),
                supply_obligation: Digest([9; 64]),
                collect_until: 10,
                reservation: Digest([11; 64]),
            },
            key: [0; PUBKEY_BYTES],
            signature: [0; SIGNATURE_BYTES],
        };
        borsh::to_vec(&signed).unwrap().try_into().unwrap()
    }

    pub(super) fn tx() -> Transaction {
        Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(12), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(80, ScriptPublicKey::default())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            encode_manifest_request_carrier_v1(scope(), &request()).unwrap().to_vec(),
        )
    }

    pub(super) fn populated(tx: &Transaction, fee: u64) -> ValidatedTransaction<'_> {
        ValidatedTransaction::new(PopulatedTransaction::new(tx, vec![UtxoEntry::new(100, ScriptPublicKey::default(), 1, false)]), fee)
    }

    pub(super) fn point() -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: Hash64::from_u64_word(13), daa_score: 14, blue_score: 15, subsidy: 0 }
    }

    #[test]
    fn pmr1_manifest_request_carrier_rejects_noncanonical_transport() {
        let literal = request();
        let payload = encode_manifest_request_carrier_v1(scope(), &literal).unwrap();
        assert_eq!(decode_manifest_request_carrier_v1(&payload).unwrap(), Some((scope(), &literal)));
        assert!(decode_manifest_request_carrier_v1(&payload[..payload.len() - 1]).is_err());
        assert!(decode_manifest_request_carrier_v1(&[payload.as_slice(), &[0]].concat()).is_err());
        for index in [3, 4, HEADER_BYTES + 2 * 64] {
            let mut changed = payload;
            changed[index] ^= 0xff;
            assert!(decode_manifest_request_carrier_v1(&changed).is_err());
        }
        assert_eq!(decode_manifest_request_carrier_v1(b"another purpose"), Ok(None));

        let mut wrong_subnet = tx();
        wrong_subnet.subnetwork_id = SUBNETWORK_ID_NATIVE;
        wrong_subnet.finalize();
        assert!(matches!(
            ValidatedManifestRequestCarrierV1::prepare_v1(&populated(&wrong_subnet, 20), 0, point().block, point(), 0),
            Err(ManifestRequestCarrierErrorV1::Decode(_))
        ));
        let mut stale = tx();
        stale.outputs[0].value += 1;
        assert!(matches!(
            ValidatedManifestRequestCarrierV1::prepare_v1(&populated(&stale, 19), 0, point().block, point(), 0),
            Err(ManifestRequestCarrierErrorV1::Fee(CarrierFeeRefusalV1::StaleTransactionId))
        ));
    }

    #[test]
    fn pmr1_manifest_request_carrier_charges_only_actual_native_fee_once() {
        use ManifestRequestCarrierErrorV1::{Charge, Context, Fee};
        let tx = tx();
        let validated = populated(&tx, 20);
        let at = point();
        assert!(matches!(
            ValidatedManifestRequestCarrierV1::prepare_v1(&populated(&tx, 21), 0, at.block, at, 2),
            Err(Fee(CarrierFeeRefusalV1::ValidatedFeeMismatch))
        ));
        assert!(matches!(
            ValidatedManifestRequestCarrierV1::prepare_v1(&validated, 21, at.block, at, 2),
            Err(Fee(CarrierFeeRefusalV1::InsufficientFee))
        ));
        let mut handoff = ValidatedManifestRequestCarrierV1::prepare_v1(&validated, 8, at.block, at, 2).unwrap().unwrap();
        let expected =
            ManifestRequestCarrierContextV1 { scope: scope(), at, carrier_block: at.block, accepted_ordinal: 2, carrier: tx.id() };
        assert_eq!(handoff.context_v1(), expected);
        assert_eq!(handoff.request_at_v1(&expected).unwrap(), &request());
        assert_eq!(handoff.remaining_fee_v1(), 12);

        let purpose = Hash64::from_u64_word(99);
        for wrong in [
            ManifestRequestCarrierContextV1 { carrier_block: purpose, ..expected },
            ManifestRequestCarrierContextV1 { at: PalwBlockContextV2 { block: purpose, ..at }, ..expected },
            ManifestRequestCarrierContextV1 { at: PalwBlockContextV2 { daa_score: at.daa_score + 1, ..at }, ..expected },
            ManifestRequestCarrierContextV1 { at: PalwBlockContextV2 { blue_score: at.blue_score + 1, ..at }, ..expected },
            ManifestRequestCarrierContextV1 { accepted_ordinal: 3, ..expected },
            ManifestRequestCarrierContextV1 { carrier: purpose, ..expected },
            ManifestRequestCarrierContextV1 {
                scope: ManifestRequestCarrierScopeV1 { reservation_scope: purpose, ..scope() },
                ..expected
            },
        ] {
            assert_eq!(handoff.request_at_v1(&wrong), Err(Context));
            assert_eq!(handoff.allocate_charge_v1(&wrong, purpose, 1), Err(Context));
        }
        assert_eq!(handoff.allocate_charge_v1(&expected, purpose, 0), Err(Charge));
        assert_eq!(handoff.allocate_charge_v1(&expected, purpose, 13), Err(Fee(CarrierFeeRefusalV1::InsufficientFee)));
        assert_eq!(handoff.remaining_fee_v1(), 12);
        assert_eq!(handoff.allocate_charge_v1(&expected, purpose, 7), Ok(()));
        assert_eq!(handoff.remaining_fee_v1(), 5);
        assert_eq!(handoff.allocate_charge_v1(&expected, purpose, 1), Err(Fee(CarrierFeeRefusalV1::DuplicateAllocation)));
        assert_eq!(
            handoff.allocate_charge_v1(&expected, Hash64::from_u64_word(100), 1),
            Err(Fee(CarrierFeeRefusalV1::DuplicateAllocation)),
        );
        assert_eq!(handoff.remaining_fee_v1(), 5);
    }
}
