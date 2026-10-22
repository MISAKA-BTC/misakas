//! Crash-recovery, reorg-across-switch and peak-bytes tests for generation storage (scaled-down caps; the logic, not
//! the bytes, is what is tested).

use super::{
    reserve::{FixedBudgetReserver, StorageReserver, UnboundedReserver},
    spill::{SpillArea, SpillError},
    undo::UndoLog,
    *,
};
use std::{collections::BTreeMap, io::Read, sync::Arc};

fn unbounded() -> Arc<dyn StorageReserver> {
    Arc::new(UnboundedReserver)
}

fn marker(subject: &str) -> VerifiedMarker {
    VerifiedMarker { subject: subject.into(), root_hex: "ab".repeat(64), whole_state_checked: true }
}

/// A generation's toy content: one file of `key=value` lines.
fn write_state(dir: &Path, state: &BTreeMap<String, String>) {
    let text: String = state.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    fs::write(dir.join("state"), text).unwrap();
}
fn read_state(dir: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(dir.join("state"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

/// Build a verified generation holding `state` and switch to it.
fn install(store: &mut GenerationStore, state: &BTreeMap<String, String>, subject: &str) -> u64 {
    let staging = store.begin_staging(1_000).unwrap();
    write_state(&staging.path, state);
    store.mark_verified(&staging, &marker(subject)).unwrap();
    store.switch_to(staging).unwrap().number
}

fn base(tag: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("tag".to_string(), tag.to_string())])
}

/// Every crash point of a full staging → verify → switch cycle, starting from a verified active generation. After
/// the crash, recovery must name a VERIFIED generation active — the old one for every point before the journal
/// rename of the switch, the new one from that rename on — and delete everything else.
#[test]
fn a_crash_at_every_switch_step_recovers_to_a_verified_generation() {
    use CrashPoint::*;
    let cases = [
        (StagingAfterCleanup, false),
        (StagingAfterJournalTmp, false),
        (StagingAfterJournal, false),
        (StagingAfterDir, false),
        (VerifyAfterMarkerTmp, false),
        (VerifyAfterMarker, false),
        (SwitchAfterJournalTmp, false),
        (SwitchAfterJournal, true),
    ];
    for (point, switched) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
        let old = install(&mut store, &base("old"), "old");
        store.arm_crash_point(point);
        let run = (|| -> GenerationResult<()> {
            let staging = store.begin_staging(1_000)?;
            write_state(&staging.path, &base("new"));
            store.mark_verified(&staging, &marker("new"))?;
            store.switch_to(staging)?;
            Ok(())
        })();
        assert!(matches!(run, Err(GenerationError::SimulatedCrash(p)) if p == point), "{point:?}: {run:?}");
        drop(store); // process death: nothing below runs any cleanup

        let (store, report) = GenerationStore::open(dir.path(), unbounded()).unwrap();
        let active = report.active.expect("a verified generation always survives a crash");
        assert!(store.is_verified(active), "{point:?}: active must be verified");
        let tag = read_state(&store.active().unwrap().path)["tag"].clone();
        if switched {
            assert_eq!(tag, "new", "{point:?}: the rename is the switch");
            assert_eq!(report.previous, Some(old));
        } else {
            assert_eq!(active, old, "{point:?}: nothing before the rename may promote the staging generation");
            assert_eq!(tag, "old");
        }
        assert!(store.generations_on_disk().unwrap().len() <= 2, "{point:?}: never more than two generations");
        assert!(!dir.path().join(JOURNAL_NEW_FILE).exists(), "{point:?}: uncommitted journal discarded");
        assert_eq!(store.staging_number(), None, "{point:?}: an unfinished staging is never resumed as verified");
    }
}

