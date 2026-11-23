//! Body evidence for an isolated QF1 Original. The reader still authenticates
//! the Original against its EXEC input and the source block's commitment.
use super::{
    exec_close_carrier::ExecCloseCarrier,
    exec_registration::{ExecRegistration, ExecRegistrationProfile, registration_key},
    releases::Tb1BeaconSeed,
};
use crate::{
    Hash64,
    block::Block,
    coinbase::{coinbase_payload_extra_data, palw_coinbase_marker},
    palw_model_market_v1::moe2::{
        award_reference::{Moe2AwardReference, Moe2AwardSource},
        settlement::Moe2ExecSlot,
    },
    palw_state_v2::PalwChainStateV2,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
};
use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65Signature, MLDSA65VerificationKey};
use sha3::{Digest, Sha3_512};

const TAG: &[u8; 8] = b"TB1ORG01";
pub const QF1_ORIGINAL_BODY_BYTES: usize = 4 * (8 + 64) + 32;
const SIGNATURE_BYTES: usize = 3309;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qf1OriginalCarrierError {
    Missing,
    Multiple,
    Encoding,
}

pub struct Qf1OriginalCarrier<'a> {
    pub body: &'a [u8; QF1_ORIGINAL_BODY_BYTES],
    pub signature: &'a [u8; SIGNATURE_BYTES],
}

impl<'a> Qf1OriginalCarrier<'a> {
    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(TAG.len() + QF1_ORIGINAL_BODY_BYTES + SIGNATURE_BYTES);
        payload.extend_from_slice(TAG);
        payload.extend_from_slice(self.body);
        payload.extend_from_slice(self.signature);
        payload
    }

    pub fn decode(payload: &'a [u8]) -> Result<Self, Qf1OriginalCarrierError> {
        use Qf1OriginalCarrierError::Encoding;
        let fields = payload.strip_prefix(TAG).ok_or(Encoding)?;
        let (body, signature) = fields.split_first_chunk().ok_or(Encoding)?;
        Ok(Self { body, signature: signature.try_into().map_err(|_| Encoding)? })
    }

    pub fn from_block(block: &'a Block) -> Result<Self, Qf1OriginalCarrierError> {
        use Qf1OriginalCarrierError::{Missing, Multiple};
        let mut carriers =
            block.transactions.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(TAG));
        let carrier = carriers.next().ok_or(Missing)?;
        if carriers.next().is_some() {
            return Err(Multiple);
        }
        Self::decode(&carrier.payload)
    }
}

/// The signed subject and original deadline used by the native Court.
#[derive(Clone)]
pub struct Qf1OriginalAdmission {
    pub(crate) original: Hash64,
    pub(crate) block: Hash64,
    pub(crate) deadline: u64,
    pub(crate) panel_intake: Option<CertifiedQf1Original>,
}

#[derive(Clone)]
pub(crate) struct CertifiedQf1Original {
    pub close: Hash64,
    pub slot: u64,
    pub registration: Hash64,
    pub funding_ref: Hash64,
}

impl Qf1OriginalAdmission {
    pub fn prepare(
        chain: &[u8; 64],
        parent: &PalwChainStateV2,
        source: Moe2AwardSource<'_>,
        close_daa: u64,
        block: &Block,
    ) -> Result<Self, &'static str> {
        if parent.tb1_exec_close_receipt(source.reference.close).is_some() {
            return Err("recorded QF1 EXEC close requires its certificate");
        }
        source.reference.check_parent_reservation(parent, block.header.daa_score)?;
        let (_, registration) = authenticate_original(chain, source, close_daa, block)?;
        Ok(Self {
            original: source.reference.claim,
            block: block.hash(),
            deadline: registration.deadline_exclusive_daa,
            panel_intake: None,
        })
    }

    /// Authenticate a Court subject from the accepted EXEC close and seed.
    /// The caller supplies the accepted close body and checks ancestry of both
    /// the close and the certificate on the selected chain.
    pub fn prepare_certified(
        chain: &[u8; 64],
        parent: &PalwChainStateV2,
        source: Moe2AwardSource<'_>,
        close: &Block,
        block: &Block,
    ) -> Result<Self, &'static str> {
        let (_, registration, funding_ref) = authenticate_certified_original(chain, parent, source, close, block)?;
        Ok(Self {
            original: source.reference.claim,
            block: block.hash(),
            deadline: registration.deadline_exclusive_daa,
            panel_intake: Some(CertifiedQf1Original {
                close: source.reference.close,
                slot: source.reference.slot,
                registration: Hash64::from_bytes(registration.key),
                funding_ref: Hash64::from_bytes(funding_ref),
            }),
        })
    }
}

