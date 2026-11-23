//! Body evidence for an isolated QF1 Original. The reader still authenticates
//! the Original against its EXEC input and the source block's commitment.
use crate::{Hash64, block::Block, subnets::SUBNETWORK_ID_PALW_LIFECYCLE};
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
