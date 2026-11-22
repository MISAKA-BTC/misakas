//! Common case and verdict inclusion checks for the private terminal relations.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutcome {
    ChallengeNotEstablished,
    ComputationRefuted,
}

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
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
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
        self.included(native, tip, self.receipt, case, self.marker)?;
        self.included(native, tip, case, verdict, self.recorded_verdict_marker(case))?;
        if self.input.check_selected(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(self.outcome)
    }
}
