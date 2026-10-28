//! The values this worker already captured once per iteration. This does not
//! snapshot the database or replace the existing unguarded consensus session.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct PanelTickReadV1<'a> {
    session: &'a kaspa_consensusmanager::ConsensusProxy,
    bond: PalwBondKeyV2,
    domain: Hash64,
    daa: u64,
}
impl<'a> PanelTickReadV1<'a> {
    pub(super) fn new(session: &'a kaspa_consensusmanager::ConsensusProxy, bond: PalwBondKeyV2, domain: Hash64, daa: u64) -> Self {
        Self { session, bond, domain, daa }
    }
    pub(super) fn session(&self) -> &'a kaspa_consensusmanager::ConsensusProxy {
        self.session
    }
    pub(super) fn bond(&self) -> PalwBondKeyV2 {
        self.bond
    }
    pub(super) fn domain(&self) -> Hash64 {
        self.domain
    }
    pub(super) fn daa(&self) -> u64 {
        self.daa
    }
}
