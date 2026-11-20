use super::*;
use kaspa_consensus_core::palw_tb1::{
    Tb1Field,
    qualification::{Tb1DealerEnd, Tb1PreparationCut, Tb1Ready, Tb1Roster, Tb1VssEnd},
    releases::{Tb1ReleaseContext, Tb1ReleaseCut, Tb1ReleaseSet, Tb1SignedRelease},
    signatures::{READY_CONTEXT, RELEASE_CONTEXT, ready_message},
};
use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65KeyPair};
use std::{collections::BTreeMap, fs, os::unix::process::ExitStatusExt, process::Command, sync::LazyLock};
use tempfile::tempdir;

const ROUND: &[u8] = b"private custody EXEC";
const Q: [usize; 3] = [0, 1, 2];
static QUALIFIED: LazyLock<Tb1Qualification<'static>> = LazyLock::new(qualified);
fn original() -> Tb1ReleaseContext<'static> {
    Tb1ReleaseContext { round_ref: ROUND, qual_root: QUALIFIED.root(), freeze_id: [31; 64] }
}
const LIMIT: usize = 2048;
const CHILD_DIR: &str = "MISAKA_PRIVATE_TB1_CUSTODY_TEST_DIRECTORY";
static KEYS: LazyLock<Vec<MLDSA65KeyPair>> = LazyLock::new(|| (0..4).map(|slot| ml_dsa_65::generate_key_pair([slot; 32])).collect());
static PUBLIC: LazyLock<Vec<MLDSA65VerificationKey>> = LazyLock::new(|| KEYS.iter().map(|k| k.verification_key.clone()).collect());
static COORDINATES: LazyLock<Vec<Tb1Field>> = LazyLock::new(|| (1..=4).map(field).collect());

// The reader fixture has selected chains 1-2-3 and 1-4-5. Signatures and storage are real.
use kaspa_consensus_core::{errors::consensus::ConsensusResult, palw_native_settlement_v1::NativeSettlementSnapshotV1};
struct Branch {
    tip: u64,
    finalized: Option<u64>,
}
const A: Branch = Branch { tip: 3, finalized: Some(2) };
const B: Branch = Branch { tip: 5, finalized: Some(4) };
fn block(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn freeze(context: Tb1ReleaseContext<'_>) -> Tb1HolderFreeze {
    Tb1HolderFreeze {
        accepting_block: block(2),
        qual_root: context.qual_root,
        freeze_id: context.freeze_id,
        release_from_daa: 17,
        deadline_exclusive_daa: 25,
    }
}
impl ConsensusApi for Branch {
    fn get_header(&self, hash: Hash64) -> ConsensusResult<std::sync::Arc<kaspa_consensus_core::header::Header>> {
        let mut header = kaspa_consensus_core::header::Header::from(&kaspa_consensus_core::config::genesis::SIMNET_GENESIS);
        header.hash = hash;
        header.daa_score = 19;
        Ok(std::sync::Arc::new(header))
    }
    fn get_sink(&self) -> Hash64 {
        block(self.tip)
    }
    fn get_native_settlement_snapshot(&self) -> ConsensusResult<Option<NativeSettlementSnapshotV1>> {
        Ok(Some(NativeSettlementSnapshotV1 {
            version: 1,
            ruleset_id: Default::default(),
            policy_id: block(22),
            retirement_daa: 0,
            generation: self.get_sink(),
            frontier: Some(self.get_sink()),
            latest: Some(self.get_sink()),
            safe: self.finalized.map(block),
            finalized: self.finalized.map(block),
            depth: 1,
            unique_work: "1".into(),
            stop: None,
        }))
    }
    fn is_chain_ancestor_of(&self, low: Hash64, high: Hash64) -> ConsensusResult<bool> {
        Ok(low == high || low == block(1) || (low == block(2) && high == block(3)) || (low == block(4) && high == block(5)))
    }
}

fn field(value: u8) -> Tb1Field {
    let mut bytes = [0; 66];
    bytes[65] = value;
    Tb1Field::from_bytes(&bytes).unwrap()
}

fn qualified() -> Tb1Qualification<'static> {
    let ends: Vec<_> = (0..4)
        .map(|d| Some(Tb1DealerEnd { end: Tb1VssEnd { transcript_root: [d; 64], end_anchor: [12; 64] }, position: 8 }))
        .collect();
    let mut ready = Vec::new();
    for dealer in Q {
        for slot in 0..3 {
            ready.push(Tb1Ready {
                round_ref: ROUND,
                dealer_slot: dealer,
                holder_slot: slot,
                end: ends[dealer].unwrap().end,
                position: 10,
                signature: &[],
            });
        }
    }
    let signatures: Vec<_> = ready
        .iter()
        .map(|r| {
            ml_dsa_65::sign(&KEYS[r.holder_slot].signing_key, &ready_message(r, LIMIT).unwrap(), READY_CONTEXT, [52; 32]).unwrap()
        })
        .collect();
    for (row, sig) in ready.iter_mut().zip(&signatures) {
        row.signature = sig.as_slice();
    }
    let cut = Tb1PreparationCut { round_ref: ROUND, deadline_position: 12, observed_through: 12, dealer_ends: &ends, ready: &ready };
    Tb1Qualification::resolve(
        Tb1Roster { coordinates: &COORDINATES, holder_keys: &PUBLIC, f: 1 },
        ROUND,
        [21; 64], // reserved resources reference
        &Q,
        &cut,
        12,
        LIMIT,
    )
    .unwrap()
}

