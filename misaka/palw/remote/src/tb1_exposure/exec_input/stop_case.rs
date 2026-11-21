//! Model-free adjudication of the last answer and the fixed Stop slot.
//! Consistency here gives no verdict on earlier tokens or the model arithmetic.
use super::*;
use kaspa_consensus_core::palw_qf1::{TreeBuilder, TreeKind, TreeRoot, is_eos, readout_chunks::record_count};

#[derive(Clone, Debug)]
pub struct StopWitness {
    pub token: u32,
    pub stop: [u8; 18],
    pub answer_path: Vec<Hash64>,
    pub trace_path: Vec<Hash64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopOutcome {
    ChallengeNotEstablished,
    ComputationRefuted,
}

impl LocalOriginalBody {
    pub fn stop_index(&self) -> io::Result<u64> {
        let t = u32::try_from(self.counts[1]).map_err(|_| invalid("Stop answer count"))?;
        self.counts[3].checked_sub(2 + u64::from(record_count(t).map_err(invalid)?)).ok_or_else(|| invalid("absent QF1 Stop slot"))
    }
}

impl LocalOriginal<'_, '_> {
    pub fn stop_witness(&self) -> io::Result<StopWitness> {
        let trace = self.trace();
        let answer = &trace.answer.tokens;
        let token = *answer.last().ok_or_else(|| invalid("empty answer"))?;
        let n = answer.len() as u64;
        let i = self.body().stop_index()?;
        let open = &trace.stop_opening;
        if open.index != i {
            return Err(invalid("Stop disclosure index"));
        }
        let mut tree = TreeBuilder::new(self.input.attempt, TreeKind::Answer, n, n, Some(n - 1)).map_err(invalid)?;
        for id in answer {
            tree.push(&id.to_le_bytes()).map_err(invalid)?;
        }
        let (root, path) = tree.finish().map_err(invalid)?;
        if root.digest != trace.roots[1].digest {
            return Err(invalid("answer opening differs from replay"));
        }
        Ok(StopWitness {
            token,
            stop: open.record.as_slice().try_into().map_err(|_| invalid("unsupported Stop width"))?,
            answer_path: path.into_iter().map(Hash64::from_bytes).collect(),
            trace_path: open.siblings.iter().copied().map(Hash64::from_bytes).collect(),
        })
    }
}

fn opened(root: &TreeRoot, i: u64, record: &[u8], path: &[Hash64]) -> bool {
    let depth = (u64::BITS - (root.count() - 1).leading_zeros()) as usize;
    if path.len() != depth {
        return false;
    }
    root.verify_opening(i, record, &path.iter().map(|h| h.as_bytes()).collect::<Vec<_>>())
}

impl StopWitness {
    pub fn encode(&self) -> Vec<u8> {
        let mut wire = Vec::new();
        wire.extend_from_slice(&self.token.to_le_bytes());
        wire.extend_from_slice(&self.stop);
        for hash in self.answer_path.iter().chain(&self.trace_path) {
            wire.extend_from_slice(hash.as_byte_slice());
        }
        wire
    }

    /// The signed counts determine both path lengths before allocation. The
    /// wire contains just the token, the fixed-width record, and the siblings.
    pub fn decode(body: &LocalOriginalBody, wire: &[u8]) -> io::Result<Self> {
        let depth =
            |n: u64| n.checked_sub(1).map(|m| (u64::BITS - m.leading_zeros()) as usize).ok_or_else(|| invalid("empty Stop tree"));
        let a = depth(body.counts[1])?;
        let t = depth(body.counts[3])?;
        if a > 63 || t > 63 || wire.len() != 22 + (a + t) * 64 {
            return Err(invalid("noncanonical Stop disclosure length"));
        }
        let token = u32::from_le_bytes(wire[..4].try_into().unwrap());
        let stop = wire[4..22].try_into().unwrap();
        let hashes = |s: &[u8]| s.chunks_exact(64).map(|b| Hash64::from_bytes(b.try_into().unwrap())).collect();
        let (answer, trace) = wire[22..].split_at(a * 64);
        Ok(Self { token, stop, answer_path: hashes(answer), trace_path: hashes(trace) })
    }

