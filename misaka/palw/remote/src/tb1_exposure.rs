//! RFC-0061 §9 holder custody for the private TB1 profile. Provision once with
//! fresh round material in the holder's durable directory outside chain undo.
//! Reopening never provisions. This local reservation precedes the signature
//! and transport. This isolated policy checks the native finalized head for
//! the accepting block on every reservation. The private close block commits to the
//! round, fixed consumer set, policy and qualification at the scheduled score.
use crate::monetary_material::try_exclusive_lease;
use kaspa_consensus_core::palw_tb1::{Tb1PolynomialError, qualification::Tb1Qualification, releases::Tb1HolderRelease};
use kaspa_consensus_core::{
    Hash64, api::ConsensusApi, errors::consensus::ConsensusError, palw_native_settlement_v1::SettlementStopV1,
};
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
pub mod model_audit;
pub mod publication;
pub mod qf1;

const MAGIC: &[u8; 8] = b"TB1EXP01";
const HEADER: usize = 8 + 64 + 4;
const BODY: usize = 4 * 64 + 2 * 8; // accepting block, roots, signing message digest, original DAA window
const RECORD: usize = BODY + 64;

/// Failures encountered while opening or reserving durable TB1 holder custody.
#[derive(Debug, thiserror::Error)]
pub enum Tb1CustodyError {
    #[error("TB1 exposed round halted after native reorg")]
    Halted,
    #[error("TB1 custody is corrupt: {0}")]
    Corrupt(&'static str),
    #[error("TB1 enrollment differs from custody")]
    EnrollmentMismatch,
    #[error("TB1 exposure record checksum mismatch")]
    ChecksumMismatch,
    #[error("TB1 custody is leased by another holder process")]
    JournalInUse,
    #[error("TB1 RoundRef is empty or oversized")]
    InvalidRound,
    #[error("TB1 freeze has not reached the native finalized head")]
    NotFinalized,
    #[error("TB1 close is outside selected history")]
    DifferentBranch,
    #[error("native tip precedes the original TB1 release window")]
    BeforeWindow,
    #[error("TB1 original release deadline elapsed")]
    DeadlineElapsed,
    #[error("TB1 round already reserved for a different exposure")]
    Conflict,
    #[error("TB1 release rejected: {0:?}")]
    InvalidRelease(Tb1PolynomialError),
    #[error("TB1 release differs from holder enrollment")]
    ReleaseEnrollmentMismatch,
    #[error("{0}")]
    NativeUnavailable(&'static str),
    #[error("native view changed during TB1 read ({tip})")]
    ViewChanged { tip: Hash64 },
    #[error("native custody read failed: {0}")]
    NativeRead(#[source] ConsensusError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tb1CustodyAction {
    WaitForFinality,
    WaitForWindow,
    WaitForLease,
    ReadNewView,
    RestoreNativeData,
    EndRound,
    RejectInput,
    KeepHalted,
    InspectStorage,
}

impl Tb1CustodyError {
    pub fn action(&self) -> Tb1CustodyAction {
        use Tb1CustodyAction::*;
        match self {
            Self::NotFinalized => WaitForFinality,
            Self::BeforeWindow => WaitForWindow,
            Self::JournalInUse => WaitForLease,
            Self::ViewChanged { .. } => ReadNewView,
            Self::NativeUnavailable(_) | Self::NativeRead(_) => RestoreNativeData,
            Self::DeadlineElapsed => EndRound,
            Self::Halted => KeepHalted,
            Self::Corrupt(_) | Self::ChecksumMismatch | Self::Io(_) => InspectStorage,
            Self::DifferentBranch
            | Self::EnrollmentMismatch
            | Self::InvalidRound
            | Self::Conflict
            | Self::InvalidRelease(_)
            | Self::ReleaseEnrollmentMismatch => RejectInput,
        }
    }
}

// The adjoining publication API uses io::Result. Preserve its existing error kinds.
impl From<Tb1CustodyError> for io::Error {
    fn from(error: Tb1CustodyError) -> Self {
        let kind = match error {
            Tb1CustodyError::Io(source) => return source,
            Tb1CustodyError::NativeUnavailable(_) | Tb1CustodyError::NotFinalized | Tb1CustodyError::ViewChanged { .. } => {
                ErrorKind::WouldBlock
            }
            Tb1CustodyError::JournalInUse => ErrorKind::WouldBlock,
            Tb1CustodyError::DeadlineElapsed => ErrorKind::TimedOut,
            Tb1CustodyError::NativeRead(_) => ErrorKind::Other,
            _ => ErrorKind::InvalidData,
        };
        Self::new(kind, error)
    }
}

fn invalid(message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}

fn binding(round: &[u8], key: &MLDSA65VerificationKey) -> Result<[u8; 64], Tb1CustodyError> {
    let length = u32::try_from(round.len()).map_err(|_| Tb1CustodyError::InvalidRound)?;
    if length == 0 {
        return Err(Tb1CustodyError::InvalidRound);
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
        let tip = native.get_sink();
        let freeze = Self::resolve_in_branch(native, accepting_block, closure, qualified)?;
        let selected = native.is_chain_ancestor_of(accepting_block, tip).map_err(Error::other)?;
        if native.get_sink() != tip {
            return Err(Tb1CustodyError::ViewChanged { tip }.into());
        }
        if !selected {
            return Err(Tb1CustodyError::DifferentBranch.into());
        }
        Ok(freeze)
    }

    // Verify the closure in a block whose DAG context is already available.
    // Only the live holder API may use the selected tip as publication authority.
    fn resolve_in_branch(
        native: &dyn ConsensusApi,
        accepting_block: Hash64,
        closure: &kaspa_consensus_core::palw_tb1::freeze::Tb1MiningClosure,
        qualified: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
        let round = &closure.round;
        let freeze_id =
            kaspa_consensus_core::palw_tb1::publication::resolve_published_freeze(native, accepting_block, closure, qualified)?;
        Ok(Self {
            accepting_block,
            qual_root: qualified.root(),
            freeze_id,
            release_from_daa: round.release_from_daa,
            deadline_exclusive_daa: round.deadline_exclusive_daa,
        })
    }

    fn check_deadline(self, daa: u64) -> Result<(), Tb1CustodyError> {
        if daa >= self.deadline_exclusive_daa {
            return Err(Tb1CustodyError::DeadlineElapsed);
        }
        Ok(())
    }

    // Finality may still be unavailable at expiry. A coherent selected tip
    // supplies the deadline clock without granting permission to release.
    fn check_tip_deadline(self, native: &dyn ConsensusApi) -> Result<(), Tb1CustodyError> {
        let tip = native.get_sink();
        let daa = native.get_header(tip).map_err(Tb1CustodyError::NativeRead)?.daa_score;
        if native.get_sink() != tip {
            return Err(Tb1CustodyError::ViewChanged { tip });
        }
        self.check_deadline(daa)
    }

    fn check_window(self, daa: u64) -> Result<(), Tb1CustodyError> {
        self.check_deadline(daa)?;
        if daa < self.release_from_daa {
            return Err(Tb1CustodyError::BeforeWindow);
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
fn anchor_state(native: &dyn ConsensusApi, anchor: Hash64) -> Result<(AnchorState, u64, Hash64), Tb1CustodyError> {
    let snapshot = native
        .get_native_settlement_snapshot()
        .map_err(Tb1CustodyError::NativeRead)?
        .ok_or_else(|| Tb1CustodyError::NativeUnavailable("native settlement unavailable"))?;
    let sink = snapshot.generation;
    if native.get_sink() != sink {
        return Err(Tb1CustodyError::ViewChanged { tip: sink });
    }
    let state = if !native.is_chain_ancestor_of(anchor, sink).map_err(Tb1CustodyError::NativeRead)? {
        AnchorState::Detached
    } else if let Some(head) = snapshot.finalized {
        if native.is_chain_ancestor_of(anchor, head).map_err(Tb1CustodyError::NativeRead)? {
            AnchorState::Finalized
        } else {
            AnchorState::Pending
        }
    } else if snapshot.stop == Some(SettlementStopV1::FinalizedConflict) {
        AnchorState::Pending
    } else {
        // A heavier branch may keep the freeze block but withdraw the work
        // that finalized it. Distinguish a named withdrawal from missing data.
        let prior = native
            .get_native_safe_readiness()
            .map_err(Tb1CustodyError::NativeRead)?
            .filter(|r| r.generation == sink && r.stop == snapshot.stop)
            .and_then(|r| r.finalized.withdrawn_from)
            .ok_or_else(|| Tb1CustodyError::NativeUnavailable("native finalized head unavailable"))?;
        if !native.is_chain_ancestor_of(anchor, prior).map_err(Tb1CustodyError::NativeRead)? {
            return Err(Tb1CustodyError::NativeUnavailable("native withdrawal does not identify this freeze"));
        }
        AnchorState::Pending
    };
    let sink_daa = native.get_header(sink).map_err(Tb1CustodyError::NativeRead)?.daa_score;
    if native.get_native_settlement_snapshot().map_err(Tb1CustodyError::NativeRead)?.as_ref() != Some(&snapshot)
        || native.get_sink() != sink
    {
        return Err(Tb1CustodyError::ViewChanged { tip: sink });
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
    pub fn provision(dir: &Path, round: &[u8], key: &MLDSA65VerificationKey, slot: u32) -> Result<(), Tb1CustodyError> {
        let id = binding(round, key)?;
        let directory = File::open(dir)?;
        let mut file = options().create_new(true).open(location(dir, &id))?;
        file.write_all(&header(&id, slot))?;
        file.sync_all()?;
        directory.sync_all()?;
        Ok(())
    }

    pub fn resume(dir: &Path, round: &[u8], key: &MLDSA65VerificationKey, slot: u32) -> Result<Self, Tb1CustodyError> {
        let id = binding(round, key)?;
        let file = options().open(location(dir, &id))?;
        if !file.metadata()?.is_file() {
            return Err(Tb1CustodyError::Corrupt("TB1 custody is not a regular file"));
        }
        try_exclusive_lease(&file)
            .map_err(|error| if error.kind() == ErrorKind::WouldBlock { Tb1CustodyError::JournalInUse } else { error.into() })?;
        let mut journal = Self { file, directory: File::open(dir)?, id, key: key.clone(), slot };
        journal.read_record()?;
        Ok(journal)
    }

    fn read_record(&mut self) -> Result<Option<[u8; BODY]>, Tb1CustodyError> {
        self.file.rewind()?;
        let mut stored_header = [0; HEADER];
        self.file.read_exact(&mut stored_header).map_err(|error| {
            if error.kind() == ErrorKind::UnexpectedEof {
                Tb1CustodyError::Corrupt("incomplete TB1 enrollment header")
            } else {
                error.into()
            }
        })?;
        if &stored_header[..MAGIC.len()] != MAGIC {
            return Err(Tb1CustodyError::Corrupt("invalid TB1 custody header"));
        }
        if stored_header != header(&self.id, self.slot) {
            return Err(Tb1CustodyError::EnrollmentMismatch);
        }
        let size = self.file.metadata()?.len();
        if size == HEADER as u64 {
            return Ok(None);
        }
        if size != (HEADER + RECORD) as u64 && size != (HEADER + RECORD + 64) as u64 {
            return Err(Tb1CustodyError::Corrupt("incomplete or oversized TB1 exposure record"));
        }
        let mut body = [0; BODY];
        let mut checksum = [0; 64];
        self.file.read_exact(&mut body)?;
        self.file.read_exact(&mut checksum)?;
        if Self::checksum(&stored_header, &body) != checksum {
            return Err(Tb1CustodyError::ChecksumMismatch);
        }
        if size > (HEADER + RECORD) as u64 {
            let mut abort = [0; 64];
            self.file.read_exact(&mut abort)?;
            if abort != Self::abort_marker(&checksum) {
                return Err(Tb1CustodyError::Corrupt("invalid TB1 exposure abort record"));
            }
            // The writer may have failed after the append and before a sync.
            self.file.sync_all()?;
            self.directory.sync_all()?;
            return Err(Tb1CustodyError::Halted);
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
    pub fn observe_branch(&mut self, native: &dyn ConsensusApi) -> Result<(), Tb1CustodyError> {
        if let Some(body) = self.read_record()? {
            self.check_exposed(native, &body)?;
        }
        Ok(())
    }

    fn check_exposed(&mut self, native: &dyn ConsensusApi, body: &[u8; BODY]) -> Result<u64, Tb1CustodyError> {
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
                Err(Tb1CustodyError::Halted)
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
    ) -> Result<Vec<u8>, Tb1CustodyError> {
        let previous = self.read_record()?;
        if let Some(saved) = &previous {
            let daa = self.check_exposed(native, saved)?;
            freeze.check_window(daa)?;
        }
        let message = qualified.holder_release_message(row, &self.key, byte_limit).map_err(Tb1CustodyError::InvalidRelease)?;
        if row.context.qual_root != freeze.qual_root
            || row.context.freeze_id != freeze.freeze_id
            || row.holder_slot != self.slot as usize
            || binding(row.context.round_ref, &self.key)? != self.id
        {
            return Err(Tb1CustodyError::ReleaseEnrollmentMismatch);
        }
        let mut body = [0; BODY];
        body[..64].copy_from_slice(freeze.accepting_block.as_byte_slice());
        body[64..128].copy_from_slice(&row.context.qual_root);
        body[128..192].copy_from_slice(&row.context.freeze_id);
        body[192..256].copy_from_slice(&Sha3_512::digest(&message));
        body[256..264].copy_from_slice(&freeze.release_from_daa.to_be_bytes());
        body[264..].copy_from_slice(&freeze.deadline_exclusive_daa.to_be_bytes());
        match previous {
            Some(previous) if previous != body => return Err(Tb1CustodyError::Conflict),
            Some(_) => {}
            None => {
                freeze.check_tip_deadline(native)?;
                let (state, daa, _) = anchor_state(native, freeze.accepting_block)?;
                freeze.check_window(daa)?;
                if !matches!(state, AnchorState::Finalized) {
                    return Err(Tb1CustodyError::NotFinalized);
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
