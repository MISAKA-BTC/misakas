//! Body evidence for an isolated QF1 Original. The reader still authenticates
//! the Original against its EXEC input and the source block's commitment.
use super::exec_registration::{ExecRegistration, ExecRegistrationProfile, registration_key};
use crate::{
    Hash64,
    block::Block,
    coinbase::{coinbase_payload_extra_data, palw_coinbase_marker},
    palw_model_market_v1::moe2::award_reference::Moe2AwardSource,
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
}

impl Qf1OriginalAdmission {
    pub fn prepare(
        chain: &[u8; 64],
        parent: &PalwChainStateV2,
        source: Moe2AwardSource<'_>,
        close_daa: u64,
        block: &Block,
    ) -> Result<Self, &'static str> {
        source.reference.check_parent_reservation(parent, block.header.daa_score)?;
        let (_, registration) = authenticate_original(chain, source, close_daa, block)?;
        Ok(Self { original: source.reference.claim, block: block.hash(), deadline: registration.deadline_exclusive_daa })
    }
}

pub(crate) fn authenticate_original<'a, 'b>(
    chain: &[u8; 64],
    source: Moe2AwardSource<'a>,
    close_daa: u64,
    block: &'b Block,
) -> Result<(Qf1OriginalCarrier<'b>, ExecRegistration<'a>), &'static str> {
    let reference = source.reference;
    let (_, slot) = source
        .open_registration(close_daa)
        .map_err(|error| error.reason())?
        .ok_or("QF1 Original requires the reserved EXEC registration context")?;
    let context = source.registration_context().ok_or("missing QF1 registration")?;
    let registration = ExecRegistration::decode(context, registration_key(context)).map_err(|_| "invalid QF1 registration")?;
    if !matches!(registration.profile, ExecRegistrationProfile::Qf1Local { .. }) {
        return Err("Original does not name a QF1 registration");
    }
    let original = Qf1OriginalCarrier::from_block(block).map_err(|_| "invalid native QF1 Original")?;
    if !(registration.open_daa..registration.deadline_exclusive_daa).contains(&block.header.daa_score)
        || close_daa > block.header.daa_score
        || signing_id(chain, &registration.key, slot.slot, &reference.attempt, original.body) != reference.claim
        || qf1_marker(block) != Some(reference.claim.as_bytes())
    {
        return Err("QF1 Original differs from its commitment or window");
    }
    let key = MLDSA65VerificationKey::new(*registration.owner);
    let signature = MLDSA65Signature::new(*original.signature);
    ml_dsa_65::verify(&key, reference.claim.as_byte_slice(), b"MISAKA/private/QF1/local-original-signature/v1", &signature)
        .map_err(|_| "invalid native QF1 Original owner signature")?;
    Ok((original, registration))
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
