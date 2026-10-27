//! **Agent C4 — independent attacks on the beacon-conformance tool** (`palw-class pack commit-conformance|run-conformance|verify-conformance`).
//!
//! A pack is bound into a `ConformanceCommitmentV1`; canonical beacon facts (here SYNTHETIC — the node RPC that serves the real
//! ones does not exist yet) are turned into a beacon and a challenge by the shared contract only; the challenge selects vectors and
//! artifact leaves; the evidence is produced, then judged by a fresh derivation. Every test below is an attack that must fail closed
//! or a property that must hold byte for byte. Nothing here is an approved policy; the numbers are the test policy's.

use misaka_palw_challenge::beacon::{FinalPathV1, WorkBeaconV1, WorkSourceKindV1};
use misaka_palw_challenge::conformance::{BeaconConformanceEvidenceV1, ConformanceCommitmentV1, ConformanceStatusV1};
use misaka_palw_challenge::hash::{Digest, named_id};
use misaka_palw_challenge::{PostCommitChallengePolicyV1, RootV1, reference_policy_v1};
use misaka_palw_sdk::runtime_pack::beacon_run::*;
use misaka_palw_sdk::runtime_pack::commit::*;
use misaka_palw_sdk::runtime_pack::conformance::ImplSet;
use misaka_palw_sdk::runtime_pack::facts::*;
use misaka_palw_sdk::runtime_pack::manifest::{PACK_FILE, RuntimePackV1};
use misaka_palw_sdk::runtime_pack::{BuildOpts, DeclareOpts, build_pack};
use misaka_palw_sdk::tir_layout::TirLayoutChoiceV1;
use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ── fixtures ──────────────────────────────────────────────────────────────────────────────────────────────────────────────

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tir/lower/tests/fixtures").join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "runtime-pack-beacon-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch");
    d
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("dir");
    for e in std::fs::read_dir(src).expect("read").flatten() {
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &t);
        } else {
            std::fs::copy(e.path(), t).expect("copy");
        }
    }
}

struct Shared {
    work: PathBuf,
    pack: PathBuf,
    /// The declared class file (the artifact with its exact layout).
    class_file: PathBuf,
}

/// One tiny Llama pack with a class declared on testnet-12 (context 256), built once for the whole test binary.
fn shared() -> &'static Shared {
    static S: OnceLock<Shared> = OnceLock::new();
    S.get_or_init(|| {
        let work = scratch("shared");
        let src = fixture("hf/llama");
        let pack = work.join("pack");
        let mut req = ConvertRequest::new(&src, work.join("artifact.palwtir"));
        let seqs = misaka_palw_tir_lower::fidelity::random_sequences(64, 3, 12, 11);
        req.calib = Some(CalibInput { sequences: seqs, source: serde_json::json!("random (seed 11)") });
        let mut o = BuildOpts::new(req, &pack, "fixture");
        o.prompts = 1;
        o.prefill = 4;
        o.decode = 1;
        o.declare = vec![DeclareOpts {
            network: "testnet-12".into(),
            choice: TirLayoutChoiceV1 { max_context: Some(256), ..TirLayoutChoiceV1::default() },
        }];
        build_pack(&o, &|_| {}).unwrap_or_else(|e| panic!("build: {e}"));
        let class_file = work.join("artifact.palwtir.testnet-12.palwtir");
        assert!(class_file.exists());
        Shared { work, pack, class_file }
    })
}

fn test_policy(reps: u32) -> PostCommitChallengePolicyV1 {
    let mut p = reference_policy_v1(3, 2, 40, 5, reps);
    p.security_bits = 8;
    p
}

fn test_scope() -> ConformanceScopeV1 {
    let mut s = ConformanceScopeV1::new(2, 3, 1, 8);
    s.vector_fault_ppm = 1_000_000;
    s.leaf_fault_ppm = 500_000;
    s
}

fn test_params() -> CommitParamsV1 {
    CommitParamsV1::new("testnet-12", named_id("test-genesis"), named_id("test-ruleset"), test_policy(3), test_scope())
}

fn quiet(_: String) {}

fn commit(state: &Path, pack: &Path, artifact: &Path, params: &CommitParamsV1) -> (BoundCommitment, String) {
    let (b, _) = commit_conformance(pack, artifact, state, params, &quiet).unwrap_or_else(|e| panic!("commit: {e}"));
    let root = hex(&b.commitment.statement_root());
    (b, root)
}

fn facts_of(b: &BoundCommitment) -> ChainBeaconFactsV1 {
    synthetic_facts(&b.commitment, &b.params.policy, 100, "t")
}