/// A crash inside retirement and inside roll-back leaves a store that still opens on a verified generation.
#[test]
fn a_crash_while_retiring_or_rolling_back_recovers() {
    for roll_back in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
        let g1 = install(&mut store, &base("g1"), "g1");
        let g2 = install(&mut store, &base("g2"), "g2");
        store.arm_crash_point(CrashPoint::RetireAfterRemove);
        let r = if roll_back { store.roll_back_to_previous().map(|_| ()) } else { store.retire_previous() };
        assert!(matches!(r, Err(GenerationError::SimulatedCrash(CrashPoint::RetireAfterRemove))));
        drop(store);
        let (store, report) = GenerationStore::open(dir.path(), unbounded()).unwrap();
        let want = if roll_back { g1 } else { g2 };
        assert_eq!(report.active, Some(want));
        assert_eq!(report.previous, None);
        assert_eq!(store.generations_on_disk().unwrap(), vec![want]);
    }
    // And a crash between writing the roll-back journal and renaming it keeps the switch as it was.
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    let g1 = install(&mut store, &base("g1"), "g1");
    let g2 = install(&mut store, &base("g2"), "g2");
    store.arm_crash_point(CrashPoint::RollBackAfterJournalTmp);
    assert!(store.roll_back_to_previous().is_err());
    drop(store);
    let (_, report) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    assert_eq!((report.active, report.previous), (Some(g2), Some(g1)));
    assert!(report.discarded_uncommitted_journal);
}

/// Receiving a root alone is never "verified", and an unverified staging generation can never be switched in.
#[test]
fn an_unverified_generation_cannot_become_active() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    let staging = store.begin_staging(10).unwrap();
    let root_only = VerifiedMarker { whole_state_checked: false, ..marker("root only") };
    assert!(matches!(store.mark_verified(&staging, &root_only), Err(GenerationError::RootAloneIsNotVerified(_))));
    let n = staging.number;
    assert!(matches!(store.switch_to(staging), Err(GenerationError::NotVerified(m)) if m == n));
    // A marker that names another generation (a directory copied or renamed by hand) is not a verification either.
    fs::write(
        dir.path().join(gen_dir_name(n)).join(VERIFIED_FILE),
        render_checked(VERIFIED_MAGIC, &[("gen", (n + 7).to_string()), ("whole_state_checked", "1".into())]),
    )
    .unwrap();
    assert!(!store.is_verified(n));
}

/// Disk corruption of the active generation's marker: recovery falls back to the verified previous one; with none,
/// it reports no active generation (resync) rather than handing out unverified state.
#[test]
fn a_corrupt_active_marker_falls_back_or_demands_resync() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    let g1 = install(&mut store, &base("g1"), "g1");
    let g2 = install(&mut store, &base("g2"), "g2");
    drop(store);
    fs::write(dir.path().join(gen_dir_name(g2)).join(VERIFIED_FILE), b"torn").unwrap();
    let (store, report) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    assert_eq!(report.active, Some(g1));
    assert!(report.fell_back_to_previous);
    assert_eq!(report.removed, vec![g2]);
    drop(store);
    fs::write(dir.path().join(gen_dir_name(g1)).join(VERIFIED_FILE), b"torn").unwrap();
    let (_, report) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    assert_eq!(report.active, None, "nothing verified survives: the caller must resync");
}

/// A torn journal (never produced by the rename discipline, only by disk failure) is refused, not guessed at.
#[test]
fn a_corrupt_journal_is_refused_with_the_verified_candidates_listed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    let g1 = install(&mut store, &base("g1"), "g1");
    drop(store);
    fs::write(dir.path().join(JOURNAL_FILE), b"misaka-generations v1\nseq=9\n").unwrap();
    match GenerationStore::open(dir.path(), unbounded()) {
        Err(GenerationError::JournalCorrupt(_, verified)) => assert_eq!(verified, vec![g1]),
        other => panic!("expected JournalCorrupt, got {other:?}"),
    }
}

/// The losing generation is removed BEFORE the new reservation: under a budget that fits exactly two generations,
/// three consecutive switches all succeed, and at no point are three directories (or three reservations) live.
#[test]
fn the_losing_generation_is_cleaned_before_new_space_is_reserved() {
    let dir = tempfile::tempdir().unwrap();
    let budget = Arc::new(FixedBudgetReserver::new(1_000));
    let (mut store, _) = GenerationStore::open(dir.path(), budget.clone()).unwrap();
    for i in 0..3 {
        let staging = store.begin_staging(1_000).unwrap();
        assert!(store.generations_on_disk().unwrap().len() <= 2, "switch {i}");
        assert_eq!(budget.used(), 1_000);
        // A second concurrent staging reservation does not fit: no double-counting of free space.
        assert!(StorageReservation::acquire(&(budget.clone() as Arc<dyn StorageReserver>), StoragePurpose::StagingGeneration, 1).is_err());
        write_state(&staging.path, &base(&i.to_string()));
        store.mark_verified(&staging, &marker("g")).unwrap();
        store.switch_to(staging).unwrap();
        assert_eq!(budget.used(), 0, "the reservation turns into measured usage at the switch");
    }
    assert!(budget.high_water() <= 1_000);
    // Refused reservation: nothing is created, the active generation is untouched.
    let before = store.generations_on_disk().unwrap();
    assert!(matches!(store.begin_staging(2_000), Err(GenerationError::Refused(_))));
    let active = store.active().unwrap().number;
    assert_eq!(store.generations_on_disk().unwrap(), vec![active], "previous was retired before the (refused) reservation");
    assert!(before.contains(&active));
}

