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
    fn included(&self, native: &dyn ConsensusApi, previous: Hash64, block: Hash64, marker: [u8; 64]) -> io::Result<()> {
        let tip = native.get_sink();
        self.input.check_live(native)?;
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
        self.included(native, self.receipt, case, self.marker)?;
        let verdict = match self.outcome {
            TerminalOutcome::ChallengeNotEstablished => 0,
            TerminalOutcome::ComputationRefuted => 1,
        };
        Ok(Sha3_512::new()
            .chain_update(self.verdict_domain)
            .chain_update(case.as_bytes())
            .chain_update(self.marker)
            .chain_update([verdict])
            .finalize()
            .into())
    }
    pub fn read_verdict(&self, native: &dyn ConsensusApi, case: Hash64, verdict: Hash64) -> io::Result<TerminalOutcome> {
        let marker = self.verdict_marker(native, case)?;
        self.included(native, case, verdict, marker)?;
        Ok(self.outcome)
    }
}