fn shares(slot: usize) -> Vec<(usize, Tb1Field)> {
    Q.map(|d| (d, field(10 + d as u8 + 3 * (slot as u8 + 1)))).to_vec()
}

#[test]
fn custody_process() {
    let Some(dir) = std::env::var_os(CHILD_DIR) else { return };
    let slot: usize = std::env::var("MISAKA_TB1_TEST_SLOT").unwrap().parse().unwrap();
    let mode = std::env::var("MISAKA_TB1_TEST_MODE").unwrap();
    let context = if mode == "competing-freeze" { Tb1ReleaseContext { freeze_id: [32; 64], ..original() } } else { original() };
    let shares = shares(slot);
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &shares };
    let dir = Path::new(&dir);
    let mut journal = Tb1ExposureJournal::resume(dir, ROUND, &PUBLIC[slot], slot as u32).unwrap();
    let message = journal.reserve_signing_message(&A, freeze(context), &qualified(), &row, LIMIT);
    match mode.as_str() {
        "kill-before-signature" => {
            message.unwrap();
            // SAFETY: SIGKILL terminates only this disposable test subprocess.
            unsafe { libc::raise(libc::SIGKILL) };
            unreachable!();
        }
        "competing-freeze" => {
            assert!(message.unwrap_err().to_string().contains("different exposure"));
        }
        "sign-original" => {
            let sig = ml_dsa_65::sign(&KEYS[slot].signing_key, &message.unwrap(), RELEASE_CONTEXT, [53; 32]).unwrap();
            let mut file = OpenOptions::new().write(true).create_new(true).open(dir.join(format!("holder-{slot}.sig"))).unwrap();
            file.write_all(sig.as_slice()).unwrap();
        }
        _ => panic!("unknown custody test mode"),
    }
}

