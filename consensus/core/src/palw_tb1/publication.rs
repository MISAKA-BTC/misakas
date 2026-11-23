//! TB1 release verification from branch-pinned native history.
pub use crate::palw_tb1::release_batch_carrier::PublishedRelease;
use crate::{
    Hash64,
    api::ConsensusApi,
    block::Block,
    coinbase::{coinbase_payload_extra_data, palw_coinbase_marker},
    header::Header,
    palw_native_settlement_v1::NativeSettlementSnapshotV1,
    palw_tb1::{
        freeze::Tb1MiningClosure,
        qualification::Tb1Qualification,
        release_batch_carrier::{encode_release_batch, release_batch_from_block},
        releases::{Tb1BeaconCertificate, Tb1BeaconSeed, Tb1ReleaseContext, Tb1ReleaseCut, Tb1ReleaseSet, Tb1SignedRelease},
        signatures::release_message,
    },
};
use sha3::{Digest, Sha3_512};
use std::{
    collections::BTreeMap,
    io::{self, Error, ErrorKind},
};

#[derive(Clone, Copy)]
pub struct PublicationLimits {
    pub records: usize,
    pub message_bytes: usize,
    pub matrix_cells: usize,
}

fn count(n: usize) -> io::Result<[u8; 4]> {
    u32::try_from(n).map(u32::to_be_bytes).map_err(|_| invalid("TB1 count exceeds private encoding"))
}