    fn judge(&self, attempt: [u8; 32], body: &LocalOriginalBody, limit: u32) -> io::Result<StopOutcome> {
        let [l, t, _, d] = body.counts;
        let y = TreeRoot::from_claimed_digest(attempt, TreeKind::Answer, t, body.roots[1].as_bytes()).map_err(invalid)?;
        let trace = TreeRoot::from_claimed_digest(attempt, TreeKind::Trace, d, body.roots[3].as_bytes()).map_err(invalid)?;
        let l = u32::try_from(l).map_err(|_| invalid("Stop input count"))?;
        let t = u32::try_from(t).map_err(|_| invalid("Stop answer count"))?;
        kaspa_consensus_core::palw_qf1::check_capacity(l, limit).map_err(invalid)?;
        if t == 0
            || t > limit
            || !opened(&y, u64::from(t - 1), &self.token.to_le_bytes(), &self.answer_path)
            || !opened(&trace, body.stop_index()?, &self.stop, &self.trace_path)
        {
            return Err(invalid("unauthenticated Stop case"));
        }
        let reason = if is_eos(self.token) {
            0
        } else if t == limit {
            1
        } else {
            return Ok(StopOutcome::ComputationRefuted);
        };
        let mut expected = [0; 18];
        expected[0] = 3;
        expected[1..5].copy_from_slice(&13u32.to_le_bytes());
        expected[5..9].copy_from_slice(&t.to_le_bytes());
        expected[9..13].copy_from_slice(&(l + t - 1).to_le_bytes());
        expected[13..17].copy_from_slice(&self.token.to_le_bytes());
        expected[17] = reason;
        Ok(if self.stop == expected { StopOutcome::ChallengeNotEstablished } else { StopOutcome::ComputationRefuted })
    }
}

pub struct LocalStopCase<'i, 't> {
    input: &'i DerivedExecInput<'t>,
    receipt: Hash64,
    marker: [u8; 64],
    outcome: StopOutcome,
}

impl<'t> DerivedExecInput<'t> {
    /// Authenticate the submitted terminal material before issuing a receipt.
    /// A committed contradiction is retainable evidence even when replay disagrees.
    pub fn verify_qf1_stop_disclosure(
        &self,
        native: &dyn ConsensusApi,
        body: &LocalOriginalBody,
        owner: &MLDSA65Signature,
        wire: &[u8],
    ) -> io::Result<(Hash64, StopOutcome)> {
        let original = self.verify_qf1_body(native, body, owner)?;
        let witness = StopWitness::decode(body, wire)?;
        Ok((original, witness.judge(self.attempt, body, self.terms.decode_tokens)?))
    }