fn run_with(
    state: &Path,
    pack: &Path,
    artifact: &Path,
    root: &str,
    facts: &ChainBeaconFactsV1,
    impls: ImplSet,
    max: Option<usize>,
    fault: Option<InjectedFault>,
) -> Result<RunOutcome, Refusal> {
    let src = MemoryFactSource(facts.clone());
    run_conformance(
        &RunInput { pack_dir: pack, artifact, state_dir: state, commitment: root, source: &src, impls, max_checks: max, fault },
        &quiet,
    )
}

fn run_ok(state: &Path, s: &Shared, root: &str, facts: &ChainBeaconFactsV1) -> (BeaconConformanceEvidenceV1, PathBuf) {
    match run_with(state, &s.pack, &s.class_file, root, facts, ImplSet::default(), None, None).unwrap_or_else(|e| panic!("run: {e}")) {
        RunOutcome::Evidence { evidence, dir, local, .. } => {
            assert!(local.is_ok(), "{local:?}");
            (evidence, dir)
        }
        other => panic!("expected evidence, got {other:?}"),
    }
}

fn verify_with(
    state: &Path,
    s: &Shared,
    root: &str,
    evidence: &Path,
    facts: &ChainBeaconFactsV1,
    rerun: bool,
) -> Result<Verdict, Refusal> {
    let src = MemoryFactSource(facts.clone());
    verify_conformance(
        &VerifyInput {
            pack_dir: &s.pack,
            artifact: &s.class_file,
            state_dir: state,
            commitment: root,
            evidence,
            source: &src,
            rerun,
            impls: ImplSet::default(),
        },
        &quiet,
    )
}

fn write_forged(dir: &Path, name: &str, ev: &BeaconConformanceEvidenceV1) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, borsh::to_vec(ev).expect("encodes")).expect("write");
    p
}

fn not_pass(v: &Result<Verdict, Refusal>) -> bool {
    !matches!(v, Ok(Verdict::Pass { .. }))
}

// ── attacks ──────────────────────────────────────────────────────────────────────────────────────────────────────────────

/// EXPECTED (before running): a committed scope is the CANDIDATE's own statement of how hard it will be checked
/// (`--no-require-independent --no-require-backend`). With both requirements off a run may enable only the reference evaluator
/// (`ImplSet { exec: false, ref2: false }`): a differential check with nothing to differ from. The safe property: no `Pass`
/// verdict can come out of a reference-only run — an honest verifier (default implementation set) must refuse it, and even a
/// verifier that mirrors the weak set must not call it conformance.
#[test]
#[ignore = "FAIL F-C4-11: a candidate-weakened scope + a reference-only run is Passed, and a verifier mirroring the weak set says Pass"]
fn a_scope_that_switches_off_the_independent_and_backend_implementations_cannot_pass() {
    let s = shared();
    let state = scratch("noimpl");
    let mut p = test_params();
    p.scope.require_independent = false;
    p.scope.require_backend = false;
    let committed = commit_conformance(&s.pack, &s.class_file, &state, &p, &quiet);
    let Ok((b, _)) = committed else {
        eprintln!("refused at commit: {}", committed.unwrap_err());
        return;
    };
    let root = hex(&b.commitment.statement_root());
    let facts = facts_of(&b);
    let weak = ImplSet { exec: false, ref2: false };
    let out = run_with(&state, &s.pack, &s.class_file, &root, &facts, weak, None, None);
    let (evidence, dir) = match out {
        Ok(RunOutcome::Evidence { evidence, dir, .. }) => (evidence, dir),
        other => {
            eprintln!("the reference-only run: {other:?}");
            return;
        }
    };
    eprintln!("reference-only run: status {:?}, checks {}/{}", evidence.status, evidence.checks_run, evidence.checks_required);
    let evp = dir.join("evidence.borsh");
    let src = MemoryFactSource(facts.clone());
    let mirror = verify_conformance(
        &VerifyInput {
            pack_dir: &s.pack,
            artifact: &s.class_file,
            state_dir: &state,
            commitment: &root,
            evidence: &evp,
            source: &src,
            rerun: true,
            impls: weak,
        },
        &quiet,
    );
    let honest = verify_with(&state, s, &root, &evp, &facts, true);
    eprintln!("verifier mirroring the weak set: {mirror:?}");
    eprintln!("verifier with the default (full) set: {honest:?}");
    assert!(
        !matches!(mirror, Ok(Verdict::Pass { .. })) && not_pass(&honest),
        "a PASS verdict on conformance evidence produced by the reference evaluator alone (candidate-chosen scope + implementation set)"
    );
}