pub fn batch_commitment(rows: &[PublishedRelease<'_>], limits: PublicationLimits) -> io::Result<[u8; 64]> {
    if rows.is_empty() || rows.len() > limits.records {
        return Err(invalid("TB1 release batch size"));
    }
    let mut h = Sha3_512::new().chain_update(b"MISAKA/private/TB1/signed-batch/v1").chain_update(count(rows.len())?);
    for signed in rows {
        let msg = release_message(&signed.row, limits.message_bytes).ok_or_else(|| invalid("TB1 release message exceeds bounds"))?;
        h.update(count(msg.len())?);
        h.update(msg);
        h.update(signed.signature.as_slice());
    }
    Ok(h.finalize().into())
}

pub fn release_batch_payload(rows: &[PublishedRelease<'_>], limits: PublicationLimits) -> io::Result<Vec<u8>> {
    encode_release_batch(rows, limits.records, limits.message_bytes)
        .map_err(|error| Error::new(ErrorKind::InvalidData, format!("TB1 release batch carrier: {error:?}")))
}

pub fn with_published_batch<T>(
    native: &dyn ConsensusApi,
    batch: Hash64,
    limits: PublicationLimits,
    read: impl FnOnce(&[PublishedRelease<'_>]) -> io::Result<T>,
) -> io::Result<T> {
    let block = native.get_block(batch).map_err(Error::other)?;
    if block.hash() != batch {
        return Err(invalid("TB1 release batch block hash differs"));
    }
    let records = release_batch_from_block(&block, limits.records, limits.message_bytes)
        .map_err(|error| Error::new(ErrorKind::InvalidData, format!("TB1 release batch carrier: {error:?}")))?;
    let rows: Vec<_> = records.iter().map(|record| record.view()).collect();
    read(&rows)
}

fn certificate_commitment(batch: Hash64, cert: &Tb1BeaconCertificate<'_, usize>) -> io::Result<[u8; 64]> {
    let mut h = Sha3_512::new()
        .chain_update(b"MISAKA/private/TB1/batch-certificate/v1")
        .chain_update(batch.as_bytes())
        .chain_update(count(cert.context.round_ref.len())?)
        .chain_update(cert.context.round_ref)
        .chain_update(cert.context.qual_root)
        .chain_update(cert.context.freeze_id)
        .chain_update(count(cert.dealers.len())?);
    for dealer in &cert.dealers {
        h.update(count(dealer.dealer_slot)?);
        h.update(count(dealer.coefficients.len())?);
        for c in &dealer.coefficients {
            h.update(c.to_bytes());
        }
        h.update(count(dealer.supporting_releases.len())?);
        for &ordinal in &dealer.supporting_releases {
            h.update(count(ordinal)?);
        }
    }
    Ok(h.finalize().into())
}

// Private order key: DAA first, selected-chain blue score second, intra-block
// phase last. This preserves ordering when many accepted blocks share one DAA.
// Limit checks prevent truncating a schedule or mapping distinct positions together.
fn daa_start(daa: u64) -> io::Result<u64> {
    Ok(u64::from(u32::try_from(daa).map_err(|_| invalid("TB1 private DAA position overflow"))?) << 32)
}

fn position(daa: u64, score: u64, phase: u32) -> io::Result<u64> {
    let low = u32::try_from(score).ok().and_then(|v| v.checked_mul(4)).ok_or_else(|| invalid("TB1 private blue position overflow"))?;
    Ok(daa_start(daa)? | u64::from(low + phase))
}

pub struct BranchPublicationGate {
    close: Hash64,
    chain: [u8; 64],
    round: [u8; 64],
    qual_root: [u8; 64],
    freeze_id: [u8; 64],
    release_from_daa: u64,
    deadline_exclusive_daa: u64,
    stable_block: Hash64,
    closed_at: u64,
    stable_at: u64,
}

impl BranchPublicationGate {
    /// The owner supplies its validated settlement snapshot at the original
    /// pre-publication observation. A later snapshot cannot move this cut.
    pub fn observe(
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        qualified: &Tb1Qualification<'_>,
        settlement: &NativeSettlementSnapshotV1,
    ) -> io::Result<Self> {
        let stable_block = settlement.generation;
        Self::check_finalized_branch(native, close, stable_block, settlement)?;
        let freeze_id = resolve_published_freeze(native, close, closure, qualified)?;
        let header = native.get_header(close).map_err(Error::other)?;
        let stable = native.get_header(stable_block).map_err(Error::other)?;
        let round = &closure.round;
        if stable.daa_score >= round.deadline_exclusive_daa {
            return Err(Error::new(ErrorKind::TimedOut, "TB1 original release deadline elapsed"));
        }
        Ok(Self {
            close,
            chain: round.chain_id,
            round: round.id(),
            qual_root: qualified.root(),
            freeze_id,
            release_from_daa: round.release_from_daa,
            deadline_exclusive_daa: round.deadline_exclusive_daa,
            stable_block,
            closed_at: position(header.daa_score, header.blue_score, 3)?,
            stable_at: position(stable.daa_score, stable.blue_score, 3)?,
        })
    }

    fn check_window(&self, daa: u64) -> io::Result<()> {
        if daa >= self.deadline_exclusive_daa {
            return Err(Error::new(ErrorKind::TimedOut, "TB1 original release deadline elapsed"));
        }
        if daa < self.release_from_daa {
            return Err(invalid("native tip outside original TB1 release window"));
        }
        Ok(())
    }

    fn check_finalized_branch(
        native: &dyn ConsensusApi,
        close: Hash64,
        parent: Hash64,
        settlement: &NativeSettlementSnapshotV1,
    ) -> io::Result<()> {
        let finalized = settlement.finalized.ok_or_else(|| invalid("TB1 candidate parent has no finalized head"))?;
        if settlement.version != 1
            || settlement.generation != parent
            || !native.is_chain_ancestor_of(finalized, parent).map_err(Error::other)?
            || !native.is_chain_ancestor_of(close, finalized).map_err(Error::other)?
        {
            return Err(invalid("TB1 freeze outside the candidate parent's finalized history"));
        }
        Ok(())
    }

    pub fn assemble_on_branch(
        &self,
        native: &dyn ConsensusApi,
        parent: Hash64,
        batch: Hash64,
        at: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<([u8; 64], Tb1BeaconSeed)> {
        if !native.is_chain_ancestor_of(batch, at).map_err(Error::other)?
            || !native.is_chain_ancestor_of(at, parent).map_err(Error::other)?
        {
            return Err(invalid("TB1 publication outside stabilized selected history"));
        }
        let header = native.get_header(at).map_err(Error::other)?;
        self.assemble_for_header(native, batch, &header, rows, q, limits)
    }

    fn assemble_for_header(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        header: &Header,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<([u8; 64], Tb1BeaconSeed)> {
        if batch == self.stable_block || !native.is_chain_ancestor_of(self.stable_block, batch).map_err(Error::other)? {
            return Err(invalid("TB1 publication outside stabilized selected history"));
        }
        let digest = batch_commitment(rows, limits)?;
        if !native.palw_coinbase_commits_to(self.chain, batch, &digest).map_err(Error::other)? {
            return Err(invalid("TB1 release batch differs from native commitment"));
        }
        let publication = native.get_header(batch).map_err(Error::other)?;
        self.check_window(publication.daa_score)?;
        let accepted_position = position(publication.daa_score, publication.blue_score, 2)?;
        let context = Tb1ReleaseContext { round_ref: &self.round, qual_root: self.qual_root, freeze_id: self.freeze_id };
        let signed: BTreeMap<_, _> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (i, Tb1SignedRelease { row: r.row, accepted_position, signature: r.signature.as_slice() }))
            .collect();
        self.check_window(header.daa_score)?;
        let cut = Tb1ReleaseCut {
            context,
            closed_at: self.closed_at,
            stable_at: self.stable_at,
            release_from: daa_start(self.release_from_daa)?
                .max(self.stable_at.checked_add(1).ok_or_else(|| invalid("TB1 gate overflow"))?),
            deadline_exclusive: daa_start(self.deadline_exclusive_daa)?,
            certificate_at: position(header.daa_score, header.blue_score, 3)?,
        };
        let set = Tb1ReleaseSet::new(&cut, q, &signed, limits.records, limits.message_bytes)
            .map_err(|_| invalid("invalid TB1 release cut"))?;
        let cert = set.assemble(limits.matrix_cells).map_err(|_| invalid("TB1 batch lacks reconstructible quorum"))?;
        let seed = set.verify_seed(&cert).map_err(|_| invalid("TB1 certificate verification failed"))?;
        let marker = certificate_commitment(batch, &cert)?;
        Ok((marker, seed))
    }

    /// Resolve an in-flight certificate with its own header and coinbase.
    /// The native owner pins the candidate parent and its settlement snapshot.
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
    ) -> io::Result<Tb1BeaconSeed> {
        Self::check_finalized_branch(native, self.close, parent, settlement)?;
        if batch == candidate.hash() || !native.is_chain_ancestor_of(batch, parent).map_err(Error::other)? {
            return Err(invalid("TB1 release batch does not precede the candidate"));
        }
        let marker = candidate
            .transactions
            .first()
            .filter(|tx| tx.is_coinbase())
            .and_then(|tx| coinbase_payload_extra_data(&tx.payload))
            .and_then(palw_coinbase_marker)
            .ok_or_else(|| invalid("TB1 certificate candidate has no commitment"))?;
        with_published_batch(native, batch, limits, |rows| {
            let (expected, seed) = self.assemble_for_header(native, batch, &candidate.header, rows, q, limits)?;
            if marker != expected {
                return Err(invalid("TB1 certificate differs from candidate block"));
            }
            Ok(seed)
        })
    }

    /// Check a recorded release against the candidate's selected parent.
    /// The native owner supplies its validated settlement snapshot for that
    /// parent. The gate still binds the original pre-publication observation.
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
        Self::check_finalized_branch(native, self.close, parent, settlement)?;
        let (marker, seed) = self.assemble_on_branch(native, parent, batch, certificate, rows, q, limits)?;
        if certificate == batch || !native.palw_coinbase_commits_to(self.chain, certificate, &marker).map_err(Error::other)? {
            return Err(invalid("TB1 certificate differs from selected block"));
        }
        Ok(seed)
    }

    /// Read the candidate branch's published rows without using the live tip.
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
        with_published_batch(native, batch, limits, |rows| {
            self.read_seed_at_parent(native, parent, settlement, batch, certificate, rows, q, limits)
        })
    }
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

/// Bind the authenticated qualification to its actual close block.
pub fn resolve_published_freeze(
    native: &dyn ConsensusApi,
    accepting_block: Hash64,
    closure: &Tb1MiningClosure,
    qualified: &Tb1Qualification<'_>,
) -> io::Result<[u8; 64]> {
    let round = &closure.round;
    if qualified.round_ref() != round.id()
        || qualified.root() != closure.qual_root
        || qualified.roster().root().ok() != Some(round.roster_root)
    {
        return Err(invalid("TB1 closure differs from the prepared round"));
    }
    let header = native.get_header(accepting_block).map_err(Error::other)?;
    if header.blue_score != round.close_blue_score
        || header.daa_score >= round.release_from_daa
        || round.release_from_daa >= round.deadline_exclusive_daa
    {
        return Err(invalid("TB1 close block outside its original schedule"));
    }
    let parent = native.get_ghostdag_data(accepting_block).map_err(Error::other)?.selected_parent;
    let freeze_id = closure.commitment(parent);
    if !native.palw_coinbase_commits_to(round.chain_id, accepting_block, &freeze_id).map_err(Error::other)? {
        return Err(invalid("TB1 close block does not commit to the supplied closure"));
    }
    Ok(freeze_id)
}
