//! RFC-0061 §9 holder custody for the private TB1 profile. Provision once with
//! fresh round material in the holder's durable directory outside chain undo.
//! Reopening never provisions. This local reservation precedes the signature
//! and transport. This isolated policy checks the native finalized head for
//! the accepting block on every reservation. The private close block commits to the
//! round, fixed consumer set, policy and qualification at the scheduled score.
use crate::monetary_material::try_exclusive_lease;
use kaspa_consensus_core::palw_tb1::{qualification::Tb1Qualification, releases::Tb1HolderRelease};
use kaspa_consensus_core::{Hash64, api::ConsensusApi, palw_native_settlement_v1::SettlementStopV1};
use libcrux_ml_dsa::ml_dsa_65::MLDSA65VerificationKey;
use sha3::{Digest, Sha3_512};
use std::{
    fs::{File, OpenOptions},
    io::{self, Error, ErrorKind, Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub mod exec_input;
pub mod mas1_control;
pub mod publication;
pub mod qf1;

const MAGIC: &[u8; 8] = b"TB1EXP01";
const HEADER: usize = 8 + 64 + 4;
const BODY: usize = 4 * 64 + 2 * 8; // accepting block, roots, signing message digest, original DAA window
const RECORD: usize = BODY + 64;

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

fn binding(round: &[u8], key: &MLDSA65VerificationKey) -> io::Result<[u8; 64]> {
    let length = u32::try_from(round.len()).map_err(|_| invalid("oversized TB1 RoundRef"))?;
    if length == 0 {
        return Err(invalid("empty TB1 RoundRef"));
    }
    Ok(Sha3_512::new()
        .chain_update(b"MISAKA/private/TB1/holder-journal/v1")
        .chain_update(length.to_be_bytes())
        .chain_update(round)
        .chain_update(key.as_slice())
        .finalize()
        .into())
}

fn location(dir: &Path, id: &[u8; 64]) -> PathBuf {
    dir.join(format!("{}.tb1-exposure", faster_hex::hex_string(id)))
}

fn header(id: &[u8; 64], slot: u32) -> [u8; HEADER] {
    let mut bytes = [0; HEADER];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..72].copy_from_slice(id);
    bytes[72..].copy_from_slice(&slot.to_be_bytes());
    bytes
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).mode(0o600);
    options
}

/// The native row owner supplies the accepting block and original TimingPlan for this FreezeId.
/// DAA is the release clock for this isolated profile.
#[derive(Clone, Copy)]
pub struct Tb1HolderFreeze {
    accepting_block: Hash64,
    qual_root: [u8; 64],
    freeze_id: [u8; 64],
    release_from_daa: u64,
    deadline_exclusive_daa: u64,
}

impl Tb1HolderFreeze {
    /// Resolve a published closure at the unique selected-chain score fixed by
    /// the round. The finality and tip-window checks run again just before signing.
    pub fn resolve(
        native: &dyn ConsensusApi,
        accepting_block: Hash64,
        closure: &kaspa_consensus_core::palw_tb1::freeze::Tb1MiningClosure,
        qualified: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
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
        Ok(Self {
            accepting_block,
            qual_root: qualified.root(),
            freeze_id,
            release_from_daa: round.release_from_daa,
            deadline_exclusive_daa: round.deadline_exclusive_daa,
        })
    }

    fn check_window(self, daa: u64) -> io::Result<()> {
        if !(self.release_from_daa..self.deadline_exclusive_daa).contains(&daa) {
            return Err(invalid("native tip outside original TB1 release window"));
        }
        Ok(())
    }
}

enum AnchorState {
    Finalized,
    Pending,
    Detached,
}