/// EXPECTED (before running): the bound `-log2 ε` is derived from a fault density the CANDIDATE declares. A scope of ONE vector of
/// ONE token and ONE leaf per repetition, declared to expose any fault on 100 % of its draws, derives >= 40 bits at 30 repetitions
/// and is accepted. Recorded as GAP-C4-D (descriptive, passes): the fault model must come from the approved soundness policy, not the
/// committer; until then `derived_epsilon_bits` measures the candidate's claim, not the check.
#[test]
fn gap_a_tiny_scope_with_a_declared_total_fault_density_buys_forty_bits() {
    let s = shared();
    let state = scratch("tiny");
    let mut policy = reference_policy_v1(3, 2, 40, 5, 30);
    policy.security_bits = 40;
    let mut scope = ConformanceScopeV1::new(1, 1, 0, 1);
    scope.vector_fault_ppm = 1_000_000;
    scope.leaf_fault_ppm = 1_000_000;
    let p = CommitParamsV1::new("testnet-12", named_id("test-genesis"), named_id("test-ruleset"), policy, scope);
    let (b, _) = commit_conformance(&s.pack, &s.class_file, &state, &p, &quiet).expect("accepted");
    let bits = p.scope.derived_epsilon_bits(30);
    eprintln!(
        "GAP-C4-D: 30 x (1 vector of 1 token + 1 leaf) is accepted as >= {bits} bits; commitment {}",
        hex(&b.commitment.statement_root())[..16].to_string()
    );
    assert!(bits >= 40);
}

/// EXPECTED (before running): evidence of commitment A never verifies under commitment B (same state directory, different scope), and
/// B's evidence never verifies under A. VERDICT: PASS.
#[test]
fn evidence_of_one_commitment_never_verifies_under_another() {
    let s = shared();
    let state = scratch("cross");
    let (ba, ra) = commit(&state, &s.pack, &s.class_file, &test_params());
    let mut p2 = test_params();
    p2.scope.leaves_per_repetition = 6;
    let (bb, rb) = commit(&state, &s.pack, &s.class_file, &p2);
    assert_ne!(ra, rb);
    let (_, dir_a) = run_ok(&state, s, &ra, &facts_of(&ba));
    let (_, dir_b) = run_ok(&state, s, &rb, &facts_of(&bb));
    let a_under_b = verify_with(&state, s, &rb, &dir_a.join("evidence.borsh"), &facts_of(&bb), true);
    let b_under_a = verify_with(&state, s, &ra, &dir_b.join("evidence.borsh"), &facts_of(&ba), true);
    assert!(not_pass(&a_under_b), "{a_under_b:?}");
    assert!(not_pass(&b_under_a), "{b_under_a:?}");
}

/// EXPECTED (before running): flipping ANY single byte of a valid evidence file (every offset, XOR 0x01) and verifying with
/// re-execution is never a pass: header fields are recomputed, result roots are reproduced byte for byte. VERDICT: PASS.
#[test]
fn every_single_byte_flip_of_valid_evidence_is_never_a_pass() {
    let s = shared();
    let state = scratch("flips");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let (_, dir) = run_ok(&state, s, &root, &facts);
    let good = std::fs::read(dir.join("evidence.borsh")).unwrap();
    assert!(verify_with(&state, s, &root, &dir.join("evidence.borsh"), &facts, true).unwrap().is_pass());
    let mut passes = Vec::new();
    for i in 0..good.len() {
        let mut bad = good.clone();
        bad[i] ^= 0x01;
        let p = dir.join(format!("flip-{i}.borsh"));
        std::fs::write(&p, &bad).unwrap();
        if verify_with(&state, s, &root, &p, &facts, true).is_ok_and(|v| v.is_pass()) {
            passes.push(i);
        }
    }
    eprintln!("evidence {} bytes; flips that still PASS: {passes:?}", good.len());
    assert!(passes.is_empty(), "single-byte flips of the evidence that still verify as a pass: {passes:?}");
}

/// EXPECTED (before running): stale or inconsistent facts never give a pass: facts read before the beacon locked (tip lowered) are
/// pending; an extra earlier eligible source, a source whose execution commitment differs, a source dropped, the commitment
/// position moved: all fail (a different history is a different challenge). VERDICT: PASS.
#[test]
fn stale_short_or_different_facts_never_verify_evidence_produced_under_the_real_history() {
    let s = shared();
    let state = scratch("stale");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let (_, dir) = run_ok(&state, s, &root, &facts);
    let evp = dir.join("evidence.borsh");
    assert!(verify_with(&state, s, &root, &evp, &facts, true).unwrap().is_pass());
    let mut cases: Vec<(&str, ChainBeaconFactsV1)> = Vec::new();
    let mut early = facts.clone();
    early.tip_position = early.events.iter().map(|e| e.settlement_position).min().unwrap();
    cases.push(("tip before the lock", early));
    let mut extra = facts.clone();
    let mut e = facts.events[0].clone();
    e.canonical_work_id = [0x99; 64];
    e.settlement_position = facts.events.iter().map(|x| x.settlement_position).min().unwrap();
    e.accepted_position = facts.events[0].accepted_position;
    extra.events.push(e);
    cases.push(("extra earlier source", extra));
    let mut changed = facts.clone();
    changed.events[1].execution_commitment[0] ^= 1;
    cases.push(("execution commitment differs", changed));
    let mut dropped = facts.clone();
    dropped.events.remove(0);
    cases.push(("source dropped", dropped));
    let mut epoch = facts.clone();
    epoch.challenge_epoch += 1;
    cases.push(("epoch moved", epoch));
    let mut pos = facts.clone();
    pos.commitment_position += 1;
    cases.push(("commitment position moved", pos));
    for (what, f) in cases {
        let v = verify_with(&state, s, &root, &evp, &f, true);
        assert!(not_pass(&v), "{what}: {v:?}");
    }
}