fn child(dir: &Path, slot: usize, mode: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["tb1_exposure::tests::custody_process", "--exact", "--nocapture"])
        .env(CHILD_DIR, dir)
        .env("MISAKA_TB1_TEST_SLOT", slot.to_string())
        .env("MISAKA_TB1_TEST_MODE", mode)
        .output()
        .unwrap();
    if mode == "kill-before-signature" {
        assert_eq!(output.status.signal(), Some(libc::SIGKILL));
    } else {
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn three_holders_killed_before_signing_recover_only_the_original_release() {
    let root = tempdir().unwrap();
    let dir = root.path();
    for (slot, key) in PUBLIC.iter().take(3).enumerate() {
        Tb1ExposureJournal::provision(dir, ROUND, key, slot as u32).unwrap();
        child(dir, slot, "kill-before-signature");
        assert!(!dir.join(format!("holder-{slot}.sig")).exists());
        assert_eq!(Tb1ExposureJournal::provision(dir, ROUND, key, slot as u32).unwrap_err().kind(), ErrorKind::AlreadyExists);
        child(dir, slot, "competing-freeze");
        child(dir, slot, "sign-original");
    }
    let shares: Vec<_> = (0..3).map(shares).collect();
    let signatures: Vec<_> = (0..3).map(|s| fs::read(dir.join(format!("holder-{s}.sig"))).unwrap()).collect();
    let signed: BTreeMap<_, _> = (0..3)
        .map(|slot| {
            (
                slot,
                Tb1SignedRelease {
                    row: Tb1HolderRelease { context: original(), holder_slot: slot, shares: &shares[slot] },
                    accepted_position: 18,
                    signature: &signatures[slot],
                },
            )
        })
        .collect();
    let qualified = qualified();
    let cut = Tb1ReleaseCut {
        context: original(),
        closed_at: 14,
        stable_at: 16,
        release_from: 17,
        certificate_at: 20,
        deadline_exclusive: 22,
    };
    let set = Tb1ReleaseSet::new(&cut, &qualified, &signed, 3, LIMIT).unwrap();
    let cert = set.assemble(100).unwrap();
    assert_eq!(set.verify(&cert).unwrap(), Q.map(|d| (d, field(10 + d as u8))));
    let seed = set.verify_seed(&cert).unwrap();
    eprintln!("qualification {}", faster_hex::hex_string(&qualified.root()));
    assert_eq!(faster_hex::hex_string(seed.as_bytes()), "99df6ee58636a43cf174f09e4f60eac5c81ac8e5e7addcc21b7b00de72a06f4e");
    eprintln!("recovered 3/3 original holder signatures, seed {}", faster_hex::hex_string(seed.as_bytes()));
}

#[test]
fn incomplete_substitution_contention_and_cold_import_cannot_reset_custody() {
    let root = tempdir().unwrap();
    let dir = root.path();
    let key = &PUBLIC[0];
    let qualified = qualified();
    let shares = shares(0);
    let row = Tb1HolderRelease { context: original(), holder_slot: 0, shares: &shares };
    assert!(Tb1ExposureJournal::resume(dir, ROUND, key, 0).is_err());
    Tb1ExposureJournal::provision(dir, ROUND, key, 0).unwrap();
    assert!(Tb1ExposureJournal::resume(dir, ROUND, key, 1).is_err());
    let mut journal = Tb1ExposureJournal::resume(dir, ROUND, key, 0).unwrap();
    assert!(matches!(Tb1ExposureJournal::resume(dir, ROUND, key, 0), Err(e) if e.kind() == ErrorKind::WouldBlock));
    assert!(
        journal
            .reserve_signing_message(&A, freeze(original()), &qualified, &Tb1HolderRelease { shares: &shares[..2], ..row }, LIMIT)
            .is_err()
    );
    assert!(journal.reserve_signing_message(&A, freeze(original()), &qualified, &row, 4).is_err());
    let expected = journal.reserve_signing_message(&A, freeze(original()), &qualified, &row, LIMIT).unwrap();
    drop(journal);
    let file = location(dir, &binding(ROUND, key).unwrap());
    let bytes = fs::read(&file).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir, ROUND, key, 0).unwrap();
    assert_eq!(expected, journal.reserve_signing_message(&A, freeze(original()), &qualified, &row, LIMIT).unwrap());
    let extended = Tb1HolderFreeze { deadline_exclusive_daa: 26, ..freeze(original()) };
    assert!(
        journal.reserve_signing_message(&A, extended, &qualified, &row, LIMIT).unwrap_err().to_string().contains("different exposure")
    );
    let mut changed = shares.clone();
    changed[0].1 = field(73);
    assert!(
        journal
            .reserve_signing_message(&A, freeze(original()), &qualified, &Tb1HolderRelease { shares: &changed, ..row }, LIMIT)
            .is_err()
    );
    drop(journal);
    assert_eq!(fs::read(&file).unwrap(), bytes);
    let mut corrupted = bytes.clone();
    corrupted[HEADER] ^= 1;
    for bad in [&bytes[..HEADER - 1], &bytes[..HEADER + 1], &bytes[..bytes.len() - 1], &corrupted[..]] {
        fs::write(&file, bad).unwrap();
        assert!(Tb1ExposureJournal::resume(dir, ROUND, key, 0).is_err());
        assert_eq!(fs::read(&file).unwrap(), bad);
    }
}