// ---- reorg across a generation switch ---------------------------------------------------------------------------

/// Toy chain state over a generation: blocks set keys; each block's undo record holds the old values.
struct ToyChain;
impl ToyChain {
    fn apply(gen_dir: &Path, block: &str, writes: &[(&str, &str)]) {
        let mut state = read_state(gen_dir);
        let mut undo = UndoLog::open(gen_dir.join("undo.log")).unwrap();
        let old: String = writes.iter().map(|(k, _)| format!("{k}={}\n", state.get(*k).cloned().unwrap_or_else(|| "\u{0}".into()))).collect();
        undo.append(block.as_bytes(), old.as_bytes()).unwrap();
        for (k, v) in writes {
            state.insert(k.to_string(), v.to_string());
        }
        state.insert("tip".into(), block.into());
        write_state(gen_dir, &state);
    }
    /// Pop one block; returns its id. Restoring old values is idempotent, so a crash between the restore and the pop
    /// replays harmlessly.
    fn undo_one(gen_dir: &Path) -> Option<String> {
        let mut undo = UndoLog::open(gen_dir.join("undo.log")).unwrap();
        let rec = undo.last().unwrap()?;
        let mut state = read_state(gen_dir);
        for line in String::from_utf8(rec.payload.clone()).unwrap().lines() {
            let (k, v) = line.split_once('=').unwrap();
            if v == "\u{0}" {
                state.remove(k);
            } else {
                state.insert(k.into(), v.into());
            }
        }
        undo.pop().unwrap();
        let tip = undo.last().unwrap().map(|r| String::from_utf8(r.id).unwrap()).unwrap_or_else(|| state["base"].clone());
        state.insert("tip".into(), tip);
        write_state(gen_dir, &state);
        Some(String::from_utf8(rec.id).unwrap())
    }
}

/// Gen 1 has base A and blocks b1, b2 above it. A pruned IBD switches to gen 2 whose verified base is b2. A reorg
/// to the fork c1 (off b1) reaches below gen 2's base: gen 2 cannot undo it, so the store rolls back to the retained
/// verified gen 1, undoes b2 there, and applies c1. Once the previous generation is retired the same reorg is
/// refused as NoPrevious (resync), never reported as an invalid block.
#[test]
fn a_reorg_across_a_generation_switch_rolls_back_to_the_retained_generation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    let mut s = base("g1");
    s.insert("base".into(), "A".into());
    let g1 = install(&mut store, &s, "base A");
    let g1_dir = store.active().unwrap().path;
    ToyChain::apply(&g1_dir, "b1", &[("x", "1")]);
    ToyChain::apply(&g1_dir, "b2", &[("x", "2"), ("y", "2")]);

    // Gen 2: the state at b2, imported and verified as a new base.
    let mut at_b2 = read_state(&g1_dir);
    at_b2.insert("base".into(), "b2".into());
    at_b2.insert("tag".into(), "g2".into());
    let g2 = install(&mut store, &at_b2, "base b2");
    let g2_dir = store.active().unwrap().path;
    ToyChain::apply(&g2_dir, "b3", &[("z", "3")]);
    // Undo within gen 2 works down to its base, and no further.
    assert_eq!(ToyChain::undo_one(&g2_dir).as_deref(), Some("b3"));
    assert_eq!(ToyChain::undo_one(&g2_dir), None, "gen 2 holds no undo below its base b2");

    // The reorg target c1 forks off b1 < base(b2): step back across the switch.
    let back = store.roll_back_to_previous().unwrap();
    assert_eq!(back.number, g1);
    assert!(!store.generations_on_disk().unwrap().contains(&g2), "the losing generation is deleted");
    assert_eq!(ToyChain::undo_one(&back.path).as_deref(), Some("b2"));
    ToyChain::apply(&back.path, "c1", &[("x", "c")]);
    let st = read_state(&back.path);
    assert_eq!((st["x"].as_str(), st.get("y"), st["tip"].as_str()), ("c", None, "c1"));

    // Crash/restart keeps the rolled-back generation and its undo log (torn tail dropped).
    UndoLog::open(back.path.join("undo.log")).unwrap().append_torn_for_test(b"c2", b"x=c\n", 9).unwrap();
    drop(store);
    let (mut store, report) = GenerationStore::open(dir.path(), unbounded()).unwrap();
    assert_eq!(report.active, Some(g1));
    let log = UndoLog::open(store.active().unwrap().path.join("undo.log")).unwrap();
    assert_eq!(log.len(), 2, "b1 and c1; the torn c2 append is gone");

    // Switch forward again, retire the previous: the same deep reorg is now NoPrevious.
    let mut at_c1 = read_state(&store.active().unwrap().path);
    at_c1.insert("base".into(), "c1".into());
    install(&mut store, &at_c1, "base c1");
    store.retire_previous().unwrap();
    assert!(matches!(store.roll_back_to_previous(), Err(GenerationError::NoPrevious)));
    let _ = g2;
}

