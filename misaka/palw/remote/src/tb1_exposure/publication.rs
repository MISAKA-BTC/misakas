//! Live, private TB1 publication reader. One selected-chain block commits to the
//! signed batch (preimages travel separately), a later one to its certificate.
//! The observer must have seen native finality before the release publication.
//! No public object tag, native credit or cold-import stability assertion.
use super::{AnchorState, Tb1HolderFreeze, anchor_state, invalid};
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    palw_tb1::{
        freeze::Tb1MiningClosure,
        qualification::Tb1Qualification,
        releases::{
            Tb1BeaconCertificate, Tb1BeaconSeed, Tb1HolderRelease, Tb1ReleaseContext, Tb1ReleaseCut, Tb1ReleaseSet, Tb1SignedRelease,
        },
        signatures::release_message,
    },
};
use libcrux_ml_dsa::ml_dsa_65::MLDSA65Signature;
use sha3::{Digest, Sha3_512};
use std::{
    collections::BTreeMap,
    io::{self, Error, ErrorKind},
};

pub struct PublishedRelease<'a> {
    pub row: Tb1HolderRelease<'a>,
    pub signature: &'a MLDSA65Signature,
}

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

pub struct NativePublicationGate {
    freeze: Tb1HolderFreeze,
    chain: [u8; 64],
    round: [u8; 64],
    stable_block: Hash64,
    closed_at: u64,
    stable_at: u64,
}

impl NativePublicationGate {
    pub fn observe(
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
        let freeze = Tb1HolderFreeze::resolve(native, close, closure, q)?;
        let (state, daa, stable_block) = anchor_state(native, close)?;
        if !matches!(state, AnchorState::Finalized) {
            return Err(Error::new(ErrorKind::WouldBlock, "TB1 close has not finalized"));
        }
        if daa >= freeze.deadline_exclusive_daa {
            return Err(invalid("TB1 publication already expired"));
        }
        let header = native.get_header(close).map_err(Error::other)?;
        let stable = native.get_header(stable_block).map_err(Error::other)?;
        Ok(Self {
            freeze,
            chain: closure.round.chain_id,
            round: closure.round.id(),
            stable_block,
            closed_at: position(header.daa_score, header.blue_score, 3)?,
            stable_at: position(daa, stable.blue_score, 3)?,
        })
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
        let at = certificate.unwrap_or(tip);
        if !matches!(state, AnchorState::Finalized)
            || batch == self.stable_block
            || !native.is_chain_ancestor_of(self.stable_block, batch).map_err(Error::other)?
            || !native.is_chain_ancestor_of(batch, at).map_err(Error::other)?
            || !native.is_chain_ancestor_of(at, tip).map_err(Error::other)?
        {
            return Err(invalid("TB1 publication outside stabilized selected history"));
        }
        let digest = batch_commitment(rows, limits)?;
        if !native.palw_coinbase_commits_to(self.chain, batch, &digest).map_err(Error::other)? {
            return Err(invalid("TB1 release batch differs from native commitment"));
        }
        let publication = native.get_header(batch).map_err(Error::other)?;
        self.freeze.check_window(publication.daa_score)?;
        let accepted_position = position(publication.daa_score, publication.blue_score, 2)?;
        let context = Tb1ReleaseContext { round_ref: &self.round, qual_root: self.freeze.qual_root, freeze_id: self.freeze.freeze_id };
        let signed: BTreeMap<_, _> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (i, Tb1SignedRelease { row: r.row, accepted_position, signature: r.signature.as_slice() }))
            .collect();
        let header = native.get_header(at).map_err(Error::other)?;
        self.freeze.check_window(header.daa_score)?;
        let cut = Tb1ReleaseCut {
            context,
            closed_at: self.closed_at,
            stable_at: self.stable_at,
            release_from: daa_start(self.freeze.release_from_daa)?
                .max(self.stable_at.checked_add(1).ok_or_else(|| invalid("TB1 gate overflow"))?),
            deadline_exclusive: daa_start(self.freeze.deadline_exclusive_daa)?,
            certificate_at: position(header.daa_score, header.blue_score, 3)?,
        };
        let set = Tb1ReleaseSet::new(&cut, q, &signed, limits.records, limits.message_bytes)
            .map_err(|_| invalid("invalid TB1 release cut"))?;
        let cert = set.assemble(limits.matrix_cells).map_err(|_| invalid("TB1 batch lacks reconstructible quorum"))?;
        let seed = set.verify_seed(&cert).map_err(|_| invalid("TB1 certificate verification failed"))?;
        let marker = certificate_commitment(batch, &cert)?;
        self.require_tip(native, tip)?;
        Ok((tip, marker, seed))
    }

    fn require_tip(&self, native: &dyn ConsensusApi, expected: Hash64) -> io::Result<()> {
        let (state, _, tip) = anchor_state(native, self.freeze.accepting_block)?;
        if !matches!(state, AnchorState::Finalized) || tip != expected {
            return Err(Error::new(ErrorKind::WouldBlock, "TB1 publication view moved"));
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
}
