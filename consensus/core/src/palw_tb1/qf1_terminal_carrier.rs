//! Bounded public disclosure for a QF1 terminal Court case. The case and
//! verdict commitments and the Original's openings are checked by the reader.
use crate::{
    Hash64,
    block::Block,
    palw_qf1::terminal::{END_BYTES, LAST_CHUNK_BYTES},
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
};

const TAG: &[u8; 8] = b"TB1QFC01";
// The larger End disclosure has 377 record bytes and two paths of at most
// 63 digests each. The particular Original fixes the exact lengths.
const MAX_DISCLOSURE_BYTES: usize = END_BYTES + LAST_CHUNK_BYTES + 2 * 63 * 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qf1TerminalRelation {
    Stop,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qf1TerminalCarrierError {
    Missing,
    Multiple,
    Encoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Qf1TerminalCarrier<'a> {
    pub relation: Qf1TerminalRelation,
    pub original_block: Hash64,
    pub disclosure: &'a [u8],
}

impl<'a> Qf1TerminalCarrier<'a> {
    pub fn encode(&self) -> Result<Vec<u8>, Qf1TerminalCarrierError> {
        if self.disclosure.is_empty() || self.disclosure.len() > MAX_DISCLOSURE_BYTES {
            return Err(Qf1TerminalCarrierError::Encoding);
        }
        let mut payload = Vec::with_capacity(TAG.len() + 1 + 64 + self.disclosure.len());
        payload.extend_from_slice(TAG);
        payload.push(match self.relation {
            Qf1TerminalRelation::Stop => 0,
            Qf1TerminalRelation::End => 1,
        });
        payload.extend_from_slice(self.original_block.as_byte_slice());
        payload.extend_from_slice(self.disclosure);
        Ok(payload)
    }

    pub fn decode(payload: &'a [u8]) -> Result<Self, Qf1TerminalCarrierError> {
        use Qf1TerminalCarrierError::Encoding;
        let (tag, fields) = payload.strip_prefix(TAG).and_then(|bytes| bytes.split_first()).ok_or(Encoding)?;
        let relation = match tag {
            0 => Qf1TerminalRelation::Stop,
            1 => Qf1TerminalRelation::End,
            _ => return Err(Encoding),
        };
        let (block, disclosure) = fields.split_first_chunk::<64>().ok_or(Encoding)?;
        if disclosure.is_empty() || disclosure.len() > MAX_DISCLOSURE_BYTES {
            return Err(Encoding);
        }
        Ok(Self { relation, original_block: Hash64::from_bytes(*block), disclosure })
    }

    pub fn from_block(block: &'a Block) -> Result<Self, Qf1TerminalCarrierError> {
        use Qf1TerminalCarrierError::{Missing, Multiple};
        let mut carriers =
            block.transactions.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(TAG));
        let carrier = carriers.next().ok_or(Missing)?;
        if carriers.next().is_some() {
            return Err(Multiple);
        }
        Self::decode(&carrier.payload)
    }
}
