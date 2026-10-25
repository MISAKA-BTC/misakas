//! Real provider snapshot publication/lease/reopen controls. The finality callback
//! is explicit fixture input, not accepted native release authority or OS-crash evidence.
use super::*;
use crate::monetary_material::PUBLICATION_CHECKPOINT_V4;
use crate::monetary_material_gc::{MonetaryMaterialGcJanitorV1, MonetaryMaterialGcOracleV1};
use std::io;

struct ResetHook;
impl Drop for ResetHook {
    fn drop(&mut self) {
        PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(None));
    }
}
fn before_rename(stage: u8) -> io::Result<()> {
    if stage == 1 { Err(io::Error::other("injected failure after file fsync, before rename")) } else { Ok(()) }
}
fn after_rename(stage: u8) -> io::Result<()> {
    if stage == 2 { Err(io::Error::other("injected failure after rename, before directory fsync")) } else { Ok(()) }
}
fn candidate_without(
    store: &DurableMonetaryMaterialProviderV1,
    root: Hash64,
    consent: Hash64,
) -> kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialProviderStoreV1> {
    let (candidate, released) =
        store.store.provider_release_finalized_v1(root, consent, consent, h(400), h(401), |_, _, _, _| Ok(true)).unwrap();
    assert!(released);
    candidate
}
struct NoOracle;
impl MonetaryMaterialGcOracleV1 for NoOracle {
    fn covenant_rows(
        &mut self,
    ) -> Result<(Hash64, Vec<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1>), String> {
        panic!("uncertain store must refuse before native discovery")
    }
    fn chain_after(&mut self, _: Hash64, _: usize) -> Result<Vec<Hash64>, String> {
        panic!("no native chain read")
    }
    fn release_finalized(&mut self, _: Hash64, _: Hash64, _: Hash64, _: Hash64) -> Result<bool, String> {
        panic!("no witness")
    }
}
#[test]
fn provider_publication_before_rename_failure_keeps_live_generation_and_retries() {
    let dir = FixtureDir::new();
    let file = dir.file();
    let (mut store, first, second, root, provider, ledger) = typed_store_with_ledger(&file);
    let old = fs::read(&file).unwrap();
    let candidate = candidate_without(&store, root, first);
    PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(Some(before_rename)));
    let _reset = ResetHook;
    assert!(store.publish(candidate).is_err());
    PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(None));
    assert!(!store.publication_uncertain);
    store.ensure_published_state_v4().unwrap();
    assert_eq!(fs::read(&file).unwrap(), old);
    assert_eq!(store.held_contracts().len(), 2);
    assert!(
        store
            .answer_material_owned_v1(first, root, root, provider, PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 })
            .is_ok()
    );
    assert!(store.gc_finalized(root, first, first, h(400), h(401), |_, _, _, _| Ok(true)).unwrap());
    assert_eq!(store.held_contracts(), vec![(second, root)]);
    drop(store);
    let cold = DurableMonetaryMaterialProviderV1::open(&file, 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
    assert_eq!(cold.held_contracts(), vec![(second, root)]);
}
#[test]
fn provider_publication_after_rename_failure_requires_reopen_for_both_snapshot_paths() {
    for routed in [false, true] {
        let dir = FixtureDir::new();
        let file = dir.file();
        let (mut store, first, second, root, provider, ledger) = typed_store_with_ledger(&file);
        let old = fs::read(&file).unwrap();
        let candidate = candidate_without(&store, root, first);
        PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(Some(after_rename)));
        let _reset = ResetHook;
        if routed {
            assert!(store.publish_routed_v2(candidate).is_err());
        } else {
            assert!(store.publish(candidate).is_err());
        }
        PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(None));
        assert!(store.publication_uncertain);
        assert_ne!(fs::read(&file).unwrap(), old, "rename really changed the persisted snapshot");
        assert_eq!(store.held_contracts().len(), 2, "old projection remains only for diagnostics");
        let why = store.ensure_published_state_v4().unwrap_err();
        assert_eq!(
            store
                .answer_material_owned_v1(second, root, root, provider, PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 })
                .err()
                .unwrap(),
            why
        );
        assert_eq!(
            store
                .answer_current_generation_with_v4(
                    first,
                    root,
                    root,
                    false,
                    |_, _| -> Result<(), AuthorityError> { panic!("uncertain state must not request authority") },
                    |_, _| -> Result<(), String> { panic!("uncertain state must not build/sign an answer") }
                )
                .unwrap_err(),
            why
        );
        assert_eq!(
            store
                .gc_finalized(root, second, second, h(400), h(401), |_, _, _, _| -> Result<bool, String> {
                    panic!("no further deletion witness")
                })
                .unwrap_err(),
            why
        );
        let candidate = candidate_without(&store, root, second);
        assert_eq!(store.publish(candidate).unwrap_err(), why);
        // The real janitor must refuse before reading/replacing an existing WAL.
        let marker = file.with_file_name("provider.gc-intent");
        fs::write(&marker, b"existing intent must remain byte-identical").unwrap();
        let mut janitor = MonetaryMaterialGcJanitorV1::open(&file).unwrap();
        assert_eq!(janitor.tick(&mut store, &mut NoOracle).unwrap_err(), why);
        assert_eq!(fs::read(&marker).unwrap(), b"existing intent must remain byte-identical");
        fs::remove_file(marker).unwrap();
        drop(store);
        let reopened = DurableMonetaryMaterialProviderV1::open(&file, 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
        reopened.ensure_published_state_v4().unwrap();
        assert_eq!(reopened.held_contracts(), vec![(second, root)]);
        assert!(
            reopened
                .answer_material_owned_v1(first, root, root, provider, PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 })
                .is_err()
        );
        assert!(
            reopened
                .answer_material_owned_v1(second, root, root, provider, PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 })
                .is_ok()
        );
    }
}

