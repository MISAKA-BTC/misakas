//! Live holder publication and the shared branch-scoped release verifier.
use super::{AnchorState, Tb1CustodyError, Tb1HolderFreeze, anchor_state, invalid};
pub(super) use kaspa_consensus_core::palw_tb1::publication::with_published_batch;
pub use kaspa_consensus_core::palw_tb1::publication::{PublicationLimits, PublishedRelease, batch_commitment, release_batch_payload};
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    block::Block,
    palw_native_settlement_v1::NativeSettlementSnapshotV1,
    palw_tb1::{
        freeze::Tb1MiningClosure, publication::BranchPublicationGate, qualification::Tb1Qualification, releases::Tb1BeaconSeed,
    },
};
use std::io::{self, Error};

pub struct NativePublicationGate {
    freeze: Tb1HolderFreeze,
    chain: [u8; 64],
    branch: BranchPublicationGate,
}

impl NativePublicationGate {
    #[allow(clippy::too_many_arguments)]
    pub fn verify_published_candidate(
        &self,
        native: &dyn ConsensusApi,
        parent: Hash64,
        settlement: &NativeSettlementSnapshotV1,
        batch: Hash64,
        candidate: &Block,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<kaspa_consensus_core::palw_tb1::publication::VerifiedTb1Publication> {
        self.branch.verify_published_candidate(native, parent, settlement, batch, candidate, q, limits)
    }

    pub fn holder_freeze(&self) -> Tb1HolderFreeze {
        self.freeze
    }

    pub fn observe(
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
        let freeze = Tb1HolderFreeze::resolve(native, close, closure, q)?;
        freeze.check_tip_deadline(native)?;
        let (state, daa, stable_block) = anchor_state(native, close)?;
        freeze.check_deadline(daa)?;
        if !matches!(state, AnchorState::Finalized) {
            return Err(Tb1CustodyError::NotFinalized.into());
        }
        let snapshot = native
            .get_native_settlement_snapshot()
            .map_err(Error::other)?
            .ok_or(Tb1CustodyError::NativeUnavailable("native settlement unavailable"))?;
        if snapshot.generation != stable_block {
            return Err(Tb1CustodyError::ViewChanged { tip: stable_block }.into());
        }
        let branch = BranchPublicationGate::observe(native, close, closure, q, &snapshot)?;
        let gate = Self { freeze, chain: closure.round.chain_id, branch };
        gate.require_tip(native, stable_block)?;
        Ok(gate)
    }

    /// Observe the original, branch-owned pre-publication settlement.
    pub fn observe_at_snapshot(
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
        settlement: &NativeSettlementSnapshotV1,
    ) -> io::Result<Self> {
        let branch = BranchPublicationGate::observe(native, close, closure, q, settlement)?;
        let freeze = Tb1HolderFreeze::resolve_in_branch(native, close, closure, q)?;
        Ok(Self { freeze, chain: closure.round.chain_id, branch })
    }

    fn assemble(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Option<Hash64>,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<(Hash64, [u8; 64], Tb1BeaconSeed)> {
        let (state, _, tip) = anchor_state(native, self.freeze.accepting_block)?;
        if !matches!(state, AnchorState::Finalized) {
            return Err(invalid("TB1 publication outside stabilized selected history"));
        }
        let (marker, seed) = self.branch.assemble_on_branch(native, tip, batch, certificate.unwrap_or(tip), rows, q, limits)?;
        self.require_tip(native, tip)?;
        Ok((tip, marker, seed))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn require_tip(&self, native: &dyn ConsensusApi, expected: Hash64) -> io::Result<()> {
        let (state, _, tip) = anchor_state(native, self.freeze.accepting_block)?;
        if tip != expected {
            return Err(Tb1CustodyError::ViewChanged { tip: expected }.into());
        }
        if !matches!(state, AnchorState::Finalized) {
            return Err(Tb1CustodyError::NotFinalized.into());
        }
        Ok(())
    }

    pub fn propose(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<[u8; 64]> {
        Ok(self.assemble(native, batch, None, rows, q, limits)?.1)
    }

    /// Assemble the certificate commitment from the batch block's own shares.
    pub fn propose_published(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<[u8; 64]> {
        with_published_batch(native, batch, limits, |rows| self.propose(native, batch, rows, q, limits))
    }

    pub fn read_seed(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<Tb1BeaconSeed> {
        let (tip, marker, seed) = self.assemble(native, batch, Some(certificate), rows, q, limits)?;
        if certificate == batch || !native.palw_coinbase_commits_to(self.chain, certificate, &marker).map_err(Error::other)? {
            return Err(invalid("TB1 certificate differs from selected block"));
        }
        self.require_tip(native, tip)?;
        Ok(seed)
    }

    pub fn read_published_seed(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<Tb1BeaconSeed> {
        with_published_batch(native, batch, limits, |rows| self.read_seed(native, batch, certificate, rows, q, limits))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn read_seed_at_parent(
        &self,
        native: &dyn ConsensusApi,
        parent: Hash64,
        settlement: &NativeSettlementSnapshotV1,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<Tb1BeaconSeed> {
        self.branch.read_seed_at_parent(native, parent, settlement, batch, certificate, rows, q, limits)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn read_published_seed_at_parent(
        &self,
        native: &dyn ConsensusApi,
        parent: Hash64,
        settlement: &NativeSettlementSnapshotV1,
        batch: Hash64,
        certificate: Hash64,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<Tb1BeaconSeed> {
        self.branch.read_published_seed_at_parent(native, parent, settlement, batch, certificate, q, limits)
    }
}