pub(super) fn authenticate_certified_original<'a, 'b>(
    chain: &[u8; 64],
    parent: &PalwChainStateV2,
    source: Moe2AwardSource<'_>,
    close: &'a Block,
    block: &'b Block,
) -> Result<(Qf1OriginalCarrier<'b>, ExecRegistration<'a>, [u8; 64]), &'static str> {
    let reference = source.reference;
    let receipt = parent.tb1_exec_close_receipt(reference.close).ok_or("missing accepted QF1 EXEC close")?;
    let certificate = parent.tb1_exec_certificate(reference.close).ok_or("missing accepted QF1 EXEC certificate")?;
    if certificate.freeze_id != receipt.freeze_id || close.hash() != reference.close {
        return Err("QF1 EXEC certificate differs from its close");
    }
    let (context, registration, funding_ref) = ExecCloseCarrier::read_accepted_registration(close, &receipt, reference.slot)
        .map_err(|_| "invalid QF1 EXEC close registration")?;
    let reservation = registration.quote.map_or_else(Hash64::default, |quote| {
        Moe2ExecSlot {
            registration: registration.key,
            slot: reference.slot,
            close: close.hash(),
            close_daa: close.header.daa_score,
            quote,
        }
        .reservation_key()
    });
    let attempt = crate::palw_qf1::attempt(context, &Tb1BeaconSeed::from_retained(certificate.seed))?;
    if source.registration_context().is_some_and(|supplied| supplied != context)
        || reference.reservation != reservation
        || reference.attempt != attempt
    {
        return Err("QF1 Original reference differs from the certified EXEC registration");
    }
    let original = verify_original(chain, reference, close.header.daa_score, block, &registration)?;
    Ok((original, registration, funding_ref))
}

pub(crate) fn authenticate_original<'a, 'b>(
    chain: &[u8; 64],
    source: Moe2AwardSource<'a>,
    close_daa: u64,
    block: &'b Block,
) -> Result<(Qf1OriginalCarrier<'b>, ExecRegistration<'a>), &'static str> {
    let reference = source.reference;
    source
        .open_registration(close_daa)
        .map_err(|error| error.reason())?
        .ok_or("QF1 Original requires the reserved EXEC registration context")?;
    let context = source.registration_context().ok_or("missing QF1 registration")?;
    let registration = ExecRegistration::decode(context, registration_key(context)).map_err(|_| "invalid QF1 registration")?;
    let original = verify_original(chain, reference, close_daa, block, &registration)?;
    Ok((original, registration))
}

fn verify_original<'a>(
    chain: &[u8; 64],
    reference: Moe2AwardReference,
    close_daa: u64,
    block: &'a Block,
    registration: &ExecRegistration<'_>,
) -> Result<Qf1OriginalCarrier<'a>, &'static str> {
    if !matches!(registration.profile, ExecRegistrationProfile::Qf1Local { .. }) {
        return Err("Original does not name a QF1 registration");
    }
    let original = Qf1OriginalCarrier::from_block(block).map_err(|_| "invalid native QF1 Original")?;
    if !(registration.open_daa..registration.deadline_exclusive_daa).contains(&block.header.daa_score)
        || close_daa > block.header.daa_score
        || signing_id(chain, &registration.key, reference.slot, &reference.attempt, original.body) != reference.claim
        || qf1_marker(block) != Some(reference.claim.as_bytes())
    {
        return Err("QF1 Original differs from its commitment or window");
    }
    let key = MLDSA65VerificationKey::new(*registration.owner);
    let signature = MLDSA65Signature::new(*original.signature);
    ml_dsa_65::verify(&key, reference.claim.as_byte_slice(), b"MISAKA/private/QF1/local-original-signature/v1", &signature)
        .map_err(|_| "invalid native QF1 Original owner signature")?;
    Ok(original)
}

pub(crate) fn qf1_marker(block: &Block) -> Option<[u8; 64]> {
    block
        .transactions
        .first()
        .filter(|tx| tx.is_coinbase())
        .and_then(|tx| coinbase_payload_extra_data(&tx.payload))
        .and_then(palw_coinbase_marker)
}

/// Canonical private Original identifier shared by production and Court.
pub fn signing_id(
    chain: &[u8; 64],
    registration: &[u8; 64],
    slot: u64,
    attempt: &[u8; 32],
    body: &[u8; QF1_ORIGINAL_BODY_BYTES],
) -> Hash64 {
    // The fixed carrier body stores counts then roots. The historical signing
    // input interleaves each count and its corresponding root.
    let mut hash = Sha3_512::new()
        .chain_update(b"MISAKA/private/QF1/local-original/v1")
        .chain_update(chain)
        .chain_update(registration)
        .chain_update(slot.to_be_bytes())
        .chain_update(attempt);
    for (count, root) in body[..32].chunks_exact(8).zip(body[32..288].chunks_exact(64)) {
        hash.update(count);
        hash.update(root);
    }
    hash.update(&body[288..]);
    Hash64::from_bytes(hash.finalize().into())
}
