//! Body evidence for an isolated QF1 Original. The reader still authenticates
//! the Original against its EXEC input and the source block's commitment.
use crate::{block::Block, subnets::SUBNETWORK_ID_PALW_LIFECYCLE};

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
