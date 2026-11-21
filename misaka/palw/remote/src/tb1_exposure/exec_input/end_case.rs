//! Direct comparison of the last Readout chunk and End to the signed Original.
//! This relation checks counters and the terminal digest. The computation of a
//! compression state from its source vectors is a separate local relation.
use super::terminal_case::{LocalTerminalCase, TerminalOutcome};
use super::*;
use kaspa_consensus_core::palw_qf1::{
    TreeKind, TreeRoot, check_capacity,
    readout_chunks::{CHUNK_BYTES, encoded_len, record_count},
};

const LAST_CHUNK_BYTES: usize = 5 + 4 + 8 + 2 + 1 + 64;
const END_BYTES: usize = 5 + 3 * 64 + 3 * 4 + 8 + 4 + 4 + 8 + 64;

impl LocalOriginal<'_, '_> {
    /// Both records and their Merkle paths come from the producer's original trace pass.
    pub fn end_disclosure(&self) -> io::Result<Vec<u8>> {
        let mut wire = Vec::new();
        for (open, size) in self.trace().end_openings.iter().zip([LAST_CHUNK_BYTES, END_BYTES]) {
            if open.record.len() != size {
                return Err(invalid("unexpected QF1 terminal record size"));
            }
            wire.extend_from_slice(&open.record);
            for sibling in &open.siblings {
                wire.extend_from_slice(sibling);
            }
        }
        Ok(wire)
    }
}