// Snapshot generation names the selected sink. Missing native data gives no
// verdict; it cannot establish either stability or a deep reorg.
fn anchor_state(native: &dyn ConsensusApi, anchor: Hash64) -> io::Result<(AnchorState, u64, Hash64)> {
    let snapshot = native
        .get_native_settlement_snapshot()
        .map_err(Error::other)?
        .ok_or_else(|| Error::new(ErrorKind::WouldBlock, "native settlement unavailable"))?;
    let sink = snapshot.generation;
    if native.get_sink() != sink {
        return Err(Error::new(ErrorKind::WouldBlock, "native settlement generation moved"));
    }
    let state = if !native.is_chain_ancestor_of(anchor, sink).map_err(Error::other)? {
        AnchorState::Detached
    } else if let Some(head) = snapshot.finalized {
        if native.is_chain_ancestor_of(anchor, head).map_err(Error::other)? { AnchorState::Finalized } else { AnchorState::Pending }
    } else if snapshot.stop == Some(SettlementStopV1::FinalizedConflict) {
        AnchorState::Pending
    } else {
        // A heavier branch may keep the freeze block but withdraw the work
        // that finalized it. Distinguish a named withdrawal from missing data.
        let prior = native
            .get_native_safe_readiness()
            .map_err(Error::other)?
            .filter(|r| r.generation == sink && r.stop == snapshot.stop)
            .and_then(|r| r.finalized.withdrawn_from)
            .ok_or_else(|| Error::new(ErrorKind::WouldBlock, "native finalized head unavailable"))?;
        if !native.is_chain_ancestor_of(anchor, prior).map_err(Error::other)? {
            return Err(Error::new(ErrorKind::WouldBlock, "native withdrawal does not identify this freeze"));
        }
        AnchorState::Pending
    };
    let sink_daa = native.get_header(sink).map_err(Error::other)?.daa_score;
    if native.get_native_settlement_snapshot().map_err(Error::other)?.as_ref() != Some(&snapshot) || native.get_sink() != sink {
        return Err(Error::new(ErrorKind::WouldBlock, "native settlement changed during custody read"));
    }
    Ok((state, sink_daa, sink))
}

/// Per holder key and RoundRef. Slot, qualification and freeze cannot select
/// a different journal file. Only a digest of the private shares is retained.
pub struct Tb1ExposureJournal {
    file: File,
    directory: File,
    id: [u8; 64],
    key: MLDSA65VerificationKey,
    slot: u32,
}

impl Tb1ExposureJournal {
    /// Called at fresh enrollment, never as recovery from a missing journal.
    /// The custody directory must already exist and survive chain restoration.
    pub fn provision(dir: &Path, round: &[u8], key: &MLDSA65VerificationKey, slot: u32) -> io::Result<()> {
        let id = binding(round, key)?;
        let directory = File::open(dir)?;
        let mut file = options().create_new(true).open(location(dir, &id))?;
        file.write_all(&header(&id, slot))?;
        file.sync_all()?;
        directory.sync_all()
    }

    pub fn resume(dir: &Path, round: &[u8], key: &MLDSA65VerificationKey, slot: u32) -> io::Result<Self> {
        let id = binding(round, key)?;
        let file = options().open(location(dir, &id))?;
        if !file.metadata()?.is_file() {
            return Err(invalid("TB1 custody is not a regular file"));
        }
        try_exclusive_lease(&file)?;
        let mut journal = Self { file, directory: File::open(dir)?, id, key: key.clone(), slot };
        journal.read_record()?;
        Ok(journal)
    }

    fn read_record(&mut self) -> io::Result<Option<[u8; BODY]>> {
        self.file.rewind()?;
        let mut stored_header = [0; HEADER];
        self.file.read_exact(&mut stored_header)?;
        if stored_header != header(&self.id, self.slot) {
            return Err(invalid("TB1 enrollment differs from custody"));
        }
        let size = self.file.metadata()?.len();
        if size == HEADER as u64 {
            return Ok(None);
        }
        if size != (HEADER + RECORD) as u64 && size != (HEADER + RECORD + 64) as u64 {
            return Err(invalid("incomplete or oversized TB1 exposure record"));
        }
        let mut body = [0; BODY];
        let mut checksum = [0; 64];
        self.file.read_exact(&mut body)?;
        self.file.read_exact(&mut checksum)?;
        if Self::checksum(&stored_header, &body) != checksum {
            return Err(invalid("TB1 exposure record checksum mismatch"));
        }
        if size > (HEADER + RECORD) as u64 {
            let mut abort = [0; 64];
            self.file.read_exact(&mut abort)?;
            if abort != Self::abort_marker(&checksum) {
                return Err(invalid("invalid TB1 exposure abort record"));
            }
            // The writer may have failed after the append and before a sync.
            self.file.sync_all()?;
            self.directory.sync_all()?;
            return Err(invalid("TB1 exposed round halted after native reorg"));
        }
        Ok(Some(body))
    }

