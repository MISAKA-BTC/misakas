// Actual janitor-generated WAL across a real renamed snapshot and controlled
// directory-barrier refusal. The finality oracle is an explicit storage fixture.
use super::*;
use crate::monetary_material::PUBLICATION_CHECKPOINT_V4;
fn fail_after_rename(stage: u8) -> std::io::Result<()> {
    if stage == 2 { Err(std::io::Error::other("GC directory barrier control")) } else { Ok(()) }
}
struct ResetHook;
impl Drop for ResetHook {
    fn drop(&mut self) {
        PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(None));
    }
}
#[test]
fn provider_publication_janitor_keeps_exact_durable_intent_until_cold_replay() {
    let dir = std::env::temp_dir().join(format!(
        "misaka-gc-publication-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let file = dir.join("provider");
    let (mut store, row, ledger) = typed_store(&file);
    let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap();
    let tail = row.tail_block.unwrap();
    let release = h(501);
    let root = row.terms.root;
    let consent = row.consent;
    let expected_claims: Vec<_> =
        store.held_native_authorities().get(&consent).map(|claims| claims.iter().copied().collect()).unwrap_or_default();
    let mut oracle = Fake { rows: Ok(vec![row]), chain: vec![release], finalized: Some((root, consent, tail, release)), calls: 0 };
    assert_eq!(janitor.tick(&mut store, &mut oracle).unwrap().retained, 1);
    let journal = fs::read(dir.join("provider.gc-journal")).unwrap();
    oracle.rows = Ok(vec![]);
    PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(Some(fail_after_rename)));
    let _reset = ResetHook;
    assert!(janitor.tick(&mut store, &mut oracle).is_err());
    PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(None));
    let why = store.ensure_published_state_v4().unwrap_err();
    assert_eq!(store.held_contracts(), vec![(consent, root)], "the obsolete projection cannot claim the disk's absence");
    let path = dir.join("provider.gc-intent");
    let bytes = fs::read(&path).unwrap();
    let saved: MonetaryGcCompletedV2 = borsh::from_slice(&bytes).unwrap();
    assert_eq!(
        (saved.intent.root, saved.intent.stored_consent, saved.intent.release_consent, saved.intent.tail, saved.intent.release_block),
        (root, consent, consent, tail, release)
    );
    assert_eq!(
        saved.native_claims, expected_claims,
        "preserve the actual saved inventory; this kernel storage fixture has no native-held authority IDs"
    );
    assert_eq!(
        fs::read(dir.join("provider.gc-journal")).unwrap(),
        journal,
        "uncertain publication cannot publish a misleading completed/watch journal"
    );
    let calls = oracle.calls;
    assert_eq!(janitor.tick(&mut store, &mut oracle).unwrap_err(), why);
    assert_eq!(oracle.calls, calls, "retry on stale memory must not ask more witnesses");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    drop(store);
    drop(janitor);
    let mut store = DurableMonetaryMaterialProviderV1::open(&file, 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
    assert!(store.held_contracts().is_empty(), "the actual renamed snapshot is recovered");
    let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap();
    // A missing witness still cannot convert physical absence into completion.
    oracle.finalized = None;
    assert!(janitor.tick(&mut store, &mut oracle).unwrap().deleted.is_empty());
    assert!(janitor.completed_receipts().next().is_none());
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "already-published deletion must keep its only durable tuple across authority refusal"
    );
    oracle.finalized = Some((root, consent, tail, release));
    let report = janitor.tick(&mut store, &mut oracle).unwrap();
    assert_eq!(report.deleted, vec![consent]);
    assert_eq!(report.retained, 0);
    assert!(!path.exists());
    assert_eq!(janitor.completed_receipts().cloned().collect::<Vec<_>>(), vec![saved.clone()]);
    drop(store);
    drop(janitor);
    assert_eq!(MonetaryMaterialGcJanitorV1::open(&file).unwrap().completed_receipts().cloned().collect::<Vec<_>>(), vec![saved]);
    fs::remove_dir_all(dir).unwrap();
}
