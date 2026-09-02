//! **ADR-0078 SA-1 — model-written initcode runs in a separate process, or it does not run.**
//!
//! *"`contract` / `code` under `evm/v1` runs model-written initcode: on an ephemeral, isolated
//! state with a gas ceiling from the transformer manifest, in a separate process under ADR-0079
//! Decision 5's confinement, never against the chain's EVM state and never inside the node
//! process."*
//!
//! One test function, because it moves the process environment (`MISAKA_PALW_EVM_RUNNER`,
//! `MISAKA_PALW_CONFINEMENT`) and an environment is process-wide: a second test running beside it
//! would read a variable this one was in the middle of changing. What it proves, in order:
//!
//! 1. the transformer finds the runner beside the binary that derives, with no configuration;
//! 2. `MISAKA_PALW_EVM_RUNNER` names it when a deployment puts it elsewhere;
//! 3. an ABSENT runner is a refusal — no object — and never an in-process fallback;
//! 4. no ephemeral tree outlives its run;
//! 5. the artifact is byte-identical with the platform backend in force and with it absent
//!    (ADR-0079 S4: a security control that can change an arithmetic result is a fork risk);
//! 6. the runner refuses a job that names a run manifest other than the one it was built with —
//!    a stale runner beside a new library cannot silently execute under someone else's ceiling;
//! 7. the runner takes no arguments and answers a malformed job with a frame, not a crash.

use kaspa_hashes::Hash64;
use misaka_palw::host_security::{ConfinementBackend, PALW_CONFINEMENT_ENV, confinement_backend_in_force};
use misaka_palw_derive::kinds::code::{
    CodeEvmTransformer, CodeGrammar, EvmJob, EvmJobCall, EvmJobResult, RUNNER_PATH_ENV, WORK_DIR_PREFIX, decode_evm_result,
    encode_evm_job, evm_v1_run_manifest_hash, read_mcod, refusal,
};
use misaka_palw_derive::{ClaimBinding, DeriveError, derive_with};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where cargo put the runner for this test run.
const RUNNER: &str = env!("CARGO_BIN_EXE_palw-evm-runner");

fn binding() -> ClaimBinding {
    ClaimBinding {
        network_domain: Hash64::from_bytes([0x01; 64]),
        claim_id: Hash64::from_bytes([0x02; 64]),
        output_root: Hash64::from_bytes([0x03; 64]),
        executor_pubkey: vec![0x11; 2592],
    }
}

/// The corpus's own sample, so this test and the golden test speak about the same answer.
fn answer() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("code").join("01-return-42.json");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn derive_answer() -> Result<Vec<u8>, DeriveError> {
    derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &answer()).map(|d| d.artifact.bytes)
}

/// Every ephemeral tree this process could have left behind.
fn stray_trees() -> Vec<PathBuf> {
    let prefix = format!("{WORK_DIR_PREFIX}evm-{}-", std::process::id());
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix)))
        .collect()
}