    fn checksum(header: &[u8; HEADER], body: &[u8; BODY]) -> [u8; 64] {
        Sha3_512::new().chain_update(header).chain_update(body).finalize().into()
    }

    fn abort_marker(checksum: &[u8; 64]) -> [u8; 64] {
        Sha3_512::new().chain_update(b"MISAKA/private/TB1/exposed-aborted/v1").chain_update(checksum).finalize().into()
    }

    /// Observe the recorded accepting block even if the replacement branch
    /// has no release to send. Once detached, an exposed round stays halted
    /// across an A-B-A reorg and process restart.
    pub fn observe_branch(&mut self, native: &dyn ConsensusApi) -> io::Result<()> {
        if let Some(body) = self.read_record()? {
            self.check_exposed(native, &body)?;
        }
        Ok(())
    }

    fn check_exposed(&mut self, native: &dyn ConsensusApi, body: &[u8; BODY]) -> io::Result<u64> {
        let anchor = Hash64::from_bytes(body[..64].try_into().expect("fixed anchor width"));
        let (state, daa, _) = anchor_state(native, anchor)?;
        match state {
            AnchorState::Finalized => Ok(daa),
            AnchorState::Pending | AnchorState::Detached => {
                let checksum = Self::checksum(&header(&self.id, self.slot), body);
                self.file.seek(SeekFrom::End(0))?;
                self.file.write_all(&Self::abort_marker(&checksum))?;
                self.file.sync_all()?;
                self.directory.sync_all()?;
                Err(invalid("TB1 exposed round halted after native reorg"))
            }
        }
    }

    /// Return the exact private-profile signing preimage only after the file
    /// and its directory entry are durable. A failed sync is retried even
    /// for an identical reservation. Never overwrite a torn or different row.
    pub fn reserve_signing_message(
        &mut self,
        native: &dyn ConsensusApi,
        freeze: Tb1HolderFreeze,
        qualified: &Tb1Qualification<'_>,
        row: &Tb1HolderRelease<'_>,
        byte_limit: usize,
    ) -> io::Result<Vec<u8>> {
        let previous = self.read_record()?;
        if let Some(saved) = &previous {
            let daa = self.check_exposed(native, saved)?;
            freeze.check_window(daa)?;
        }
        let message = qualified
            .holder_release_message(row, &self.key, byte_limit)
            .map_err(|_| invalid("TB1 release differs from qualification, signing key or size limit"))?;
        if row.context.qual_root != freeze.qual_root
            || row.context.freeze_id != freeze.freeze_id
            || row.holder_slot != self.slot as usize
            || binding(row.context.round_ref, &self.key)? != self.id
        {
            return Err(invalid("TB1 release differs from holder enrollment"));
        }
        let mut body = [0; BODY];
        body[..64].copy_from_slice(freeze.accepting_block.as_byte_slice());
        body[64..128].copy_from_slice(&row.context.qual_root);
        body[128..192].copy_from_slice(&row.context.freeze_id);
        body[192..256].copy_from_slice(&Sha3_512::digest(&message));
        body[256..264].copy_from_slice(&freeze.release_from_daa.to_be_bytes());
        body[264..].copy_from_slice(&freeze.deadline_exclusive_daa.to_be_bytes());
        match previous {
            Some(previous) if previous != body => return Err(invalid("TB1 round already reserved for a different exposure")),
            Some(_) => {}
            None => {
                let (state, daa, _) = anchor_state(native, freeze.accepting_block)?;
                freeze.check_window(daa)?;
                if !matches!(state, AnchorState::Finalized) {
                    return Err(Error::new(ErrorKind::WouldBlock, "TB1 freeze has not reached the native finalized head"));
                }
                let checksum = Self::checksum(&header(&self.id, self.slot), &body);
                self.file.seek(SeekFrom::End(0))?;
                self.file.write_all(&body)?;
                self.file.write_all(&checksum)?;
            }
        }
        self.file.sync_all()?;
        self.directory.sync_all()?;
        freeze.check_window(self.check_exposed(native, &body)?)?;
        Ok(message)
    }
}

#[cfg(test)]
mod tests;