#[test]
fn the_undo_log_survives_reopen_and_drops_only_a_torn_tail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("undo.log");
    let mut log = UndoLog::open(&path).unwrap();
    for i in 0..5u8 {
        assert_eq!(log.append(&[i], &[i; 10]).unwrap(), i as u64);
    }
    for keep in [0usize, 1, 3, 4, 10, 20, 33] {
        let mut l = UndoLog::open(&path).unwrap();
        l.append_torn_for_test(b"x", &[9; 9], keep).unwrap();
        let l = UndoLog::open(&path).unwrap();
        assert_eq!(l.len(), 5, "keep={keep}");
    }
    let mut l = UndoLog::open(&path).unwrap();
    assert_eq!(l.pop().unwrap().unwrap().id, vec![4]);
    assert_eq!(UndoLog::open(&path).unwrap().len(), 4);
    // A flipped byte inside a middle record ends the trusted prefix there.
    let mut bytes = fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    let l = UndoLog::open(&path).unwrap();
    assert!(l.len() < 4 && l.torn_bytes_dropped() > 0);
}

// ---- spill -------------------------------------------------------------------------------------------------------

#[test]
fn spill_files_are_capped_counted_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    // A leftover from a dead process is swept on open.
    fs::write(dir.path().join("spill-dead.tmp"), vec![0u8; 50]).unwrap();
    let budget = Arc::new(FixedBudgetReserver::new(2 * 64));
    let area = SpillArea::open(dir.path(), 64, 2, budget.clone()).unwrap();
    assert!(!dir.path().join("spill-dead.tmp").exists());
    let mut a = area.acquire().unwrap();
    let mut b = area.acquire().unwrap();
    assert!(matches!(area.acquire(), Err(SpillError::Busy(2))));
    assert_eq!(budget.used(), 128, "each file reserves its full cap before it exists");
    a.write_all(&[1; 64]).unwrap();
    let err = a.write_all(&[1]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::StorageFull, "past the cap is an error, never a wrap or truncation");
    b.write_all(&[2; 40]).unwrap();
    let mut back = Vec::new();
    a.reader().unwrap().read_to_end(&mut back).unwrap();
    assert_eq!(back, vec![1; 64]);
    assert_eq!(area.accounting().high_water, 104);
    assert!(area.accounting().high_water <= area.max_bytes());
    drop(a);
    drop(b);
    assert_eq!(area.accounting().bytes, 0);
    assert_eq!(budget.used(), 0);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0, "nothing left behind");
    // A refused reservation leaves the slot free.
    let tight = SpillArea::open(dir.path(), 64, 2, Arc::new(FixedBudgetReserver::new(10))).unwrap();
    assert!(matches!(tight.acquire(), Err(SpillError::Refused(_))));
    assert_eq!(tight.accounting().files, 0);
}
