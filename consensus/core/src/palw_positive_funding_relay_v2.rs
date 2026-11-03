//! Fixed RFC-0046 body14 allocation owner. This is transport DATA, not native
//! R admission. The actual intake must select its separate fired fence BEFORE
//! invoking this scanner, then resolve lineage, bond/key, fee and custody.
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDemandLeaseV1, TirNativeRequestsV1, reserve_native_requests_v1,
    wire::{NativeWireReaderV1, NativeWireScanV1},
};
use misaka_palw_positive_contract::{
    ContractError,
    assignment::funding_relay::{FUNDING_RELAY_MAX_BODY_BYTES_V2, FundingRelayRegistrationBodyV2},
    identity::PUBKEY_BYTES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundingRelayBodyQuoteV2 {
    pub encoded_bytes: u64,
    pub scan_steps: u64,
    pub decoded_inline_bytes: u64,
    pub logical_decode_requests: u64,
    /// Purpose consent and ordinary relay transaction signature. Outer Full
    /// carrier signatures are independent and never included by assertion.
    pub signatures: u32,
}
#[derive(Debug)]
pub enum FundingRelayBodyErrorV2 {
    Contract(ContractError),
    Native(NativeCoverageV1),
}
impl From<NativeCoverageV1> for FundingRelayBodyErrorV2 {
    fn from(error: NativeCoverageV1) -> Self {
        Self::Native(error)
    }
}
fn malformed() -> FundingRelayBodyErrorV2 {
    FundingRelayBodyErrorV2::Contract(ContractError::NonCanonical)
}
fn budget() -> FundingRelayBodyErrorV2 {
    FundingRelayBodyErrorV2::Contract(ContractError::Budget)
}

/// Allocation-free structural scan over exact literal bytes, with finite enums
/// and fixed signatures only. A peer-supplied quote is never consumed here.
pub fn quote_funding_relay_body_v2(raw: &[u8], maximum_frame: u64) -> Result<FundingRelayBodyQuoteV2, FundingRelayBodyErrorV2> {
    if maximum_frame == 0
        || maximum_frame == u64::MAX
        || raw.len() as u64 > maximum_frame
        || raw.len() > FUNDING_RELAY_MAX_BODY_BYTES_V2
    {
        return Err(budget());
    }
    if raw.get(..5) != Some(&[2, 0, 1, 0, 14]) {
        return Err(malformed());
    }
    let mut reader = NativeWireReaderV1::new(raw, raw.len() as u64);
    let shape = FundingRelayRegistrationBodyV2::scan_native_wire_v1(&mut reader).map_err(|_| malformed())?;
    if reader.remaining() != 0 || shape.heap != 0 || shape.collection_entries != 0 {
        return Err(malformed());
    }
    let mut q = TirNativeRequestsV1::new();
    // Borsh construction/move scratch and retained literal body are distinct
    // fixed inline owners. No arbitrary byte-count multiplier prices them.
    q.exact_v1::<FundingRelayRegistrationBodyV2>(2)?;
    q.exact_v1::<FundingRelayBodyQuoteV2>(1)?;
    q.exact_v1::<NativeDemandLeaseV1>(1)?;
    Ok(FundingRelayBodyQuoteV2 {
        encoded_bytes: raw.len() as u64,
        scan_steps: reader.observed_steps_v1(),
        decoded_inline_bytes: std::mem::size_of::<FundingRelayRegistrationBodyV2>() as u64,
        logical_decode_requests: q.bytes_v1(),
        signatures: 2,
    })
}

/// Retains the real typed reservation. A detached Body or a caller-rebuilt
/// quote is still DATA; no accepted origin or saved signature-success is kept.
pub struct ReservedFundingRelayBodyV2 {
    body: FundingRelayRegistrationBodyV2,
    quote: FundingRelayBodyQuoteV2,
    _lease: NativeDemandLeaseV1,
}
impl ReservedFundingRelayBodyV2 {
    pub fn decode(raw: &[u8], maximum_frame: u64, maximum_decode_requests: u64) -> Result<Self, FundingRelayBodyErrorV2> {
        let quote = quote_funding_relay_body_v2(raw, maximum_frame)?;
        if maximum_decode_requests == 0
            || maximum_decode_requests == u64::MAX
            || quote.logical_decode_requests > maximum_decode_requests
        {
            return Err(budget());
        }
        let lease = reserve_native_requests_v1(quote.logical_decode_requests)?.ok_or_else(|| {
            FundingRelayBodyErrorV2::Native(NativeCoverageV1::unsupported::<Self>(
                "relay body14 decoder",
                "actual native request owner is not armed",
            ))
        })?;
        let body = FundingRelayRegistrationBodyV2::decode(raw, maximum_frame).map_err(FundingRelayBodyErrorV2::Contract)?;
        Ok(Self { body, quote, _lease: lease })
    }
    pub fn body(&self) -> &FundingRelayRegistrationBodyV2 {
        &self.body
    }
    pub fn quote(&self) -> FundingRelayBodyQuoteV2 {
        self.quote
    }
}

/// Authenticate the purpose and the standard settlement template from the
/// same literal registration. The key must be resolved by the caller's
/// native state/lineage checks. This grants no admission or custody.
pub fn verify_funding_relay_package_signatures_v2(
    body: &FundingRelayRegistrationBodyV2,
    payer_key: &[u8; PUBKEY_BYTES],
    maximum_requests: u64,
) -> Result<(), FundingRelayBodyErrorV2> {
    use crate::{
        palw_positive_relay_signature_v2::{ASSIGNMENT_RELAY_PAYLOAD_BYTES_V2, FundingRelaySignatureTemplateV2},
        tx::{TransactionInput, TransactionOutpoint},
    };

    let terms = &body.terms;
    if terms.refund_payload.0 != kaspa_hashes::blake2b_512_address_payload(payer_key).as_bytes() {
        return Err(FundingRelayBodyErrorV2::Contract(ContractError::Identity));
    }
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<FundingRelaySignatureTemplateV2>(1)?;
    requests.exact_v1::<TransactionInput>(1)?;
    // Literal payload and the P2PKH script Vec plus ScriptVec copy.
    requests.exact_v1::<u8>((ASSIGNMENT_RELAY_PAYLOAD_BYTES_V2 + 2 * 69) as u64)?;
    let bytes = requests.bytes_v1();
    if maximum_requests == u64::MAX || bytes > maximum_requests {
        return Err(budget());
    }
    let _lease = reserve_native_requests_v1(bytes)?.ok_or_else(|| {
        FundingRelayBodyErrorV2::Native(NativeCoverageV1::unsupported::<FundingRelaySignatureTemplateV2>(
            "relay signature template",
            "native owner is not armed",
        ))
    })?;
    terms.authenticate_purpose(payer_key, &body.purpose_signature).map_err(FundingRelayBodyErrorV2::Contract)?;
    let point = terms.funding_outpoint;
    FundingRelaySignatureTemplateV2::new(
        terms.assignment,
        TransactionOutpoint::new(crate::Hash64::from_bytes(point.transaction.0), point.index),
        terms.expected_amount,
        *payer_key,
    )
    .and_then(|template| template.verify(&body.relay_signature))
    .map_err(|_| FundingRelayBodyErrorV2::Contract(ContractError::Signature))
}

/// Match canonical carrier framing and every signed byte without allocating a
/// second body buffer while the historical replay lease is held.
pub fn funding_relay_carrier_matches_v2(body: &FundingRelayRegistrationBodyV2, payload: &[u8]) -> bool {
    payload.strip_prefix(&[1, 0, 176]).is_some_and(|encoded| canonical_equal(body, encoded))
}

/// Workspace held across selected-parent archive and payer replay.
pub fn pending_funding_relay_archive_workspace_v2() -> u64 {
    use misaka_palw_positive_contract::rows::NativeRowValue;
    (4 * size_of::<NativeRowValue>() + 2 * size_of::<FundingRelayRegistrationBodyV2>() + 2 * 1024 + 2 * 74) as u64
}

/// Scratch for the bounded payer projection and its purpose-specific row.
pub fn pending_funding_relay_payer_scratch_v2(
    purpose: misaka_palw_positive_contract::assignment::funding_relay::RelayFundingPurposeV2,
) -> u64 {
    use misaka_palw_positive_contract::{
        assignment::{
            funding_relay::RelayFundingPurposeV2,
            rows::{AssignmentNonceRow, AssignmentRow},
        },
        rows::{MAX_NATIVE_KEY_BYTES, MAX_NATIVE_ROW_BYTES},
    };
    let common = 4 * MAX_NATIVE_KEY_BYTES + 3 * size_of::<AssignmentNonceRow>();
    let case = MAX_NATIVE_ROW_BYTES + 4 * size_of::<AssignmentRow>();
    (common + if purpose == RelayFundingPurposeV2::CaseC { case } else { 0 }) as u64
}

/// Minimum for the two new R rows and mandatory final point. Every actual
/// delta, which may contain other entries, still requires its own preflight.
pub(crate) fn minimum_pending_funding_relay_replay_heap_v2(
    terms: &misaka_palw_positive_contract::assignment::funding_relay::FundingRelayTermsV2,
) -> Result<u64, ContractError> {
    use crate::palw_state_v2::{PalwDeltaEntryV2, PalwStateDeltaV2};
    use misaka_palw_positive_contract::assignment::{
        funding_relay_policy::FundingRelayStorageExtentV2, funding_relay_rows::FUNDING_RELAY_CHARGE_BYTES_V2,
    };
    let extent = FundingRelayStorageExtentV2::for_terms(terms)?;
    let aux = extent
        .retained_bytes
        .checked_sub(extent.body_bytes)
        .and_then(|n| n.checked_sub((3 + FUNDING_RELAY_CHARGE_BYTES_V2) as u64))
        .ok_or(ContractError::Budget)?;
    // Native wire preflight charges twice the count for both the encoded
    // row/key Vecs and the three PalwDeltaEntryV2 entries.
    let delta = aux
        .checked_mul(2)
        .and_then(|n| n.checked_add((size_of::<PalwStateDeltaV2>() + 6 * size_of::<PalwDeltaEntryV2>()) as u64))
        .ok_or(ContractError::Budget)?;
    pending_funding_relay_archive_workspace_v2()
        .checked_add(pending_funding_relay_payer_scratch_v2(terms.funding_purpose))
        .and_then(|n| n.checked_add(delta))
        .ok_or(ContractError::Budget)
}

/// Match the registered row bodies against a historical pending pair without
/// allocating or decoding another copy of either fixed-size signature row.
pub fn pending_funding_relay_row_images_match_v2(
    authorization: &misaka_palw_positive_contract::assignment::funding_relay_rows::FundingRelayAuthorizationRowV2,
    custody: &misaka_palw_positive_contract::assignment::funding_relay_rows::FundingRelayCustodyRowV2,
    authorization_bytes: &[u8],
    custody_bytes: &[u8],
) -> bool {
    custody.binding.is_none()
        && authorization_bytes.strip_prefix(&[2, 0, 1, 0, 19]).is_some_and(|raw| canonical_equal(authorization, raw))
        && custody_bytes.strip_prefix(&[2, 0, 1, 0, 20]).is_some_and(|raw| canonical_equal(custody, raw))
}

/// Full canonical byte comparison without an additional body/transaction Vec.
pub(crate) fn canonical_equal<T: borsh::BorshSerialize>(value: &T, expected: &[u8]) -> bool {
    struct Compare<'a> {
        expected: &'a [u8],
        at: usize,
    }
    impl std::io::Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let end = self.at.checked_add(bytes.len()).ok_or(std::io::ErrorKind::InvalidData)?;
            if self.expected.get(self.at..end) != Some(bytes) {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            self.at = end;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare { expected, at: 0 };
    borsh::to_writer(&mut compare, value).is_ok() && compare.at == expected.len()
}
