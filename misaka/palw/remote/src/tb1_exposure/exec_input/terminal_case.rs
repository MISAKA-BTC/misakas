//! Common case and verdict inclusion checks for the private terminal relations.
use super::{
    original::{decode_published_qf1_original, read_published_qf1_original},
    stop_case::StopWitness,
    *,
};
use kaspa_consensus_core::{
    coinbase::{coinbase_payload_extra_data, palw_coinbase_marker},
    palw_tb1::qf1_terminal_carrier::{Qf1TerminalCarrier, Qf1TerminalRelation},
};

pub use kaspa_consensus_core::palw_qf1::terminal::TerminalOutcome;

pub struct LocalTerminalCase<'i, 't> {
    pub(super) input: &'i DerivedExecInput<'t>,
    pub(super) receipt: Hash64,
    pub(super) marker: [u8; 64],
    pub(super) verdict_domain: &'static [u8],
    pub(super) outcome: TerminalOutcome,
}

impl LocalTerminalCase<'_, '_> {
    pub fn marker(&self) -> [u8; 64] {
        self.marker
    }
    fn included(&self, native: &dyn ConsensusApi, tip: Hash64, previous: Hash64, block: Hash64, marker: [u8; 64]) -> io::Result<()> {
        let daa = native.get_header(block).map_err(io::Error::other)?.daa_score;
        if !(self.input.terms.open_daa..self.input.terms.deadline_exclusive_daa).contains(&daa) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "local terminal evidence outside original window"));
        }
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(previous, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(block, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.input.chain, block, &marker).map_err(io::Error::other)?
        {
            return Err(invalid("local terminal case outside committed history"));
        }
        Ok(())
    }
    pub fn verdict_marker(&self, native: &dyn ConsensusApi, case: Hash64) -> io::Result<[u8; 64]> {
        let tip = native.get_sink();
        self.input.check_live(native)?;
        self.included(native, tip, self.receipt, case, self.marker)?;
        self.input.check_live(native)?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(self.recorded_verdict_marker(case))
    }

    fn recorded_verdict_marker(&self, case: Hash64) -> [u8; 64] {
        let verdict = match self.outcome {
            TerminalOutcome::ChallengeNotEstablished => 0,
            TerminalOutcome::ComputationRefuted => 1,
        };
        Sha3_512::new()
            .chain_update(self.verdict_domain)
            .chain_update(case.as_bytes())
            .chain_update(self.marker)
            .chain_update([verdict])
            .finalize()
            .into()
    }
    pub fn read_verdict(&self, native: &dyn ConsensusApi, case: Hash64, verdict: Hash64) -> io::Result<TerminalOutcome> {
        let (_, tip) = self.input.check_selected(native)?;
        let outcome = self.read_verdict_on_branch(native, tip, case, verdict)?;
        if self.input.check_selected(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(outcome)
    }

    fn read_verdict_on_branch(
        &self,
        native: &dyn ConsensusApi,
        tip: Hash64,
        case: Hash64,
        verdict: Hash64,
    ) -> io::Result<TerminalOutcome> {
        self.included(native, tip, self.receipt, case, self.marker)?;
        self.included(native, tip, case, verdict, self.recorded_verdict_marker(case))?;
        Ok(self.outcome)
    }
}

impl BranchExecInput<'_> {
    /// Reconstruct a named terminal decision from the selected parent's own
    /// blocks. A failed challenge establishes only this individual relation.
    pub fn read_published_terminal_verdict(
        &self,
        native: &dyn ConsensusApi,
        expected_original: Hash64,
        case: Hash64,
        verdict: Hash64,
    ) -> io::Result<TerminalOutcome> {
        let source = native.get_block(case).map_err(io::Error::other)?;
        if source.hash() != case {
            return Err(invalid("QF1 terminal case source block hash differs"));
        }
        let carrier = Qf1TerminalCarrier::from_block(&source)
            .map_err(|why| io::Error::new(io::ErrorKind::InvalidData, format!("QF1 terminal case carrier: {why:?}")))?;
        let receipt = carrier.original_block;
        let (body, signature) = read_published_qf1_original(native, receipt)?;
        let original = self.input.read_qf1_original_on_branch(native, self.parent, receipt, &body, &signature)?;
        if original != expected_original {
            return Err(invalid("QF1 terminal case belongs to a different Original"));
        }
        let resolved = self.input.judge_terminal_carrier(original, &body, carrier)?;
        resolved.read_verdict_on_branch(native, self.parent, case, verdict)
    }
}