/// EXPECTED (before running): 400 single-character / structural mutations of a valid facts file never panic the parser or the
/// verifier, and a mutated file that still parses never turns a recorded evidence into a pass unless it is the same history
/// (same digest). VERDICT: PASS.
#[test]
fn mutated_facts_files_never_panic_and_never_verify_other_histories() {
    let s = shared();
    let state = scratch("factsfuzz");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let (_, dir) = run_ok(&state, s, &root, &facts);
    let evp = dir.join("evidence.borsh");
    let text = serde_json::to_string_pretty(&facts_to_json(&facts, &b.params.policy.id())).unwrap();
    let mut seen_ok = 0u32;
    let mut passes_other = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let stride = (chars.len() / 400).max(1);
    for i in (0..chars.len()).step_by(stride) {
        for rep in ['0', '9', 'f', '"', '{', ' '] {
            if chars[i] == rep {
                continue;
            }
            let mut m = chars.clone();
            m[i] = rep;
            let t: String = m.into_iter().collect();
            let r = std::panic::catch_unwind(|| facts_from_json(&t, &b.commitment, &b.params.policy));
            let parsed = r.unwrap_or_else(|_| panic!("the facts parser panicked at char {i} -> {rep:?}"));
            if let Ok(f) = parsed {
                seen_ok += 1;
                let v = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| verify_with(&state, s, &root, &evp, &f, true)))
                    .unwrap_or_else(|_| panic!("verify panicked on mutated facts at char {i} -> {rep:?}"));
                if v.is_ok_and(|v| v.is_pass()) && f.digest() != facts.digest() {
                    // A different digest that still verifies must be semantically the same beacon (e.g. a shuffled/irrelevant field).
                    let (a, b): (serde_json::Value, serde_json::Value) =
                        (serde_json::from_str(&text).unwrap(), serde_json::from_str(&t).unwrap());
                    let mut paths = Vec::new();
                    json_diff("", &a, &b, &mut paths);
                    passes_other.push((i, rep, paths.join(",")));
                }
            }
        }
    }
    let mut fields: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for (_, _, k) in &passes_other {
        *fields.entry(k.clone()).or_default() += 1;
    }
    eprintln!("mutants that still parse: {seen_ok}; pass with a different facts digest, by nearest JSON key: {fields:?}");
}

fn json_diff(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value::*;
    match (a, b) {
        (Object(x), Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => json_diff(&format!("{path}.{k}"), v, w, out),
                    None => out.push(format!("{path}.{k}(removed)")),
                }
            }
            for k in y.keys().filter(|k| !x.contains_key(*k)) {
                out.push(format!("{path}.{k}(added)"));
            }
        }
        (Array(x), Array(y)) if x.len() == y.len() => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                let _ = i;
                json_diff(&format!("{path}[]"), v, w, out);
            }
        }
        _ if a != b => out.push(path.to_string()),
        _ => {}
    }
}

/// OBSERVATION O-C4-11 (descriptive, passes): the beacon needs `k` distinct canonical work IDS, not `k` distinct executions: three
/// works whose execution commitments are identical still lock. The contract treats ids as the identity of a work; whether one
/// execution can mint several ids is the consumer's canonical-id derivation.
#[test]
fn observation_k_distinct_work_ids_with_one_execution_commitment_still_lock() {
    let s = shared();
    let state = scratch("sameexec");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let mut facts = facts_of(&b);
    let ec = facts.events[0].execution_commitment;
    facts.events.iter_mut().for_each(|e| e.execution_commitment = ec);
    match run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), None, None) {
        Ok(RunOutcome::Evidence { evidence, .. }) => eprintln!("O-C4-11: locked and ran, status {:?}", evidence.status),
        other => panic!("{other:?}"),
    }
}