    pub fn prepare_stop_case(
        &self,
        native: &dyn ConsensusApi,
        receipt: Hash64,
        body: &LocalOriginalBody,
        owner: &MLDSA65Signature,
        w: &StopWitness,
    ) -> io::Result<LocalStopCase<'_, 't>> {
        let original = self.read_qf1_original(native, receipt, body, owner)?;
        let outcome = w.judge(self.attempt, body, self.terms.decode_tokens)?;
        let mut h = Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/court/last-answer-stop/v1")
            .chain_update(self.chain)
            .chain_update(receipt.as_bytes())
            .chain_update(original.as_bytes())
            .chain_update(w.token.to_le_bytes())
            .chain_update(w.stop);
        for path in [&w.answer_path, &w.trace_path] {
            h.update([path.len() as u8]);
            for p in path {
                h.update(p.as_bytes());
            }
        }
        Ok(LocalStopCase { input: self, receipt, marker: h.finalize().into(), outcome })
    }
}

impl LocalStopCase<'_, '_> {
    pub fn marker(&self) -> [u8; 64] {
        self.marker
    }
    fn included(&self, native: &dyn ConsensusApi, previous: Hash64, block: Hash64, marker: [u8; 64]) -> io::Result<()> {
        let tip = native.get_sink();
        self.input.check_live(native)?;
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(previous, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(block, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.input.chain, block, &marker).map_err(io::Error::other)?
        {
            return Err(invalid("local Stop case outside committed history"));
        }
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(())
    }
    pub fn verdict_marker(&self, native: &dyn ConsensusApi, case: Hash64) -> io::Result<[u8; 64]> {
        self.included(native, self.receipt, case, self.marker)?;
        let verdict = match self.outcome {
            StopOutcome::ChallengeNotEstablished => 0,
            StopOutcome::ComputationRefuted => 1,
        };
        Ok(Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/court/last-answer-stop/verdict/v1")
            .chain_update(case.as_bytes())
            .chain_update(self.marker)
            .chain_update([verdict])
            .finalize()
            .into())
    }
    pub fn read_verdict(&self, native: &dyn ConsensusApi, case: Hash64, verdict: Hash64) -> io::Result<StopOutcome> {
        let marker = self.verdict_marker(native, case)?;
        self.included(native, case, verdict, marker)?;
        Ok(self.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_qf1::{VOCAB, ticket};

    /// Isolate the two authenticated trees of this one Court relation.
    fn committed(token: u32, reason: u8, positions: u32, recorded_token: u32) -> (LocalOriginalBody, StopWitness) {
        let s = [7; 32];
        let count = u64::from(record_count(1).unwrap()) + 4;
        let index = count - 2 - u64::from(record_count(1).unwrap());
        let mut stop = [0; 18];
        stop[0] = 3;
        stop[1..5].copy_from_slice(&13u32.to_le_bytes());
        stop[5..9].copy_from_slice(&1u32.to_le_bytes());
        stop[9..13].copy_from_slice(&positions.to_le_bytes());
        stop[13..17].copy_from_slice(&recorded_token.to_le_bytes());
        stop[17] = reason;
        let mut trace = TreeBuilder::new(s, TreeKind::Trace, count, count, Some(index)).unwrap();
        for i in 0..count {
            trace.push(if i == index { &stop } else { &[1u8][..] }).unwrap();
        }
        let (trace, trace_path) = trace.finish().unwrap();
        let x = TreeRoot::build(s, TreeKind::Input, std::iter::once(1u32.to_le_bytes().as_slice()), 1).unwrap();
        let y = TreeRoot::build(s, TreeKind::Answer, std::iter::once(token.to_le_bytes().as_slice()), 1).unwrap();
        let z = TreeRoot::build(s, TreeKind::Readout, std::iter::once([0; 64].as_slice()), 1).unwrap();
        let roots = [x, y, z, trace];
        (
            LocalOriginalBody {
                counts: roots.map(|r| r.count()),
                roots: roots.map(|r| Hash64::from_bytes(r.digest)),
                ticket: ticket(roots.each_ref()).unwrap(),
            },
            StopWitness { token, stop, answer_path: vec![], trace_path: trace_path.into_iter().map(Hash64::from_bytes).collect() },
        )
    }

    #[test]
    fn authenticated_stop_distinguishes_invalid_witness_from_committed_contradiction() {
        use StopOutcome::*;
        for (token, reason, positions, limit, want) in [
            (151643, 0, 1, 1, ChallengeNotEstablished),
            (151645, 0, 1, 2, ChallengeNotEstablished),
            (7, 1, 1, 1, ChallengeNotEstablished),
            (VOCAB - 1, 1, 1, 1, ChallengeNotEstablished),
            (151643, 1, 1, 1, ComputationRefuted),
            (7, 1, 1, 2, ComputationRefuted),
            (151643, 0, 2, 2, ComputationRefuted),
        ] {
            let (body, w) = committed(token, reason, positions, token);
            let wire = w.encode();
            let decoded = StopWitness::decode(&body, &wire).unwrap();
            assert_eq!(decoded.judge([7; 32], &body, limit).unwrap(), want);
            assert!(StopWitness::decode(&body, &wire[..wire.len() - 1]).is_err());
            let mut changed = w.clone();
            changed.stop[17] ^= 1;
            assert!(changed.judge([7; 32], &body, limit).is_err());
            let mut changed = w;
            changed.token ^= 1;
            assert!(changed.judge([7; 32], &body, limit).is_err());
        }
        let (body, mut w) = committed(VOCAB - 1, 1, 1, VOCAB - 1);
        for outside in [VOCAB, u32::MAX] {
            w.token = outside;
            assert!(w.judge([7; 32], &body, 1).is_err());
        }
        let (body, w) = committed(151643, 0, 1, 151645);
        assert_eq!(w.judge([7; 32], &body, 3).unwrap(), ComputationRefuted);
    }
}