#[test]
fn observed_finality_loss_halts_across_branch_return_and_restart() {
    let dir = tempdir().unwrap();
    let key = &PUBLIC[0];
    let q = qualified();
    let shares = shares(0);
    let row = Tb1HolderRelease { context: original(), holder_slot: 0, shares: &shares };
    let pending = Branch { tip: 3, finalized: Some(1) };
    let unavailable = Branch { tip: 3, finalized: None };
    Tb1ExposureJournal::provision(dir.path(), ROUND, key, 0).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir.path(), ROUND, key, 0).unwrap();
    assert!(journal.reserve_signing_message(&pending, freeze(original()), &q, &row, LIMIT).is_err());
    assert!(journal.reserve_signing_message(&B, freeze(original()), &q, &row, LIMIT).is_err());
    for closed in [
        Tb1HolderFreeze { release_from_daa: 20, ..freeze(original()) },
        Tb1HolderFreeze { deadline_exclusive_daa: 19, ..freeze(original()) },
    ] {
        assert!(journal.reserve_signing_message(&A, closed, &q, &row, LIMIT).is_err());
    }
    assert!(journal.read_record().unwrap().is_none());
    journal.reserve_signing_message(&A, freeze(original()), &q, &row, LIMIT).unwrap();
    assert!(journal.observe_branch(&unavailable).is_err());
    journal.observe_branch(&A).unwrap();
    assert!(
        journal
            .reserve_signing_message(&A, Tb1HolderFreeze { accepting_block: block(1), ..freeze(original()) }, &q, &row, LIMIT)
            .is_err()
    );
    assert!(journal.observe_branch(&B).unwrap_err().to_string().contains("halted"));
    assert!(journal.reserve_signing_message(&A, freeze(original()), &q, &row, LIMIT).unwrap_err().to_string().contains("halted"));
    drop(journal);
    assert!(matches!(Tb1ExposureJournal::resume(dir.path(), ROUND, key, 0), Err(e) if e.to_string().contains("halted")));
}

#[test]
fn generation_change_during_read_neither_reserves_nor_halts() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Return one old snapshot, then its replacement at the final consistency check.
    struct MovingView<'a> {
        first: &'a Branch,
        next: &'a Branch,
        calls: AtomicUsize,
    }
    impl ConsensusApi for MovingView<'_> {
        fn get_native_settlement_snapshot(&self) -> ConsensusResult<Option<NativeSettlementSnapshotV1>> {
            let view = if self.calls.fetch_add(1, Ordering::Relaxed) == 0 { self.first } else { self.next };
            view.get_native_settlement_snapshot()
        }
        fn get_sink(&self) -> Hash64 {
            self.first.get_sink()
        }
        fn is_chain_ancestor_of(&self, low: Hash64, high: Hash64) -> ConsensusResult<bool> {
            self.first.is_chain_ancestor_of(low, high)
        }
        fn get_header(&self, hash: Hash64) -> ConsensusResult<std::sync::Arc<kaspa_consensus_core::header::Header>> {
            self.first.get_header(hash)
        }
    }

    let dir = tempdir().unwrap();
    let key = &PUBLIC[0];
    let q = qualified();
    let values = shares(0);
    let row = Tb1HolderRelease { context: original(), holder_slot: 0, shares: &values };
    Tb1ExposureJournal::provision(dir.path(), ROUND, key, 0).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir.path(), ROUND, key, 0).unwrap();
    let moved = MovingView { first: &A, next: &B, calls: AtomicUsize::new(0) };
    assert_eq!(
        journal.reserve_signing_message(&moved, freeze(original()), &q, &row, LIMIT).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert!(journal.read_record().unwrap().is_none());
    journal.reserve_signing_message(&A, freeze(original()), &q, &row, LIMIT).unwrap();
    let saved = journal.read_record().unwrap().unwrap();
    let moved = MovingView { first: &B, next: &A, calls: AtomicUsize::new(0) };
    assert_eq!(journal.observe_branch(&moved).unwrap_err().kind(), ErrorKind::WouldBlock);
    assert_eq!(journal.read_record().unwrap(), Some(saved));
    journal.observe_branch(&A).unwrap();
}
