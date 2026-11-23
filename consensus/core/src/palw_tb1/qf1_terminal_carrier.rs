//! Bounded public disclosure for a QF1 terminal Court case. The case and
//! verdict commitments and the Original's openings are checked by the reader.
use super::{
    exec_registration::ExecRegistration,
    qf1_original_carrier::{Qf1OriginalCarrier, authenticate_certified_original, authenticate_original, qf1_marker},
};
use crate::{
    Hash64,
    block::Block,
    palw_model_market_v1::moe2::award_reference::{Moe2AwardReference, Moe2AwardSource},
    palw_qf1::terminal::{END_BYTES, LAST_CHUNK_BYTES},
    palw_state_v2::PalwChainStateV2,
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

/// Authenticated outcome from a candidate's own Court carrier. It names the
/// exact Original. It can withhold an award but cannot authorize one.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Qf1TerminalRuling {
    pub(crate) original: Hash64,
    pub(crate) case: Hash64,
    pub(crate) verdict: [u8; 64],
    pub(crate) deadline: u64,
    pub(crate) refuted: bool,
}

impl Qf1TerminalRuling {
    /// The caller supplies the validated Original block from the candidate's
    /// selected history. The parent must fund the exact EXEC registration.
    pub fn prepare(
        chain: [u8; 64],
        parent: &PalwChainStateV2,
        source: Moe2AwardSource<'_>,
        slot_close_daa: u64,
        original_block: &Block,
        candidate: &Block,
    ) -> Result<Self, &'static str> {
        if parent.tb1_exec_close_receipt(source.reference.close).is_some() {
            return Err("recorded QF1 EXEC close requires its certificate");
        }
        source.reference.check_parent_reservation(parent, candidate.header.daa_score).map_err(|error| error.reason())?;
        let (original, registration) = authenticate_original(&chain, source, slot_close_daa, original_block)?;
        Self::judge(chain, source.reference, original_block.hash(), original, &registration, candidate)
    }

    /// The Original and close bodies come from the selected chain. Require
    /// the admitted Original's own reference before opening its commitments.
    pub fn prepare_certified(
        chain: [u8; 64],
        parent: &PalwChainStateV2,
        source: Moe2AwardSource<'_>,
        close: &Block,
        original_block: &Block,
        candidate: &Block,
    ) -> Result<Self, &'static str> {
        let reference = source.reference;
        let (admitted_block, deadline) = parent.qf1_original_receipt(reference.claim).ok_or("missing QF1 Original receipt")?;
        if admitted_block != original_block.hash()
            || Moe2AwardReference::from_transactions(original_block.transactions.iter()).map_err(|error| error.reason())? != reference
        {
            return Err("QF1 Court case differs from the admitted Original");
        }
        let (original, registration, _) = authenticate_certified_original(&chain, parent, source, close, original_block)?;
        if deadline != registration.deadline_exclusive_daa {
            return Err("QF1 Court case differs from the admitted deadline");
        }
        Self::judge(chain, reference, admitted_block, original, &registration, candidate)
    }

    fn judge(
        chain: [u8; 64],
        reference: Moe2AwardReference,
        original_block: Hash64,
        original: Qf1OriginalCarrier<'_>,
        registration: &ExecRegistration<'_>,
        candidate: &Block,
    ) -> Result<Self, &'static str> {
        use crate::palw_qf1::terminal::{Qf1TerminalContext, TerminalOutcome};
        use sha3::{Digest, Sha3_512};
        let carrier = Qf1TerminalCarrier::from_block(candidate).map_err(|_| "invalid native QF1 terminal carrier")?;
        let window = registration.open_daa..registration.deadline_exclusive_daa;
        if carrier.original_block != original_block || !window.contains(&candidate.header.daa_score) {
            return Err("QF1 terminal case differs from its committed Original or window");
        }
        let counts = std::array::from_fn(|i| u64::from_le_bytes(original.body[8 * i..8 * (i + 1)].try_into().unwrap()));
        let roots = std::array::from_fn(|i| Hash64::from_bytes(original.body[32 + 64 * i..32 + 64 * (i + 1)].try_into().unwrap()));
        let terminal =
            Qf1TerminalContext { attempt: reference.attempt, counts, roots: &roots, decode_limit: registration.decode_tokens };
        let hash = |domain| {
            Sha3_512::new()
                .chain_update(domain)
                .chain_update(chain)
                .chain_update(carrier.original_block.as_bytes())
                .chain_update(reference.claim.as_bytes())
        };
        let (outcome, case_marker, verdict_domain): (_, [u8; 64], &[u8]) = match carrier.relation {
            Qf1TerminalRelation::Stop => {
                let (record, paths) = carrier.disclosure.split_first_chunk::<22>().ok_or("short QF1 Stop disclosure")?;
                let depth = |count: u64| count.checked_sub(1).map(|n| (u64::BITS - n.leading_zeros()) as usize);
                let answer = depth(counts[1]).ok_or("empty QF1 answer tree")?;
                let trace = depth(counts[3]).ok_or("empty QF1 trace tree")?;
                if answer > 63 || trace > 63 || paths.len() != (answer + trace) * 64 {
                    return Err("invalid QF1 Stop paths");
                }
                let (a, t) = paths.split_at(answer * 64);
                let decode =
                    |path: &[u8]| path.chunks_exact(64).map(|word| Hash64::from_bytes(word.try_into().unwrap())).collect::<Vec<_>>();
                let token = u32::from_le_bytes(record[..4].try_into().unwrap());
                let outcome = terminal.judge_stop(token, record[4..].try_into().unwrap(), &decode(a), &decode(t));
                let mut case = hash(b"MISAKA/private/QF1/court/last-answer-stop/v1".as_slice()).chain_update(record);
                case.update([answer as u8]);
                case.update(a);
                case.update([trace as u8]);
                case.update(t);
                (outcome, case.finalize().into(), b"MISAKA/private/QF1/court/last-answer-stop/verdict/v1")
            }
            Qf1TerminalRelation::End => (
                terminal.judge_end(carrier.disclosure),
                hash(b"MISAKA/private/QF1/court/readout-end/v1".as_slice()).chain_update(carrier.disclosure).finalize().into(),
                b"MISAKA/private/QF1/court/readout-end/verdict/v1",
            ),
        };
        let refuted = outcome.map_err(|_| "invalid native QF1 terminal disclosure")? == TerminalOutcome::ComputationRefuted;
        if qf1_marker(candidate) != Some(case_marker) {
            return Err("QF1 case differs from the candidate commitment");
        }
        let verdict = Sha3_512::new()
            .chain_update(verdict_domain)
            .chain_update(candidate.hash().as_bytes())
            .chain_update(case_marker)
            .chain_update([u8::from(refuted)])
            .finalize()
            .into();
        Ok(Self { original: reference.claim, case: candidate.hash(), verdict, deadline: window.end, refuted })
    }
}
