//! Deterministic terminal Court relations over openings of an authenticated
//! Original. The caller authenticates its receipt, EXEC input and deadline.
use super::{
    TreeKind, TreeRoot, check_capacity, is_eos,
    readout_chunks::{CHUNK_BYTES, encoded_len, record_count},
};
use crate::Hash64;
use std::io;

pub const LAST_CHUNK_BYTES: usize = 5 + 4 + 8 + 2 + 1 + 64;
pub const END_BYTES: usize = 5 + 3 * 64 + 3 * 4 + 8 + 4 + 4 + 8 + 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutcome {
    ChallengeNotEstablished,
    ComputationRefuted,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn stop_index(counts: [u64; 4]) -> io::Result<u64> {
    let t = u32::try_from(counts[1]).map_err(|_| invalid("Stop answer count"))?;
    counts[3].checked_sub(2 + u64::from(record_count(t).map_err(invalid)?)).ok_or_else(|| invalid("absent QF1 Stop slot"))
}

fn opened(root: &TreeRoot, i: u64, record: &[u8], path: &[Hash64]) -> bool {
    let depth = (u64::BITS - (root.count() - 1).leading_zeros()) as usize;
    if path.len() != depth {
        return false;
    }
    root.verify_opening(i, record, &path.iter().map(|h| h.as_bytes()).collect::<Vec<_>>())
}

pub struct Qf1TerminalContext<'a> {
    pub attempt: [u8; 32],
    pub counts: [u64; 4],
    pub roots: &'a [Hash64; 4],
    pub decode_limit: u32,
}

impl Qf1TerminalContext<'_> {
    pub fn judge_stop(
        &self,
        token: u32,
        stop: &[u8; 18],
        answer_path: &[Hash64],
        trace_path: &[Hash64],
    ) -> io::Result<TerminalOutcome> {
        let [l, t, _, d] = self.counts;
        let y = TreeRoot::from_claimed_digest(self.attempt, TreeKind::Answer, t, self.roots[1].as_bytes()).map_err(invalid)?;
        let trace = TreeRoot::from_claimed_digest(self.attempt, TreeKind::Trace, d, self.roots[3].as_bytes()).map_err(invalid)?;
        let l = u32::try_from(l).map_err(|_| invalid("Stop input count"))?;
        let t = u32::try_from(t).map_err(|_| invalid("Stop answer count"))?;
        check_capacity(l, self.decode_limit).map_err(invalid)?;
        if t == 0
            || t > self.decode_limit
            || !opened(&y, u64::from(t - 1), &token.to_le_bytes(), answer_path)
            || !opened(&trace, stop_index(self.counts)?, stop, trace_path)
        {
            return Err(invalid("unauthenticated Stop case"));
        }
        let reason = if is_eos(token) {
            0
        } else if t == self.decode_limit {
            1
        } else {
            return Ok(TerminalOutcome::ComputationRefuted);
        };
        let mut expected = [0; 18];
        expected[0] = 3;
        expected[1..5].copy_from_slice(&13u32.to_le_bytes());
        expected[5..9].copy_from_slice(&t.to_le_bytes());
        expected[9..13].copy_from_slice(&(l + t - 1).to_le_bytes());
        expected[13..17].copy_from_slice(&token.to_le_bytes());
        expected[17] = reason;
        Ok(if *stop == expected { TerminalOutcome::ChallengeNotEstablished } else { TerminalOutcome::ComputationRefuted })
    }

    pub fn judge_end(&self, wire: &[u8]) -> io::Result<TerminalOutcome> {
        let [l, t, z_count, d] = self.counts;
        let l = u32::try_from(l).map_err(|_| invalid("End input length"))?;
        let t = u32::try_from(t).map_err(|_| invalid("End output length"))?;
        check_capacity(l, self.decode_limit).map_err(invalid)?;
        let h = record_count(t).map_err(invalid)?;
        let tiles = d.checked_sub(u64::from(t) + u64::from(h) + 3).ok_or_else(|| invalid("End trace length"))?;
        if t > self.decode_limit || z_count != 1 {
            return Err(invalid("End counts outside registered frame"));
        }
        let depth = (u64::BITS - (d - 1).leading_zeros()) as usize;
        if depth > 63 || wire.len() != LAST_CHUNK_BYTES + END_BYTES + 2 * depth * 64 {
            return Err(invalid("noncanonical QF1 End disclosure length"));
        }
        let trace = TreeRoot::from_claimed_digest(self.attempt, TreeKind::Trace, d, self.roots[3].as_bytes()).map_err(invalid)?;
        let (first, last) = wire.split_at(LAST_CHUNK_BYTES + depth * 64);
        let (chunk, chunk_path) = first.split_at(LAST_CHUNK_BYTES);
        let (end, end_path) = last.split_at(END_BYTES);
        for (index, record, siblings) in [(d - 2, chunk, chunk_path), (d - 1, end, end_path)] {
            let path = siblings.chunks_exact(64).map(|hash| hash.try_into().unwrap()).collect::<Vec<_>>();
            if !trace.verify_opening(index, record, &path) {
                return Err(invalid("unauthenticated QF1 terminal record"));
            }
        }

        let bytes = encoded_len(t).map_err(invalid)?;
        let last_len = bytes - (h - 1) * CHUNK_BYTES as u32;
        let z = &chunk[LAST_CHUNK_BYTES - 64..];
        let rz = TreeRoot::from_claimed_digest(self.attempt, TreeKind::Readout, 1, self.roots[2].as_bytes()).map_err(invalid)?;
        let mut expected_chunk = vec![4];
        expected_chunk.extend_from_slice(&((LAST_CHUNK_BYTES - 5) as u32).to_le_bytes());
        expected_chunk.extend_from_slice(&(h - 1).to_le_bytes());
        expected_chunk.extend_from_slice(&u64::from(bytes).to_le_bytes());
        expected_chunk.extend_from_slice(&(last_len as u16).to_le_bytes());
        expected_chunk.push(1);
        expected_chunk.extend_from_slice(z);

        let mut expected_end = vec![5];
        expected_end.extend_from_slice(&((END_BYTES - 5) as u32).to_le_bytes());
        for root in &self.roots[..3] {
            expected_end.extend_from_slice(root.as_byte_slice());
        }
        for field in [l, t, l + t - 1] {
            expected_end.extend_from_slice(&field.to_le_bytes());
        }
        expected_end.extend_from_slice(&tiles.to_le_bytes());
        expected_end.extend_from_slice(&t.to_le_bytes());
        expected_end.extend_from_slice(&h.to_le_bytes());
        expected_end.extend_from_slice(&d.to_le_bytes());
        expected_end.extend_from_slice(z);
        let correct = chunk == expected_chunk && end == expected_end && rz.verify_opening(0, z, &[]);
        Ok(if correct { TerminalOutcome::ChallengeNotEstablished } else { TerminalOutcome::ComputationRefuted })
    }
}