impl<'t> DerivedExecInput<'t> {
    fn judge_terminal_carrier(
        &self,
        original: Hash64,
        body: &LocalOriginalBody,
        carrier: Qf1TerminalCarrier<'_>,
    ) -> io::Result<LocalTerminalCase<'_, 't>> {
        let receipt = carrier.original_block;
        match carrier.relation {
            Qf1TerminalRelation::Stop => {
                let witness = StopWitness::decode(body, carrier.disclosure)?;
                self.judge_stop_case(receipt, original, body, &witness)
            }
            Qf1TerminalRelation::End => self.judge_end_case(receipt, original, body, carrier.disclosure),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedTerminalCase {
    pub block: Hash64,
    pub receipt: Hash64,
    pub relation: Qf1TerminalRelation,
    pub verdict: Option<(Hash64, TerminalOutcome)>,
}

/// Complete read of the two terminal relations in the original case window
/// on a pinned parent. Other Court relations and Gate C are separate inputs.
#[derive(Debug, PartialEq, Eq)]
pub struct BranchTerminalCourt {
    parent: Hash64,
    original: Hash64,
    window_elapsed: bool,
    cases: Vec<PublishedTerminalCase>,
}

impl BranchTerminalCourt {
    pub fn parent(&self) -> Hash64 {
        self.parent
    }

    pub fn original(&self) -> Hash64 {
        self.original
    }

    pub fn cases(&self) -> &[PublishedTerminalCase] {
        &self.cases
    }

    /// The terminal relations can prevent success. Clearing both is not
    /// permission to skip other prosecutions or independent Gate C.
    pub fn blocks_success(&self) -> bool {
        !self.window_elapsed
            || self.cases.iter().any(|case| !matches!(case.verdict, Some((_, TerminalOutcome::ChallengeNotEstablished))))
    }
}

impl BranchExecInput<'_> {
    /// Scan from the EXEC certificate to the selected parent. Starting at the
    /// Panel's chosen receipt could omit a case against an earlier publication
    /// of the same Original. An exhausted budget or missing body leaves no
    /// usable result. Walk selected parents only, even during a reorg.
    pub fn read_terminal_court_at_parent(
        &self,
        native: &dyn ConsensusApi,
        expected_original: Hash64,
        original_block: Hash64,
        max_blocks: usize,
    ) -> io::Result<BranchTerminalCourt> {
        let (body, signature) = read_published_qf1_original(native, original_block)?;
        let original = self.input.read_qf1_original_on_branch(native, self.parent, original_block, &body, &signature)?;
        if original != expected_original {
            return Err(invalid("QF1 terminal history belongs to a different Original"));
        }
        let parent_header = native.get_header(self.parent).map_err(io::Error::other)?;
        let window = self.input.terms.open_daa..self.input.terms.deadline_exclusive_daa;
        let mut cursor = self.parent;
        let mut budget = HistoryScanBudget::new("qf1_terminal", self.parent, max_blocks);
        let mut timely = Vec::new();
        while cursor != self.input.certificate {
            budget.visit(cursor)?;
            let header = native.get_header(cursor).map_err(io::Error::other)?;
            if header.hash != cursor {
                return Err(invalid("QF1 terminal history header hash differs"));
            }
            if window.contains(&header.daa_score) {
                timely.push(cursor);
            }
            cursor = native.get_ghostdag_data(cursor).map_err(io::Error::other)?.selected_parent;
        }
        let mut receipts = BTreeSet::new();
        let mut pending: BTreeMap<[u8; 64], (usize, TerminalOutcome)> = BTreeMap::new();
        let mut cases: Vec<PublishedTerminalCase> = Vec::new();
        for block in timely.into_iter().rev() {
            let source = native.get_block(block).map_err(io::Error::other)?;
            if source.hash() != block {
                return Err(invalid("QF1 terminal history body hash differs"));
            }
            let extra = source
                .transactions
                .first()
                .filter(|tx| tx.is_coinbase())
                .and_then(|tx| coinbase_payload_extra_data(&tx.payload))
                .ok_or_else(|| invalid("QF1 terminal history coinbase missing or malformed"))?;
            let Some(marker) = palw_coinbase_marker(extra) else { continue };
            if let Some((i, outcome)) = pending.remove(&marker) {
                cases[i].verdict = Some((block, outcome));
                continue;
            }
            if marker == original.as_bytes() {
                if block == original_block
                    || decode_published_qf1_original(&source).is_ok_and(|(copy, sig)| {
                        copy == body && self.input.verify_original_body(&copy, &sig).is_ok_and(|id| id == original)
                    })
                {
                    receipts.insert(block);
                }
                continue;
            }
            // Only a well-formed, attached disclosure opens a terminal case.
            // Invalid or unrelated traffic cannot place a claim on hold.
            let Ok(carrier) = Qf1TerminalCarrier::from_block(&source) else { continue };
            if !receipts.contains(&carrier.original_block) {
                continue;
            }
            let Ok(resolved) = self.input.judge_terminal_carrier(original, &body, carrier) else { continue };
            if resolved.marker != marker {
                continue;
            }
            pending.insert(resolved.recorded_verdict_marker(block), (cases.len(), resolved.outcome));
            cases.push(PublishedTerminalCase { block, receipt: carrier.original_block, relation: carrier.relation, verdict: None });
        }
        Ok(BranchTerminalCourt { parent: self.parent, original, window_elapsed: parent_header.daa_score >= window.end, cases })
    }
}