/// Hand the runner a frame and read what it answers.
fn ask_the_runner(frame: &[u8], args: &[&str]) -> (std::process::ExitStatus, Vec<u8>, String) {
    let mut child = std::process::Command::new(RUNNER)
        .args(args)
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the runner spawns");
    child.stdin.take().map(|mut pipe| pipe.write_all(frame));
    let out = child.wait_with_output().expect("the runner exits");
    (out.status, out.stdout, String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn the_evm_runs_in_a_separate_confined_process() {
    // A single-test binary: the only thread that touches the environment is this one.
    unsafe {
        std::env::remove_var(RUNNER_PATH_ENV);
        std::env::remove_var(PALW_CONFINEMENT_ENV);
    }

    // (1) Found beside the binary that derives — `target/debug/deps/<this test>` is one directory
    // below `target/debug/palw-evm-runner`, which is the same relation a deployment has.
    let unconfined = derive_answer().expect("the runner is beside this test binary");
    let mcod = read_mcod(&unconfined).expect("the artifact reads");
    assert_eq!(mcod.run_manifest, evm_v1_run_manifest_hash(), "the artifact names the ceiling and fixture it ran under");
    // The corpus sample carries one expectation that does NOT hold, on purpose: a failing test is
    // a verdict in the log, never a failed derivation (ADR-0078 Decision 10).
    assert_eq!((mcod.tests_passed, mcod.tests_failed), (1, 1), "01-return-42's second expectation is wrong on purpose");
    assert!(!mcod.runtime_code.is_empty());

    // (2) And named outright when a deployment puts it elsewhere.
    unsafe { std::env::set_var(RUNNER_PATH_ENV, RUNNER) };
    assert_eq!(derive_answer().expect("the named runner runs"), unconfined, "the same runner by either door");

    // (3) An absent runner REFUSES. There is no in-process fallback to fall back to: SA-1 says the
    // row does not ship without the cage, so the derivation produces no object.
    unsafe { std::env::set_var(RUNNER_PATH_ENV, "/nonexistent/palw-evm-runner") };
    match derive_answer() {
        Err(DeriveError::Transformer(msg)) => {
            assert!(msg.contains("is not a file"), "{msg}");
        }
        other => panic!("an absent runner must refuse, not fall back: {other:?}"),
    }
    unsafe { std::env::set_var(RUNNER_PATH_ENV, RUNNER) };

    // (4) No tree outlives its run — and the detector can see one, so the assertion is not vacuous.
    let decoy = std::env::temp_dir().join(format!("{WORK_DIR_PREFIX}evm-{}-decoy", std::process::id()));
    std::fs::create_dir(&decoy).expect("a decoy tree");
    assert_eq!(stray_trees(), vec![decoy.clone()], "the stray-tree detector sees a tree that is there");
    std::fs::remove_dir(&decoy).unwrap();
    assert!(stray_trees().is_empty(), "an ephemeral tree outlived its run: {:?}", stray_trees());

    // (5) ADR-0079 S4 — the backend cannot change the answer. On a host whose drill passes the
    // artifact is byte-identical to the unconfined one; on a host where it does not, the run says
    // so rather than pretending it tested something.
    unsafe { std::env::set_var(PALW_CONFINEMENT_ENV, "macos-sandbox-exec") };
    let confined = derive_answer();
    unsafe { std::env::remove_var(PALW_CONFINEMENT_ENV) };
    match confined {
        Ok(bytes) => assert_eq!(bytes, unconfined, "the confinement backend changed the artifact — that is a fork risk"),
        Err(e) => panic!("a requested backend that cannot install must degrade to `none`, not fail the job: {e}"),
    }
    // …and say which of the two it actually was, because "identical under `none` twice" is not the
    // claim. `establish_confinement` declares a backend only after its own drill has observed the
    // denials it promises (ADR-0079 S12), so this is the honest observation and not the request.
    match confinement_backend_in_force() {
        ConfinementBackend::None => eprintln!(
            "NOTE: this host's confinement drill did not pass, so the S4 comparison above was `none` against \
             `none`. The identity it asserts still holds; the backend half was not exercised here."
        ),
        backend => assert_eq!(backend, ConfinementBackend::MacosSandboxExec, "{}", backend.name()),
    }
    assert!(stray_trees().is_empty(), "an ephemeral tree outlived a confined run: {:?}", stray_trees());

    // (6) The runner's own pin: the job names a DIGEST, and a runner built with other ceilings
    // refuses rather than executing under them.
    let job = EvmJob {
        run_manifest: [0u8; 64],
        deploy_data: vec![0x60, 0x00, 0x60, 0x00, 0xf3],
        calls: vec![EvmJobCall { calldata: Vec::new(), value: 0, gas_limit: 21_000 }],
    };
    let (status, stdout, _) = ask_the_runner(&encode_evm_job(&job), &[]);
    assert!(status.success(), "a refusal is an answer, and an answer exits 0");
    let (manifest, result) = decode_evm_result(&stdout).expect("the runner answers with a frame");
    assert_eq!(manifest, evm_v1_run_manifest_hash());
    match result {
        EvmJobResult::Refused { code, detail, .. } => {
            assert_eq!(code, refusal::JOB_MALFORMED);
            let text = String::from_utf8_lossy(&detail);
            assert!(text.contains("run manifest") && text.contains("this runner was built with"), "{text}");
        }
        other => panic!("a job under another manifest must be refused: {other:?}"),
    }

    // (7) Garbage is answered with a frame; an argument is refused outright.
    let (status, stdout, _) = ask_the_runner(b"not a frame", &[]);
    assert!(status.success());
    let (_, result) = decode_evm_result(&stdout).expect("even garbage gets a frame");
    assert!(matches!(result, EvmJobResult::Refused { code, .. } if code == refusal::JOB_MALFORMED));
    let (status, _, stderr) = ask_the_runner(b"", &["--help"]);
    assert_eq!(status.code(), Some(2), "the runner takes no arguments");
    assert!(stderr.contains("takes no arguments"), "{stderr}");
}
