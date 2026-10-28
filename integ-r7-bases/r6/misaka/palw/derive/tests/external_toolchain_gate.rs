//! **ADR-0079 Decision 12 / invariant S11 — the external toolchain gate.**
//!
//! *"An external toolchain runs with no network, an ephemeral tree and the manifest's whitelist;
//! its outputs are never executed on a host holding a bond or wallet key."* (S11) — and Decision
//! 12's own words: *"a `code` row's test log is the execution of a program a model wrote; it runs
//! on a disposable host or in the same confinement with no writable state that outlives it, or the
//! row's transformer does not ship. This is a completion condition for ADR-0078's Q-05, not
//! advice."*
//!
//! The toolchain under test is a fixture — a two-line `/bin/sh` script that copies one file — for
//! the reason ADR-0078 Decision 11 gives: *no external toolchain is named by an object until its
//! fleet drill passes*, so there is no rustc, solc or clang here and there is nothing in
//! `register()` that could reach one. What is under test is the GATE.
//!
//! One test function: it moves `MISAKA_PALW_CONFINEMENT` and a signing-secret variable, and an
//! environment is process-wide.

use misaka_palw::host_security::{PALW_CONFINEMENT_ENV, confinement_backend_available};
use misaka_palw_derive::DeriveError;
use misaka_palw_derive::kinds::code::{ExternalToolchainManifest, WORK_DIR_PREFIX, fresh_work_dir, run_external, sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

const FIXTURE: &[u8] = b"#!/bin/sh\ncat \"$1\" > \"$2\"\n";

fn manifest(binary_sha256: [u8; 32]) -> ExternalToolchainManifest {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
    ExternalToolchainManifest {
        name: "fake-copy/1".to_string(),
        binary_sha256,
        argv: vec!["{src}/hello.txt".to_string(), "{out}/hello.out".to_string()],
        env,
        source_date_epoch: 1_700_000_000,
    }
}

fn sources() -> BTreeMap<String, String> {
    let mut sources = BTreeMap::new();
    sources.insert("hello.txt".to_string(), "hello, hermetic world\n".to_string());
    sources
}

/// Every ephemeral tree an external run could have left behind.
fn stray_trees() -> Vec<PathBuf> {
    let prefix = format!("{WORK_DIR_PREFIX}run-{}-", std::process::id());
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix)))
        .collect()
}

fn refusal(result: Result<Vec<(String, Vec<u8>)>, DeriveError>) -> String {
    match result {
        Err(DeriveError::Transformer(msg)) => msg,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
#[cfg(unix)]
fn the_external_toolchain_gate_refuses_a_bare_host_and_a_host_with_keys() {
    use std::os::unix::fs::PermissionsExt;

    let dir = fresh_work_dir("gate-fixture").expect("a scratch directory");
    let script = dir.join("copy.sh");
    std::fs::write(&script, FIXTURE).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let good = manifest(sha256(&std::fs::read(&script).unwrap()));
    let outs = ["hello.out".to_string()];

    // Single-test binary: this is the only thread that touches the environment.
    unsafe {
        std::env::remove_var(PALW_CONFINEMENT_ENV);
        std::env::remove_var("MISAKA_BOND_KEY_SEED");
    }

    // (a) A host whose backend is `none` does not run an external toolchain at all. There is no
    // network denial to be had on such a host, and a declared one would be a promise.
    let msg = refusal(run_external(&good, &script, &sources(), &outs, &[]));
    assert!(msg.contains("backend is `none`"), "{msg}");
    assert!(msg.contains("named only by its fleet drill"), "the refusal names the fence: {msg}");
    assert!(stray_trees().is_empty(), "a refused run left a tree: {:?}", stray_trees());

    // (b) The binary pin still refuses first, and before any tree is made.
    let wrong = ExternalToolchainManifest { binary_sha256: [0xAB; 32], ..good.clone() };
    let msg = refusal(run_external(&wrong, &script, &sources(), &outs, &[]));
    assert!(msg.contains("hashes to") && msg.contains("the manifest names"), "{msg}");

    // (c) A reachable signing secret refuses BEFORE the binary is even read — the strongest of the
    // three, because the output of this build would be executed on this host.
    let key_dir = dir.join("identity");
    std::fs::create_dir(&key_dir).unwrap();
    std::fs::write(key_dir.join("bond.seed"), [0x7u8; 32]).unwrap();
    let msg = refusal(run_external(&good, &script, &sources(), &outs, &[&key_dir]));
    assert!(msg.contains("bond or wallet key"), "{msg}");
    assert!(msg.contains("bond.seed"), "the refusal names what it found: {msg}");
    // The same refusal for the environment half, with no directory named at all.
    unsafe { std::env::set_var("MISAKA_BOND_KEY_SEED", "0102030405") };
    let msg = refusal(run_external(&good, &script, &sources(), &outs, &[]));
    assert!(msg.contains("MISAKA_BOND_KEY_SEED"), "{msg}");
    unsafe { std::env::remove_var("MISAKA_BOND_KEY_SEED") };
    // A key directory with nothing key-shaped in it is not a refusal.
    let empty = dir.join("outbox");
    std::fs::create_dir(&empty).unwrap();
    assert!(refusal(run_external(&good, &script, &sources(), &outs, &[&empty])).contains("backend is `none`"));

    // (d) With a backend in force the toolchain RUNS, inside the ephemeral tree, and the tree is
    // gone afterwards. On a host whose drill does not pass this half cannot be tested and says so
    // rather than asserting something it did not observe.
    // The backend this build could install HERE: the Linux backend exercises this the day it
    // ships, and a host with none takes the refusal branch below and says so.
    unsafe { std::env::set_var(PALW_CONFINEMENT_ENV, confinement_backend_available().name()) };
    let result = run_external(&good, &script, &sources(), &outs, &[&empty]);
    match result {
        Ok(collected) => {
            assert_eq!(collected, vec![("hello.out".to_string(), b"hello, hermetic world\n".to_vec())]);
            assert!(stray_trees().is_empty(), "the ephemeral tree outlived the run: {:?}", stray_trees());

            // A missing output is named, and that run's tree is gone too.
            let msg = refusal(run_external(&good, &script, &sources(), &["missing.out".to_string()], &[]));
            assert!(msg.contains("collect out/missing.out"), "{msg}");
            assert!(stray_trees().is_empty(), "a failed collection left a tree: {:?}", stray_trees());

            // Even with a backend in force, a reachable key still refuses: the cage is not the
            // answer to "this host holds a bond".
            let msg = refusal(run_external(&good, &script, &sources(), &outs, &[&key_dir]));
            assert!(msg.contains("bond or wallet key"), "{msg}");
        }
        Err(DeriveError::Transformer(msg)) if msg.contains("backend is `none`") => {
            eprintln!(
                "SKIPPED (the execution half): this host's confinement drill did not pass, so `run_external` \
                 correctly refuses and there is no in-force backend to execute under. The drill said: {msg}"
            );
        }
        other => panic!("{other:?}"),
    }
    unsafe { std::env::remove_var(PALW_CONFINEMENT_ENV) };

    // A source path that would escape the tree is refused before anything is written or spawned.
    let mut escaping = BTreeMap::new();
    escaping.insert("../escape.txt".to_string(), String::new());
    assert!(matches!(run_external(&good, &script, &escaping, &[], &[]), Err(DeriveError::Grammar(_))));

    let _ = std::fs::remove_dir_all(&dir);
}