fn judge(attempt: [u8; 32], body: &LocalOriginalBody, limit: u32, wire: &[u8]) -> io::Result<TerminalOutcome> {
    let [l, t, z_count, d] = body.counts;
    let l = u32::try_from(l).map_err(|_| invalid("End input length"))?;
    let t = u32::try_from(t).map_err(|_| invalid("End output length"))?;
    check_capacity(l, limit).map_err(invalid)?;
    let h = record_count(t).map_err(invalid)?;
    let tiles = d.checked_sub(u64::from(t) + u64::from(h) + 3).ok_or_else(|| invalid("End trace length"))?;
    if t > limit || z_count != 1 {
        return Err(invalid("End counts outside registered frame"));
    }
    let depth = (u64::BITS - (d - 1).leading_zeros()) as usize;
    if depth > 63 || wire.len() != LAST_CHUNK_BYTES + END_BYTES + 2 * depth * 64 {
        return Err(invalid("noncanonical QF1 End disclosure length"));
    }
    let trace = TreeRoot::from_claimed_digest(attempt, TreeKind::Trace, d, body.roots[3].as_bytes()).map_err(invalid)?;
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
    let rz = TreeRoot::from_claimed_digest(attempt, TreeKind::Readout, 1, body.roots[2].as_bytes()).map_err(invalid)?;
    let mut expected_chunk = vec![4];
    expected_chunk.extend_from_slice(&((LAST_CHUNK_BYTES - 5) as u32).to_le_bytes());
    expected_chunk.extend_from_slice(&(h - 1).to_le_bytes());
    expected_chunk.extend_from_slice(&u64::from(bytes).to_le_bytes());
    expected_chunk.extend_from_slice(&(last_len as u16).to_le_bytes());
    expected_chunk.push(1);
    expected_chunk.extend_from_slice(z);

    let mut expected_end = vec![5];
    expected_end.extend_from_slice(&((END_BYTES - 5) as u32).to_le_bytes());
    for root in &body.roots[..3] {
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

impl<'t> DerivedExecInput<'t> {
    pub fn verify_qf1_end_disclosure(
        &self,
        native: &dyn ConsensusApi,
        body: &LocalOriginalBody,
        owner: &MLDSA65Signature,
        wire: &[u8],
    ) -> io::Result<(Hash64, TerminalOutcome)> {
        let original = self.verify_qf1_body(native, body, owner)?;
        Ok((original, judge(self.attempt, body, self.terms.decode_tokens, wire)?))
    }

    pub fn prepare_end_case(
        &self,
        native: &dyn ConsensusApi,
        receipt: Hash64,
        body: &LocalOriginalBody,
        owner: &MLDSA65Signature,
        wire: &[u8],
    ) -> io::Result<LocalTerminalCase<'_, 't>> {
        let original = self.read_qf1_original(native, receipt, body, owner)?;
        let outcome = judge(self.attempt, body, self.terms.decode_tokens, wire)?;
        let marker = Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/court/readout-end/v1")
            .chain_update(self.chain)
            .chain_update(receipt.as_bytes())
            .chain_update(original.as_bytes())
            .chain_update(wire)
            .finalize()
            .into();
        Ok(LocalTerminalCase {
            input: self,
            receipt,
            marker,
            outcome,
            verdict_domain: b"MISAKA/private/QF1/court/readout-end/verdict/v1",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_qf1::{ROLES, Readout, TreeBuilder, WIDTH, ticket};

    #[test]
    fn authenticates_both_records_before_adjudicating_the_terminal_link() {
        let attempt = [17; 32];
        let prompt = 7u32.to_le_bytes();
        let answer = 151645u32;
        let mut readout = Readout::default();
        readout.step(&[&[3i16; WIDTH]; ROLES]).unwrap();
        let (z, chunks) = readout.finish(1, &[answer], 1).unwrap();
        let last = chunks.last().unwrap();
        let input = TreeRoot::build(attempt, TreeKind::Input, std::iter::once(prompt.as_slice()), 1).unwrap();
        let y = TreeRoot::build(attempt, TreeKind::Answer, std::iter::once(answer.to_le_bytes().as_slice()), 1).unwrap();
        let rz = TreeRoot::build(attempt, TreeKind::Readout, std::iter::once(z.as_slice()), 1).unwrap();
        let h = chunks.len() as u32;
        let tiles = 2u64;
        let d = tiles + 4 + u64::from(h);
        let mut chunk = vec![4];
        chunk.extend_from_slice(&79u32.to_le_bytes());
        chunk.extend_from_slice(&last.index.to_le_bytes());
        chunk.extend_from_slice(&last.consumed.to_le_bytes());
        chunk.extend_from_slice(&last.length.to_le_bytes());
        chunk.push(u8::from(last.final_flag));
        chunk.extend_from_slice(&last.chain_after);
        let mut end = vec![5];
        end.extend_from_slice(&292u32.to_le_bytes());
        for r in [&input, &y, &rz] {
            end.extend_from_slice(&r.digest);
        }
        for v in [1u32, 1, 1] {
            end.extend_from_slice(&v.to_le_bytes());
        }
        end.extend_from_slice(&tiles.to_le_bytes());
        end.extend_from_slice(&1u32.to_le_bytes());
        end.extend_from_slice(&h.to_le_bytes());
        end.extend_from_slice(&d.to_le_bytes());
        end.extend_from_slice(&z);

        for change in
            [None, Some((false, 9)), Some((false, 19)), Some((false, 20)), Some((true, 5)), Some((true, 225)), Some((true, 233))]
        {
            let (mut c, mut e) = (chunk.clone(), end.clone());
            if let Some((at_end, byte)) = change {
                if at_end {
                    e[byte] ^= 1;
                } else {
                    c[byte] ^= 1;
                }
            }
            let mut tree = TreeBuilder::with_openings(attempt, TreeKind::Trace, d, d, [Some(d - 2), Some(d - 1)]).unwrap();
            for _ in 0..d - 2 {
                tree.push(&[0]).unwrap();
            }
            tree.push(&c).unwrap();
            tree.push(&e).unwrap();
            let (root, [a, b]) = tree.finish_with_openings().unwrap();
            let roots = [&input, &y, &rz, &root];
            let body = LocalOriginalBody {
                counts: roots.map(|r| r.count()),
                roots: roots.map(|r| Hash64::from_bytes(r.digest)),
                ticket: ticket(roots).unwrap(),
            };
            let mut wire = c;
            wire.extend(a.concat());
            wire.extend(e);
            wire.extend(b.concat());
            let expected =
                if change.is_some() { TerminalOutcome::ComputationRefuted } else { TerminalOutcome::ChallengeNotEstablished };
            assert_eq!(judge(attempt, &body, 1, &wire).unwrap(), expected);
            wire[0] ^= 1;
            assert!(judge(attempt, &body, 1, &wire).is_err());
            wire[0] ^= 1;
            *wire.last_mut().unwrap() ^= 1;
            assert!(judge(attempt, &body, 1, &wire).is_err());
            assert!(judge(attempt, &body, 1, &wire[..wire.len() - 1]).is_err());
        }
    }
}