#[cfg(unix)]
fn wait_for_kill(stage: u8) -> io::Result<()> {
    if stage.to_string() == std::env::var("MISAKA_PROVIDER_KILL_STAGE_V4").unwrap() {
        fs::write(std::env::var_os("MISAKA_PROVIDER_KILL_READY_V4").unwrap(), stage.to_string())?;
        loop {
            std::thread::park_timeout(std::time::Duration::from_secs(1));
        }
    }
    Ok(())
}
#[cfg(unix)]
struct KillChild(std::process::Child);
#[cfg(unix)]
impl Drop for KillChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[cfg(unix)]
#[test]
fn provider_publication_sigkill_releases_lease_and_reopens_exact_atomic_generation() {
    const DIR: &str = "MISAKA_PROVIDER_KILL_DIR_V4";
    if let Some(dir) = std::env::var_os(DIR) {
        let file = PathBuf::from(dir).join("provider");
        let (mut store, first, _, root, _, _) = typed_store_with_ledger(&file);
        PUBLICATION_CHECKPOINT_V4.with(|hook| hook.set(Some(wait_for_kill)));
        // Both checkpoints execute after actual candidate write + file fsync;
        // stage2 additionally follows actual rename. This call never returns.
        let _ = store.gc_finalized(root, first, first, h(400), h(401), |_, _, _, _| Ok(true));
        panic!("the crash child must stop at its publication checkpoint");
    }
    use std::os::unix::process::ExitStatusExt;
    let reference = FixtureDir::new();
    let (reference_store, first, second, root, provider, ledger) = typed_store_with_ledger(&reference.file());
    drop(reference_store);
    for stage in [1u8, 2] {
        let dir = FixtureDir::new();
        fs::create_dir_all(&dir.0).unwrap();
        let ready = dir.0.join("ready");
        let full = format!("{}::provider_publication_sigkill_releases_lease_and_reopens_exact_atomic_generation", module_path!());
        let mut child = KillChild(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(DIR, &dir.0)
                .env("MISAKA_PROVIDER_KILL_STAGE_V4", stage.to_string())
                .env("MISAKA_PROVIDER_KILL_READY_V4", &ready)
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !ready.exists() {
            assert!(child.0.try_wait().unwrap().is_none(), "child exited before publication checkpoint");
            assert!(std::time::Instant::now() < deadline, "publication checkpoint timeout");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(fs::read_to_string(&ready).unwrap(), stage.to_string());
        assert!(
            DurableMonetaryMaterialProviderV1::open(&dir.file(), 1 << 24, 1 << 26, Some(&ledger), |_| None).is_err(),
            "live child holds the actual flock lease"
        );
        child.0.kill().unwrap();
        assert_eq!(child.0.wait().unwrap().signal(), Some(9), "actual SIGKILL, no Rust Drop/clean shutdown");
        let store = DurableMonetaryMaterialProviderV1::open(&dir.file(), 1 << 24, 1 << 26, Some(&ledger), |_| None).unwrap();
        store.ensure_published_state_v4().unwrap();
        let mut expected = if stage == 1 { vec![(first, root), (second, root)] } else { vec![(second, root)] };
        expected.sort();
        assert_eq!(store.held_contracts(), expected);
        assert_eq!(
            store
                .answer_material_owned_v1(first, root, root, provider, PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 })
                .is_ok(),
            stage == 1
        );
        assert!(
            store
                .answer_material_owned_v1(second, root, root, provider, PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 })
                .is_ok()
        );
        assert!(
            !fs::read_dir(&dir.0).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("provider.pending-")),
            "reopen removes only interrupted uncommitted snapshots under its lease"
        );
        drop(store);
        assert!(DurableMonetaryMaterialProviderV1::open(&dir.file(), 1 << 24, 1 << 26, Some(&ledger), |_| None).is_ok());
    }
}
