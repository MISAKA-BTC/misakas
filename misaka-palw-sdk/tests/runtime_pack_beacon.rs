//! **Beacon conformance of a runtime pack, attack by attack** (RFC-0013 §9, RFC-0007 Part VI).
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(rel)
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

// ── the commitment ────────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_commitment_binds_every_root_from_the_artifact_and_any_change_is_a_new_commitment() {
    let s = shared();
    let state = scratch("commit");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let c = &b.commitment;
    // Every root is real and the commitment is the contract's statement root, not JSON.
    let zero = [0u8; 64];
    for (name, d) in [
        ("artifact_root", c.artifact_root),
        ("program_root", c.program_root),
        ("layout_root", c.layout_root),
        ("verification_plan_root", c.verification_plan_root),
        ("kernel_descriptor_id", c.kernel_descriptor_id),
        ("implementation_set_root", c.implementation_set_root),
        ("test_scope_root", c.test_scope_root),
        ("challenge_policy_id", c.challenge_policy_id),
        ("candidate_id", c.candidate_id),
        ("resource_profile_id", c.resource_profile_id),
    ] {
        assert_ne!(d, zero, "{name}");
    }
    assert!(matches!(c.tokenizer_or_input_schema_root, RootV1::Present(_)));
    assert!(matches!(c.calibration_id, RootV1::Present(_)));
    assert!(matches!(c.input_and_state_binding_root, RootV1::Present(_)));
    assert_eq!(c.constraint_root, RootV1::Absent, "typed absence, never a zero wildcard");
    assert_eq!((c.commitment_object_id, c.canonical_commitment_position), (None, None), "provenance is the chain's to fill");
    assert_eq!(root, hex(&c.statement_root()));
    assert!(b.admission.hypothetically_armed, "the kernel is judged armed; the record says so");
    assert_eq!(b.admission.shipped_outcome, "KERNEL_NOT_ACTIVE", "no kernel is Active in the shipped schedule");
    // The same inputs give the same commitment (a pure function of pack, artifact and parameters).
    let state2 = scratch("commit2");
    let (b2, _) = commit(&state2, &s.pack, &s.class_file, &test_params());
    assert_eq!(b2.commitment, b.commitment);

    // Each parameter the commitment fixes moves the statement root, and moves the field it should.
    let mut seen = std::collections::BTreeSet::new();
    seen.insert(c.statement_root());
    let mut vary = |what: &str, edit: &dyn Fn(&mut CommitParamsV1), field: &str| {
        let mut p = test_params();
        edit(&mut p);
        let st = scratch("vary");
        let (v, _) = commit(&st, &s.pack, &s.class_file, &p);
        let diff = commitment_diff(&v.commitment, &b.commitment);
        assert!(diff.contains(&field), "{what}: expected {field} among {diff:?}");
        assert!(seen.insert(v.commitment.statement_root()), "{what}: not a new commitment");
    };
    vary(
        "policy k",
        &|p| {
            p.policy = {
                let mut x = test_policy(3);
                x.work_count_k = 4;
                x
            }
        },
        "challenge_policy_id",
    );
    vary(
        "policy delay",
        &|p| {
            p.policy = {
                let mut x = test_policy(3);
                x.anchor_delay_slots = 3;
                x
            }
        },
        "challenge_policy_id",
    );
    vary("repetitions", &|p| p.policy = test_policy(4), "challenge_policy_id");
    vary("scope vectors", &|p| p.scope.vectors_per_repetition = 3, "test_scope_root");
    vary("scope fault model", &|p| p.scope.leaf_fault_ppm = 400_000, "test_scope_root");
    vary("scope decode tokens", &|p| p.scope.decode_tokens += 1, "test_scope_root");
    vary("chain", &|p| p.chain_genesis = named_id("another-genesis"), "chain_genesis");
    vary("ruleset", &|p| p.ruleset_id = named_id("another-ruleset"), "ruleset_id");
    vary("candidate", &|p| p.candidate_id = Some(named_id("another-candidate")), "candidate_id");
    vary("plan positions", &|p| p.plan_positions = Some(8), "verification_plan_root");
}

#[test]
fn a_changed_layout_is_a_different_class_and_a_different_commitment_and_a_stale_one_is_refused() {
    use misaka_palw_sdk::runtime_pack::bind::bind_class;
    use misaka_palw_sdk::tir_layout::tir_declare_layout_v1;
    let s = shared();
    let work = scratch("layout");
    let params = misaka_palw_sdk::runtime_pack::build::network("testnet-12").unwrap();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("network")
    };
    let class_b = work.join("class-b.palwtir");
    let choice = TirLayoutChoiceV1 { max_context: Some(128), tile_len: 16, h_chunk: 8, logits_tile: Some(32), ..Default::default() };
    tir_declare_layout_v1(&params, bundle, &s.work.join("artifact.palwtir"), &class_b, &choice, None).expect("a second layout");
    let pack2 = work.join("pack2");
    bind_class(&s.pack, &class_b, "testnet-12", &pack2).expect("both layouts are now declared");
    let pack_a_prefix = RuntimePackV1::parse(&std::fs::read_to_string(pack2.join(PACK_FILE)).unwrap())
        .unwrap()
        .declared
        .iter()
        .map(|d| d.class_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(pack_a_prefix.len(), 2);
    // Without a class prefix the network is ambiguous: refused, not guessed.
    let state = scratch("layout-state");
    let e = commit_conformance(&pack2, &s.class_file, &state, &test_params(), &quiet).unwrap_err();
    assert_eq!(e.code, "AMBIGUOUS_CLASS", "{e}");
    // Each layout is its own commitment: the layout root, the candidate and the plan differ; the artifact root is the same program's.
    let mut pa = test_params();
    pa.class_id_prefix = Some(pack_a_prefix[0][..16].to_string());
    let mut pb = test_params();
    pb.class_id_prefix = Some(pack_a_prefix[1][..16].to_string());
    let (cls_a, cls_b) =
        if pack_a_prefix[0] == class_id_of(&s.class_file) { (&s.class_file, &class_b) } else { (&class_b, &s.class_file) };
    let (a, root_a) = commit(&state, &pack2, cls_a, &pa);
    let (b, _) = commit(&state, &pack2, cls_b, &pb);
    let diff = commitment_diff(&a.commitment, &b.commitment);
    assert!(diff.contains(&"layout_root") && diff.contains(&"candidate_id"), "{diff:?}");
    assert_eq!(a.commitment.artifact_root, b.commitment.artifact_root, "one program, one inventory root, two classes");
    // A run for commitment A with class B's file is stale: the layout moved after the commitment.
    let facts = facts_of(&a);
    let r = run_with(&state, &pack2, cls_b, &root_a, &facts, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(r.code, "COMMITMENT_STALE", "{r}");
    let _ = std::fs::remove_dir_all(work);
}

fn class_id_of(class_file: &Path) -> String {
    misaka_palw_sdk::tir_manifest::PalwTirManifestV1::derive_streamed(class_file)
        .expect("manifest")
        .class_id
        .expect("a class")
        .to_string()
}

#[test]
fn a_pack_without_an_exact_layout_has_no_commitment_and_a_scope_the_policy_cannot_use_is_refused_before_the_beacon() {
    let s = shared();
    // No declared class: LAYOUT_REQUIRED, and no state is written.
    let work = scratch("nolayout");
    let bare = work.join("bare");
    copy_dir(&s.pack, &bare);
    let mut p = RuntimePackV1::parse(&std::fs::read_to_string(bare.join(PACK_FILE)).unwrap()).unwrap();
    p.declared.clear();
    std::fs::write(bare.join(PACK_FILE), p.to_pretty()).unwrap();
    let state = work.join("state");
    let e = commit_conformance(&bare, &s.class_file, &state, &test_params(), &quiet).unwrap_err();
    assert_eq!(e.code, "LAYOUT_REQUIRED", "{e}");
    assert!(!state.exists(), "a refused commitment leaves nothing behind");
    // A scope that derives fewer bits than the policy asks is refused before any randomness exists.
    let mut weak = test_params();
    weak.policy.security_bits = 60;
    let e = commit_conformance(&s.pack, &s.class_file, &state, &weak, &quiet).unwrap_err();
    assert_eq!(e.code, "SCOPE_CANNOT_MEET_POLICY", "{e}");
    // A leaf draw larger than the artifact, and an empty scope, are refused.
    let mut big = test_params();
    big.scope.leaves_per_repetition = 1_000_000;
    assert_eq!(commit_conformance(&s.pack, &s.class_file, &state, &big, &quiet).unwrap_err().code, "SCOPE_INVALID");
    let mut empty = test_params();
    empty.scope.vectors_per_repetition = 0;
    empty.scope.leaves_per_repetition = 0;
    assert_eq!(commit_conformance(&s.pack, &s.class_file, &state, &empty, &quiet).unwrap_err().code, "SCOPE_INVALID");
    // An invalid policy never reaches the artifact.
    let mut bad = test_params();
    bad.policy.sampling_algorithm_id = named_id("someone-elses-sampler");
    assert_eq!(commit_conformance(&s.pack, &s.class_file, &state, &bad, &quiet).unwrap_err().code, "POLICY_INVALID");
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn static_admission_comes_first_a_program_no_kernel_expresses_has_no_commitment() {
    // The sketch fixture whose ranges are not proven is the FRONTEND's to narrow under every reference kernel.
    let fx = misaka_palw_tir_sketch::fixture::wide_v1(1);
    let root = misaka_palw_challenge::hash::h(b"test-program-root", &fx.program.encode());
    let e = static_admission(&fx.program, root, 64).unwrap_err();
    assert_eq!(e.code, "FRONTEND_REQUIRED", "{e}");
    // And a program a kernel can express yields a plan root, hypothetically armed.
    let ok = misaka_palw_tir_sketch::fixture::dense_moe_v1(1);
    let a = static_admission(&ok.program, root, 64).expect("admitted when armed");
    assert!(a.hypothetically_armed);
    assert_eq!(a.shipped_outcome, "KERNEL_NOT_ACTIVE");
}

#[test]
fn the_derived_epsilon_is_an_integer_conditional_bound_and_never_zero_for_a_scope_that_checks_something() {
    let mut s = ConformanceScopeV1::new(14, 4, 2, 512);
    // 4 reps x 14 vectors at 50 % fault density, 4 x 512 leaves at 6.25 %: min(40, 184).
    assert_eq!(s.derived_epsilon_bits(4), 40);
    s.vectors_per_repetition = 1;
    assert_eq!(s.derived_epsilon_bits(4), 2, "four vector draws at 50 %: the bound is honest, not a target");
    s.vectors_per_repetition = 0;
    assert_eq!(s.derived_epsilon_bits(4), 184);
    s.leaves_per_repetition = 0;
    let policy = reference_policy_v1(3, 2, 40, 5, 4);
    assert!(s.validate(&policy).is_err(), "a scope that checks nothing is refused");
    // C4 F-C4-11 / GAP-C4-D: the candidate cannot waive an implementation, and the fault model is the soundness policy's.
    let mut weak = ConformanceScopeV1::new(14, 4, 2, 512);
    weak.validate(&policy).unwrap();
    weak.require_independent = false;
    assert!(weak.validate(&policy).is_err(), "a reference-only scope is refused");
    let mut weak = ConformanceScopeV1::new(14, 4, 2, 512);
    weak.require_backend = false;
    assert!(weak.validate(&policy).is_err(), "a scope without the backend is refused");
    let mut reviewed = policy.clone();
    reviewed.soundness_policy_id = misaka_palw_challenge::hash::named_id("soundness/some-reviewed-policy/v1");
    assert!(ConformanceScopeV1::new(14, 4, 2, 512).validate(&reviewed).is_err(), "no fault model is approved for it yet");
    assert!(s.statement(4).contains("not whole-model fidelity") && s.statement(4).contains("not full-scope"));
}

// ── beacon states, resume and retries ─────────────────────────────────────────────────────────────────────────────────────

#[test]
fn waiting_unavailable_and_retries_are_pending_states_never_a_pass_and_never_a_fallback() {
    let s = shared();
    let state = scratch("states");
    let mut params = test_params();
    params.policy.retry_limit = 1;
    params.policy.security_bits = 8;
    let (b, root) = commit(&state, &s.pack, &s.class_file, &params);
    let full = facts_of(&b);
    // Two of three works: collecting.
    let mut two = full.clone();
    two.events.truncate(2);
    two.tip_position = 130;
    match run_with(&state, &s.pack, &s.class_file, &root, &two, ImplSet::default(), None, None).unwrap() {
        RunOutcome::Waiting { have: 2, need: 3, lock_position: None, .. } => {}
        o => panic!("{o:?}"),
    }
    // Three works but the last is not yet at depth D: a candidate, still waiting.
    let mut early = full.clone();
    early.tip_position -= 1;
    match run_with(&state, &s.pack, &s.class_file, &root, &early, ImplSet::default(), None, None).unwrap() {
        RunOutcome::Waiting { have: 3, lock_position: Some(_), .. } => {}
        o => panic!("{o:?}"),
    }
    // Nothing was run: no seed directory exists.
    let cdir = state.join(commitment_dirname(&b.commitment.statement_root()));
    assert!(std::fs::read_dir(&cdir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with("seed-")));
    // The window closes short: BEACON_UNAVAILABLE, counted, no evidence, no fallback.
    let mut closed = two.clone();
    closed.tip_position = 142;
    match run_with(&state, &s.pack, &s.class_file, &root, &closed, ImplSet::default(), None, None).unwrap() {
        RunOutcome::Unavailable { have: 2, need: 3, retries: 1, retry_limit: 1, .. } => {}
        o => panic!("{o:?}"),
    }
    assert!(std::fs::read_dir(&cdir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with("seed-")));
    // The first retry is a NEW commitment (and window) and is allowed; the next is refused once the limit is spent.
    let mut p2 = params.clone();
    p2.policy.anchor_delay_slots = 3;
    let (b2, root2) = commit(&state, &s.pack, &s.class_file, &p2);
    let mut closed2 = facts_of(&b2);
    closed2.events.truncate(1);
    closed2.tip_position = 200;
    assert!(matches!(
        run_with(&state, &s.pack, &s.class_file, &root2, &closed2, ImplSet::default(), None, None).unwrap(),
        RunOutcome::Unavailable { retries: 2, .. }
    ));
    let mut p3 = params.clone();
    p3.policy.anchor_delay_slots = 4;
    let e = commit_conformance(&s.pack, &s.class_file, &state, &p3, &quiet).unwrap_err();
    assert_eq!(e.code, "RETRY_LIMIT_EXHAUSTED", "{e}");
    // The first commitment still locks on a history that does have the works: availability is a function of the facts alone.
    let (ev, _) = run_ok(&state, s, &root, &full);
    assert_eq!(ev.status, ConformanceStatusV1::Passed);
    // The ledger kept every observation, in order.
    let ledger = std::fs::read_to_string(ledger_path(&state)).unwrap();
    for needle in ["COMMITTED", "WAITING_RANDOMNESS", "BEACON_UNAVAILABLE", "CHALLENGE_RESOLVED", "EVIDENCE"] {
        assert!(ledger.contains(needle), "{needle}");
    }
}

#[test]
fn beacon_sources_that_are_not_fresh_final_useful_independent_work_never_lock() {
    let s = shared();
    let state = scratch("sources");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let honest = facts_of(&b);
    let never_locks = |what: &str, facts: ChainBeaconFactsV1| {
        let o = run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), None, None);
        assert!(matches!(o, Ok(RunOutcome::Waiting { .. }) | Ok(RunOutcome::Unavailable { .. })), "{what}: {o:?}");
    };
    // Heartbeat, BASE-0, EXEC and receipt noise is never entropy, however much of it there is.
    for kind in [
        WorkSourceKindV1::Heartbeat,
        WorkSourceKindV1::Base0Fallback,
        WorkSourceKindV1::ExecTx,
        WorkSourceKindV1::ExecWorkSlice,
        WorkSourceKindV1::ReceiptOnly,
        WorkSourceKindV1::ProvisionalAttempt,
        WorkSourceKindV1::PanelReceipt,
    ] {
        let mut f = honest.clone();
        f.events.iter_mut().for_each(|e| e.kind = kind);
        f.tip_position = 300;
        never_locks(&format!("{kind:?}"), f);
    }
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.claim_final = false);
    never_locks("unfinalized work", f);
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.da_satisfied = false);
    never_locks("DA unsatisfied", f);
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.validity_independent = false);
    never_locks("validity depends on the challenge", f);
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.accepted_position = 50);
    never_locks("committed before S, Final after it", f);
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.settlement_position += 1000);
    f.tip_position += 2000;
    never_locks("settled outside the window", f);
    // One work repeated is one work, however many times it is reattached.
    let mut f = honest.clone();
    let first = f.events[0].clone();
    f.events = (0..9)
        .map(|i| {
            let mut e = first.clone();
            e.occurrence_index = i;
            e.settlement_position += i as u64;
            e
        })
        .collect();
    f.tip_position = 300;
    never_locks("duplicate work contributions", f);
    // The candidate under test is never a source: its own works are excluded by the context.
    let cand = b.commitment.candidate_id;
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.source_profile_id = cand);
    f.tip_position = 300;
    never_locks("candidate self-beacon", f);
    let mut f = honest.clone();
    f.events.iter_mut().for_each(|e| e.depends_on_profiles = vec![cand]);
    f.tip_position = 300;
    never_locks("work depending on the candidate", f);
    // A facts source that does not exclude the candidate, or lists it as eligible, is refused outright.
    let mut f = honest.clone();
    f.excluded_profiles.clear();
    let e = run_with(&state, &s.pack, &s.class_file, &root, &f, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(e.code, "FACTS_MISSING_SELF_EXCLUSION");
    let mut f = honest.clone();
    f.eligible_profiles.insert(cand);
    let e = run_with(&state, &s.pack, &s.class_file, &root, &f, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(e.code, "FACTS_SELF_ELIGIBLE");
    // A source profile that was not Active and G14-complete at the commitment is not eligible.
    let mut f = honest.clone();
    f.eligible_profiles.clear();
    f.eligible_profiles.insert(named_id("some-other-profile"));
    f.tip_position = 300;
    never_locks("profile not eligible at the commitment", f);
}

#[test]
fn a_facts_file_that_names_another_policy_is_refused_and_a_substituted_policy_in_the_state_is_refused() {
    let s = shared();
    let state = scratch("policy");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    // The facts file names the committed policy; another id is a substitution.
    let good = facts_to_json(&facts, &b.params.policy.id());
    let path = state.join("facts.json");
    std::fs::write(&path, good.to_string()).unwrap();
    let src = FileFactSource(path.clone());
    let loaded = src.facts(&b.commitment, &b.params.policy).expect("the committed policy is accepted");
    assert_eq!(loaded, facts, "the file form round-trips");
    let other = reference_policy_v1(2, 2, 40, 5, 3);
    std::fs::write(&path, facts_to_json(&facts, &other.id()).to_string()).unwrap();
    assert_eq!(FileFactSource(path).facts(&b.commitment, &b.params.policy).unwrap_err().code, "POLICY_SUBSTITUTED");
    // How each work reached Final travels with the facts: Panel-licensed (today's normal case) and Panel-independent both round-trip,
    // and a file that omits the path is malformed rather than defaulted.
    let mut mixed = facts.clone();
    mixed.events[1].final_path = FinalPathV1::PanelIndependent;
    assert!(matches!(mixed.events[0].final_path, FinalPathV1::PanelLicensed { .. }));
    let mixed_path = state.join("facts-mixed.json");
    std::fs::write(&mixed_path, facts_to_json(&mixed, &b.params.policy.id()).to_string()).unwrap();
    assert_eq!(FileFactSource(mixed_path).facts(&b.commitment, &b.params.policy).unwrap(), mixed);
    let mut stripped = facts_to_json(&facts, &b.params.policy.id());
    stripped["events"][0].as_object_mut().unwrap().remove("final_path");
    let stripped_path = state.join("facts-stripped.json");
    std::fs::write(&stripped_path, stripped.to_string()).unwrap();
    assert_eq!(FileFactSource(stripped_path).facts(&b.commitment, &b.params.policy).unwrap_err().code, "FACTS_MALFORMED");
    // A different valid policy dropped into the state directory is not the one the statement names.
    let dir = state.join(commitment_dirname(&b.commitment.statement_root()));
    let mut weaker = b.params.policy.clone();
    weaker.work_count_k = 1;
    std::fs::write(dir.join("policy.borsh"), borsh::to_vec(&weaker).unwrap()).unwrap();
    let r = run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(r.code, "POLICY_SUBSTITUTED", "{r}");
}

// ── end to end ────────────────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn commit_beacon_evidence_verify_passes_reproduces_byte_for_byte_and_the_verifier_needs_no_producer_state() {
    let s = shared();
    let state = scratch("e2e");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let (ev, dir) = run_ok(&state, s, &root, &facts);
    assert_eq!(ev.status, ConformanceStatusV1::Passed);
    assert_eq!((ev.checks_required, ev.checks_run, ev.checks_failed), (3 * (2 + 8), 3 * (2 + 8), 0));
    assert!(ev.missing_checks.is_empty() && ev.failures.is_empty());
    assert_eq!(ev.derived_epsilon_bits, 8);
    assert_eq!(ev.transcript_root, RootV1::Absent);
    for (n, d) in [
        ("reference", ev.reference_result_root),
        ("independent", ev.independent_result_root),
        ("backend", ev.backend_result_root),
        ("openings", ev.authenticated_openings_root),
        ("vectors", ev.selected_vectors_root),
        ("ranges", ev.selected_tensor_ranges_root),
        ("sources", ev.qualifying_source_evidence_root),
        ("locators", ev.public_material_locator_root),
    ] {
        assert_ne!(d, [0u8; 64], "{n}");
    }
    // Three different implementations: their result roots are over their own results and are distinct objects.
    assert_ne!(ev.reference_result_root, ev.independent_result_root);

    // A fresh verification recomputes the beacon, seed, selections and re-executes every check.
    let evp = dir.join("evidence.borsh");
    match verify_with(&state, s, &root, &evp, &facts, true).unwrap() {
        Verdict::Pass { evidence_id, provenance, .. } => {
            assert_eq!(evidence_id, ev.id());
            assert!(provenance.is_synthetic(), "the verdict must say these facts are synthetic");
        }
        v => panic!("{v:?}"),
    }
    // Without re-execution the result roots are unverified: never a pass.
    assert!(matches!(verify_with(&state, s, &root, &evp, &facts, false).unwrap(), Verdict::NotReproduced { .. }));

    // Same pack + same canonical history => the same challenge => byte-identical evidence, in another state directory, on another run.
    let state2 = scratch("e2e2");
    let (_, root2) = commit(&state2, &s.pack, &s.class_file, &test_params());
    assert_eq!(root, root2);
    let (_, dir2) = run_ok(&state2, s, &root2, &facts);
    assert_eq!(std::fs::read(dir.join("evidence.borsh")).unwrap(), std::fs::read(dir2.join("evidence.borsh")).unwrap());
    // Re-running in the same state directory reuses the completion records and gives the same bytes.
    let again = match run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), None, None).unwrap() {
        RunOutcome::Evidence { evidence, measures, .. } => {
            assert_eq!(measures.checks_executed, 0, "every check was already recorded");
            assert_eq!(measures.checks_reused, ev.checks_required);
            evidence
        }
        o => panic!("{o:?}"),
    };
    assert_eq!(again, ev);
    // Arrival order is not an input: a shuffled history gives the same evidence.
    let mut shuffled = facts.clone();
    shuffled.events.reverse();
    let state3 = scratch("e2e3");
    let (_, root3) = commit(&state3, &s.pack, &s.class_file, &test_params());
    let (_, dir3) = run_ok(&state3, s, &root3, &shuffled);
    assert_eq!(std::fs::read(dir.join("evidence.borsh")).unwrap(), std::fs::read(dir3.join("evidence.borsh")).unwrap());
    // A MODEL_CONFORMANCE beacon accepts either Final path: all-Panel-independent sources lock too (a different history, a different seed).
    let mut independent = facts.clone();
    independent.events.iter_mut().for_each(|e| e.final_path = FinalPathV1::PanelIndependent);
    let state5 = scratch("e2e5");
    let (_, root5) = commit(&state5, &s.pack, &s.class_file, &test_params());
    let (ev5, _) = run_ok(&state5, s, &root5, &independent);
    assert_eq!(ev5.status, ConformanceStatusV1::Passed);
    // A later tip with more history gives the same beacon, hence the same evidence.
    let mut later = facts.clone();
    later.tip_position += 50;
    let state4 = scratch("e2e4");
    let (_, root4) = commit(&state4, &s.pack, &s.class_file, &test_params());
    let (_, dir4) = run_ok(&state4, s, &root4, &later);
    assert_eq!(std::fs::read(dir.join("evidence.borsh")).unwrap(), std::fs::read(dir4.join("evidence.borsh")).unwrap());
}

/// **RFC-0013 §7.2: a stored Merkle index changes what the openings cost and nothing the evidence says.** With `<artifact>.merkleidx` beside the
/// artifact, the drawn leaves are opened by reading those leaves only (the index folds to the committed root, the multiproof is built from the
/// stored hashes); the evidence is byte for byte the streamed pass's. A sidecar that is damaged, or that is another artifact's, is not believed:
/// the run makes the streamed pass it always made and ends in the same bytes.
#[test]
fn a_stored_merkle_index_changes_what_the_openings_cost_and_nothing_the_evidence_says() {
    use misaka_palw_sdk::tir_merkle_index::PalwTirMerkleIndexV1;
    use misaka_palw_sdk::tir_stream::ContainerRanges;
    let s = shared();
    let work = scratch("merkle-index");
    let copy = work.join("class.palwtir");
    std::fs::copy(&s.class_file, &copy).expect("a private copy of the class file");
    let run = |tag: &str| {
        let state = scratch(tag);
        let (b, root) = commit(&state, &s.pack, &copy, &test_params());
        let facts = facts_of(&b);
        match run_with(&state, &s.pack, &copy, &root, &facts, ImplSet::default(), None, None).unwrap_or_else(|e| panic!("run: {e}")) {
            RunOutcome::Evidence { evidence, dir, measures, .. } => {
                (evidence, std::fs::read(dir.join("evidence.borsh")).unwrap(), measures)
            }
            other => panic!("expected evidence, got {other:?}"),
        }
    };
    let artifact_bytes = std::fs::metadata(&copy).unwrap().len();

    // No sidecar: the streamed pass reads the whole artifact.
    let (ev_pass, bytes_pass, m_pass) = run("mi-pass");
    assert_eq!(ev_pass.status, ConformanceStatusV1::Passed);
    assert!(!m_pass.open_via_index && m_pass.open_pass_hashed_bytes >= artifact_bytes / 2, "{m_pass:?}");

    // The sidecar: the same evidence, opened by reading the drawn leaves only.
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(&copy).unwrap();
    let index = PalwTirMerkleIndexV1::build_streamed(&container.program, &ContainerRanges::open(&container).unwrap()).unwrap();
    let sidecar = merkle_index_path(&copy);
    index.write(&sidecar).expect("sidecar");
    let (ev_idx, bytes_idx, m_idx) = run("mi-indexed");
    assert!(m_idx.open_via_index, "{m_idx:?}");
    assert_eq!(bytes_idx, bytes_pass, "the evidence is the streamed pass's, byte for byte");
    assert_eq!(ev_idx, ev_pass);
    assert!(
        m_idx.open_pass_hashed_bytes > 0 && m_idx.open_pass_hashed_bytes < m_pass.open_pass_hashed_bytes,
        "{} bytes hashed of an artifact of {artifact_bytes} ({} for the pass): the drawn leaves, not the artifact",
        m_idx.open_pass_hashed_bytes,
        m_pass.open_pass_hashed_bytes
    );

    // A damaged sidecar (a flipped bit in a stored leaf) and another artifact's index are refused, not believed: the pass runs, the evidence stands.
    let good = std::fs::read(&sidecar).unwrap();
    let mut damaged = good.clone();
    damaged[good.len() / 2] ^= 1;
    std::fs::write(&sidecar, &damaged).unwrap();
    let (_, bytes_damaged, m_damaged) = run("mi-damaged");
    assert!(!m_damaged.open_via_index, "{m_damaged:?}");
    assert_eq!(bytes_damaged, bytes_pass);
    let other = {
        let mut leaves = index.leaves().to_vec();
        leaves[0] = kaspa_hashes::Hash64::from_u64_word(0xBAD);
        PalwTirMerkleIndexV1::from_leaves(&container.program, leaves).unwrap()
    };
    other.write(&sidecar).unwrap();
    let (_, bytes_other, m_other) = run("mi-other");
    assert!(!m_other.open_via_index, "an index that folds to another root is not used");
    assert_eq!(bytes_other, bytes_pass);
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn an_interrupted_run_resumes_from_its_records_and_ends_with_the_same_evidence() {
    let s = shared();
    let state = scratch("resume");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let total = 3 * (2 + 8);
    // The process stops after four checks: honest INCOMPLETE state, nothing written as evidence.
    match run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), Some(4), None).unwrap() {
        RunOutcome::Interrupted { done: 4, total: t, .. } => assert_eq!(t, total as usize),
        o => panic!("{o:?}"),
    }
    let cdir = state.join(commitment_dirname(&b.commitment.statement_root()));
    let seed_dir =
        std::fs::read_dir(&cdir).unwrap().flatten().find(|e| e.file_name().to_string_lossy().starts_with("seed-")).unwrap().path();
    assert!(!seed_dir.join("evidence.borsh").exists());
    assert_eq!(std::fs::read_dir(seed_dir.join("checks")).unwrap().count(), 4, "one atomic completion record per finished check");
    // A damaged record is not trusted: its own digest does not verify, so the check is run again.
    let victim = std::fs::read_dir(seed_dir.join("checks")).unwrap().flatten().next().unwrap().path();
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&victim).unwrap()).unwrap();
    v["outcome_digest"] = "0".repeat(128).into();
    std::fs::write(&victim, v.to_string()).unwrap();
    // A fresh process resumes: three records are reused, the damaged one is re-run, the rest are run.
    let resumed = match run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), None, None).unwrap() {
        RunOutcome::Evidence { evidence, dir, measures, .. } => {
            assert_eq!(measures.checks_reused, 3);
            assert_eq!(measures.checks_executed, total - 3);
            assert_eq!(evidence.status, ConformanceStatusV1::Passed);
            std::fs::read(dir.join("evidence.borsh")).unwrap()
        }
        o => panic!("{o:?}"),
    };
    // ... and it is exactly the evidence an uninterrupted run gives.
    let straight = scratch("resume-straight");
    let (_, root2) = commit(&straight, &s.pack, &s.class_file, &test_params());
    let (_, dir2) = run_ok(&straight, s, &root2, &facts);
    assert_eq!(resumed, std::fs::read(dir2.join("evidence.borsh")).unwrap());
    // Enabling an executor that an earlier record skipped re-runs that record (the binding includes the enabled executors).
    let skipped = scratch("resume-skip");
    let (_, root3) = commit(&skipped, &s.pack, &s.class_file, &test_params());
    let no_ref2 = ImplSet { exec: true, ref2: false };
    match run_with(&skipped, &s.pack, &s.class_file, &root3, &facts, no_ref2, None, None).unwrap() {
        RunOutcome::Evidence { evidence, local, .. } => {
            assert_eq!(evidence.status, ConformanceStatusV1::Skipped);
            assert!(local.is_err(), "a skipped check is never a pass");
        }
        o => panic!("{o:?}"),
    }
    match run_with(&skipped, &s.pack, &s.class_file, &root3, &facts, ImplSet::default(), None, None).unwrap() {
        RunOutcome::Evidence { evidence, measures, .. } => {
            assert_eq!(evidence.status, ConformanceStatusV1::Passed);
            assert_eq!(measures.checks_reused, 0, "nothing recorded under the skipped binding is reused");
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn a_reorg_is_a_different_challenge_the_old_evidence_is_retained_and_invalidated_never_reused() {
    let s = shared();
    let state = scratch("reorg");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts_a = facts_of(&b);
    let (ev_a, dir_a) = run_ok(&state, s, &root, &facts_a);
    // The branch changes before the evidence is relied on: one source work is replaced (another canonical work identity).
    let mut facts_b = facts_a.clone();
    facts_b.events[1].canonical_work_id = named_id("replacement-work");
    facts_b.events[1].execution_commitment = named_id("replacement-exec");
    let (ev_b, dir_b) = run_ok(&state, s, &root, &facts_b);
    assert_ne!(ev_a.challenge_seed, ev_b.challenge_seed, "another history, another seed");
    assert_ne!(ev_a.beacon_output, ev_b.beacon_output);
    assert_ne!(ev_a.selected_vectors_root, ev_b.selected_vectors_root, "the selections follow the seed");
    assert_ne!(dir_a, dir_b);
    // The old evidence is retained and marked; the ledger says why.
    assert!(dir_a.join("evidence.borsh").exists() && dir_a.join("INVALIDATED").exists() && !dir_b.join("INVALIDATED").exists());
    assert!(std::fs::read_to_string(ledger_path(&state)).unwrap().contains("INVALIDATED"));
    // Under the new history the old evidence's seed is stale; under its own history it still verifies.
    let old = dir_a.join("evidence.borsh");
    // The beacon presented beside it is not canonical under the new history ...
    let v = verify_with(&state, s, &root, &old, &facts_b, true);
    assert!(matches!(&v, Ok(Verdict::Fail { code: "BEACON_NOT_CANONICAL", .. })), "{v:?}");
    // ... and the evidence alone carries a beacon output and seed that this node no longer derives.
    let alone = scratch("reorg-alone");
    std::fs::copy(&old, alone.join("evidence.borsh")).unwrap();
    let v = verify_with(&state, s, &root, &alone.join("evidence.borsh"), &facts_b, true);
    assert!(matches!(&v, Ok(Verdict::Fail { code: "BEACON_MISMATCH" | "STALE_OR_FORGED_SEED", .. })), "{v:?}");
    assert!(verify_with(&state, s, &root, &old, &facts_a, true).unwrap().is_pass());
    assert!(verify_with(&state, s, &root, &dir_b.join("evidence.borsh"), &facts_b, true).unwrap().is_pass());
    // The reorg is undone: the first history is canonical again, its retained evidence is reused and unmarked, the other is marked.
    let (ev_a2, dir_a2) = run_ok(&state, s, &root, &facts_a);
    assert_eq!((ev_a2, dir_a2.clone()), (ev_a.clone(), dir_a.clone()));
    assert!(!dir_a.join("INVALIDATED").exists() && dir_b.join("INVALIDATED").exists());
    assert!(std::fs::read_to_string(ledger_path(&state)).unwrap().contains("REVALIDATED"));
    // A reorg that removes the sources before the lock: the evidence cannot be judged at all on that branch.
    let mut thin = facts_a.clone();
    thin.events.truncate(1);
    thin.tip_position = 130;
    let v = verify_with(&state, s, &root, &old, &thin, true).unwrap();
    assert!(matches!(v, Verdict::Pending { .. }), "{v:?}");
}

#[test]
fn a_presented_beacon_that_is_reordered_non_canonical_or_forged_is_refused() {
    let s = shared();
    let state = scratch("beacon-forge");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let (_, dir) = run_ok(&state, s, &root, &facts);
    let evp = dir.join("evidence.borsh");
    let beacon: WorkBeaconV1 = borsh::from_slice(&std::fs::read(dir.join("beacon.borsh")).unwrap()).unwrap();
    let try_beacon = |what: &str, f: &dyn Fn(&mut WorkBeaconV1)| {
        let mut bad = beacon.clone();
        f(&mut bad);
        let d = scratch("beacon-forge-case");
        std::fs::copy(&evp, d.join("evidence.borsh")).unwrap();
        std::fs::write(d.join("beacon.borsh"), borsh::to_vec(&bad).unwrap()).unwrap();
        let v = verify_with(&state, s, &root, &d.join("evidence.borsh"), &facts, true);
        assert!(matches!(&v, Ok(Verdict::Fail { code: "BEACON_NOT_CANONICAL", .. })), "{what}: {v:?}");
    };
    try_beacon("contribution reorder", &|b| b.sources.swap(0, 2));
    try_beacon("duplicated contribution", &|b| b.sources[1] = b.sources[0].clone());
    try_beacon("non-canonical (heartbeat position)", &|b| b.sources[0].accepted_position = 1);
    try_beacon("substituted work identity", &|b| b.sources[2].canonical_work_id = named_id("someone else's work"));
    try_beacon("dropped contribution", &|b| {
        b.sources.pop();
    });
    try_beacon("forged accumulator", &|b| b.accumulators[1][0] ^= 1);
    try_beacon("forged output", &|b| b.output[0] ^= 1);
    try_beacon("forged anchor", &|b| b.challenge_anchor[0] ^= 1);
    try_beacon("forged lock position", &|b| b.lock_position += 1);
    // The honest presentation is accepted.
    assert!(verify_with(&state, s, &root, &evp, &facts, true).unwrap().is_pass());
}

#[test]
fn forged_evidence_is_never_a_pass_whichever_field_is_edited() {
    let s = shared();
    let state = scratch("forge");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    let (ev, dir) = run_ok(&state, s, &root, &facts);
    fn flip(d: &mut Digest) {
        d[0] ^= 1;
    }
    type Edit = Box<dyn Fn(&mut BeaconConformanceEvidenceV1)>;
    let cases: Vec<(&str, Edit)> = vec![
        ("version", Box::new(|e| e.version = 2)),
        ("commitment_root", Box::new(|e| flip(&mut e.commitment_root))),
        ("challenge_policy_id", Box::new(|e| flip(&mut e.challenge_policy_id))),
        ("challenge_anchor", Box::new(|e| flip(&mut e.challenge_anchor))),
        ("beacon_output", Box::new(|e| flip(&mut e.beacon_output))),
        ("challenge_seed", Box::new(|e| flip(&mut e.challenge_seed))),
        ("lock_position", Box::new(|e| e.lock_position += 1)),
        ("qualifying_source_evidence_root", Box::new(|e| flip(&mut e.qualifying_source_evidence_root))),
        ("selected_vectors_root", Box::new(|e| flip(&mut e.selected_vectors_root))),
        ("selected_tensor_ranges_root", Box::new(|e| flip(&mut e.selected_tensor_ranges_root))),
        ("reference_result_root", Box::new(|e| flip(&mut e.reference_result_root))),
        ("independent_result_root", Box::new(|e| flip(&mut e.independent_result_root))),
        ("backend_result_root", Box::new(|e| flip(&mut e.backend_result_root))),
        ("authenticated_openings_root", Box::new(|e| flip(&mut e.authenticated_openings_root))),
        ("public_material_locator_root", Box::new(|e| flip(&mut e.public_material_locator_root))),
        ("scope_and_fault_model_id", Box::new(|e| flip(&mut e.scope_and_fault_model_id))),
        ("transcript_root", Box::new(|e| e.transcript_root = RootV1::Present([7; 64]))),
        ("checks_required", Box::new(|e| e.checks_required += 1)),
        ("checks_run", Box::new(|e| e.checks_run -= 1)),
        ("checks_failed", Box::new(|e| e.checks_failed = 1)),
        ("a stronger epsilon than the scope derives", Box::new(|e| e.derived_epsilon_bits += 40)),
        ("a failure appended to a pass", Box::new(|e| e.failures.push("leaf/r0/k0: invented".into()))),
        ("status", Box::new(|e| e.status = ConformanceStatusV1::Failed)),
        ("status BeaconUnavailable", Box::new(|e| e.status = ConformanceStatusV1::BeaconUnavailable)),
    ];
    for (name, edit) in &cases {
        let mut forged = ev.clone();
        edit(&mut forged);
        let p = write_forged(&dir, "forged.borsh", &forged);
        let v = verify_with(&state, s, &root, &p, &facts, true);
        assert!(not_pass(&v), "{name}: a forged field passed: {v:?}");
    }
    // Garbage is not evidence.
    std::fs::write(dir.join("forged.borsh"), b"not evidence").unwrap();
    assert!(matches!(
        verify_with(&state, s, &root, &dir.join("forged.borsh"), &facts, true),
        Ok(Verdict::Fail { code: "EVIDENCE_MALFORMED", .. })
    ));
    // The honest evidence passes, so the table above refused the edits and not the evidence.
    assert!(verify_with(&state, s, &root, &dir.join("evidence.borsh"), &facts, true).unwrap().is_pass());
}

#[test]
fn skipped_and_incomplete_are_never_a_pass_even_when_the_status_and_counters_are_forged_to_say_so() {
    let s = shared();
    let state = scratch("skipped");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    // The independent implementation is switched off: SKIPPED, honestly.
    let (ev, dir) =
        match run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet { exec: true, ref2: false }, None, None).unwrap() {
            RunOutcome::Evidence { evidence, dir, local, .. } => {
                assert!(matches!(
                    local,
                    Err(misaka_palw_challenge::conformance::ConformanceRefusalV1::NotPassed(ConformanceStatusV1::Skipped))
                ));
                (evidence, dir)
            }
            o => panic!("{o:?}"),
        };
    assert_eq!(ev.status, ConformanceStatusV1::Skipped);
    assert!(ev.missing_checks.iter().all(|m| m.contains("independent")), "{:?}", ev.missing_checks);
    let v = verify_with(&state, s, &root, &dir.join("evidence.borsh"), &facts, true).unwrap();
    assert!(matches!(v, Verdict::NotPass { status: ConformanceStatusV1::Skipped, .. }), "{v:?}");
    // CONFORMANCE_SKIPPED claimed as PASS: status, counters and missing list edited to look complete. The verifier's own run of the
    // independent implementation does not reproduce the skipped run's independent result root.
    let mut forged = ev.clone();
    forged.status = ConformanceStatusV1::Passed;
    forged.checks_run = forged.checks_required;
    forged.missing_checks.clear();
    let p = write_forged(&dir, "forged.borsh", &forged);
    let v = verify_with(&state, s, &root, &p, &facts, true).unwrap();
    assert!(matches!(&v, Verdict::Fail { code: "EVIDENCE_NOT_REPRODUCED", .. }), "{v:?}");
    // An incomplete run (checks never run) writes no evidence at all.
    let state2 = scratch("incomplete");
    let (_, root2) = commit(&state2, &s.pack, &s.class_file, &test_params());
    let r = run_with(&state2, &s.pack, &s.class_file, &root2, &facts, ImplSet::default(), Some(2), None).unwrap();
    assert!(matches!(r, RunOutcome::Interrupted { done: 2, .. }), "{r:?}");
}

#[test]
fn a_disagreeing_implementation_is_a_failed_check_and_a_failed_evidence_is_not_a_pass() {
    let s = shared();
    let state = scratch("fault");
    let (b, _) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    for (id, role) in [("leaf/r1/k3", Role::Independent), ("vec/r0/i1", Role::Backend), ("vec/r2/i0", Role::Reference)] {
        let st = scratch("fault-case");
        let (_, r) = commit(&st, &s.pack, &s.class_file, &test_params());
        let out = run_with(
            &st,
            &s.pack,
            &s.class_file,
            &r,
            &facts,
            ImplSet::default(),
            None,
            Some(InjectedFault { check_id: id.into(), role }),
        )
        .unwrap();
        let RunOutcome::Evidence { evidence, dir, local, .. } = out else { panic!("expected evidence") };
        assert_eq!(evidence.status, ConformanceStatusV1::Failed, "{id}");
        assert_eq!(evidence.checks_failed, 1);
        assert!(evidence.failures[0].starts_with(id), "{:?}", evidence.failures);
        assert!(local.is_err());
        // An honest verifier re-executes without the fault: the producer's failed evidence is not reproduced, and is not a pass.
        let v = verify_with(&st, s, &r, &dir.join("evidence.borsh"), &facts, true);
        assert!(not_pass(&v), "{id}: {v:?}");
    }
}

#[test]
fn a_commitment_whose_artifact_layout_plan_policy_or_implementation_moved_is_stale() {
    let s = shared();
    let state = scratch("stale");
    let (b, root) = commit(&state, &s.pack, &s.class_file, &test_params());
    let facts = facts_of(&b);
    // The honest inputs run.
    run_ok(&state, s, &root, &facts);

    // Artifact changed after commit: one weight byte flipped in a copy of the class file.
    let work = scratch("stale-artifact");
    let tampered = work.join("class.palwtir");
    std::fs::copy(&s.class_file, &tampered).unwrap();
    {
        use std::io::{Read, Seek, SeekFrom, Write};
        let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(&tampered).unwrap();
        let (off, len) = c.locate(c.header.tensors[0].param, c.header.tensors[0].layer).unwrap();
        let mut f = std::fs::OpenOptions::new().read(true).write(true).open(&tampered).unwrap();
        f.seek(SeekFrom::Start(off + len / 2)).unwrap();
        let mut byte = [0u8; 1];
        f.read_exact(&mut byte).unwrap();
        byte[0] ^= 0x40;
        f.seek(SeekFrom::Start(off + len / 2)).unwrap();
        f.write_all(&byte).unwrap();
    }
    let e = run_with(&state, &s.pack, &tampered, &root, &facts, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(e.code, "COMMITMENT_STALE", "{e}");

    // Plan substitution: the verification-plan root edited in the stored commitment. Moved to its new name it is a different
    // statement the pack/artifact do not derive; left in place it no longer matches its own directory.
    let cdir = state.join(commitment_dirname(&b.commitment.statement_root()));
    let mut c: ConformanceCommitmentV1 = borsh::from_slice(&std::fs::read(cdir.join("commitment.borsh")).unwrap()).unwrap();
    c.verification_plan_root[0] ^= 1;
    let st2 = scratch("stale-plan");
    let moved = st2.join(commitment_dirname(&c.statement_root()));
    copy_dir(&cdir, &moved);
    std::fs::write(moved.join("commitment.borsh"), borsh::to_vec(&c).unwrap()).unwrap();
    let r = run_with(&st2, &s.pack, &s.class_file, &hex(&c.statement_root()), &facts, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(r.code, "COMMITMENT_STALE", "{r}");
    assert!(r.detail.contains("verification_plan_root"), "{r}");
    std::fs::write(cdir.join("commitment.borsh"), borsh::to_vec(&c).unwrap()).unwrap();
    let r = run_with(&state, &s.pack, &s.class_file, &root, &facts, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(r.code, "COMMITMENT_INVALID", "{r}");

    // Implementation set substituted: the committed revisions are what the run must be.
    let st3 = scratch("stale-impl");
    let (b3, _) = commit(&st3, &s.pack, &s.class_file, &test_params());
    let d3 = st3.join(commitment_dirname(&b3.commitment.statement_root()));
    let mut c3 = b3.commitment.clone();
    c3.implementation_set_root[0] ^= 1;
    let moved3 = st3.join(commitment_dirname(&c3.statement_root()));
    copy_dir(&d3, &moved3);
    std::fs::write(moved3.join("commitment.borsh"), borsh::to_vec(&c3).unwrap()).unwrap();
    let r = run_with(&st3, &s.pack, &s.class_file, &hex(&c3.statement_root()), &facts, ImplSet::default(), None, None).unwrap_err();
    assert_eq!(r.code, "COMMITMENT_STALE", "{r}");
    assert!(r.detail.contains("implementation_set_root"), "{r}");
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn the_cli_commits_resumes_runs_and_verifies_with_exit_codes_that_mean_pending_and_pass() {
    let s = shared();
    let work = scratch("cli");
    let bin = env!("CARGO_BIN_EXE_palw-class");
    let go = |args: &[&str]| -> (i32, String, String) {
        let o = std::process::Command::new(bin).arg("pack").args(args).output().expect("runs");
        (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
    };
    let (pack, class, state) = (s.pack.to_str().unwrap(), s.class_file.to_str().unwrap(), work.join("state"));
    let st = state.to_str().unwrap();
    let (code, out, err) = go(&[
        "commit-conformance",
        "--pack",
        pack,
        "--artifact",
        class,
        "--state",
        st,
        "--network",
        "testnet-12",
        "--chain-genesis",
        "label:cli-genesis",
        "--ruleset-id",
        "label:cli-ruleset",
        "--repetitions",
        "3",
        "--security-bits",
        "8",
        "--vectors",
        "2",
        "--prompt-len",
        "3",
        "--decode",
        "1",
        "--leaves",
        "8",
        "--vector-fault-ppm",
        "1000000",
        "--leaf-fault-ppm",
        "500000",
    ]);
    assert_eq!(code, 0, "{err}");
    let root = out.trim().to_string();
    assert_eq!(root.len(), 128);
    assert!(err.contains("UNAPPROVED TEST POLICY") && err.contains("hypothetically armed") && err.contains("SAMPLED"), "{err}");
    let (r16, f) = (&root[..16], work.join("facts.json"));
    let f = f.to_str().unwrap();
    // Two of three works: WaitingRandomness, exit 3, nothing run.
    let (code, ..) = go(&["synthetic-facts", "--state", st, "--commitment", r16, "--out", f, "--works", "2", "--tip", "1010"]);
    assert_eq!(code, 0);
    let (code, out, _) =
        go(&["run-conformance", "--pack", pack, "--artifact", class, "--state", st, "--commitment", r16, "--facts", f]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("WAITING_RANDOMNESS"), "{out}");
    // The window closes short: BEACON_UNAVAILABLE, exit 3.
    let (code, ..) = go(&["synthetic-facts", "--state", st, "--commitment", r16, "--out", f, "--works", "2", "--tip", "2000"]);
    assert_eq!(code, 0);
    let (code, out, _) =
        go(&["run-conformance", "--pack", pack, "--artifact", class, "--state", st, "--commitment", r16, "--facts", f]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("BEACON_UNAVAILABLE"), "{out}");
    // Enough canonical history: evidence, exit 0; a fresh process verifies it, exit 0.
    let (code, ..) = go(&["synthetic-facts", "--state", st, "--commitment", r16, "--out", f]);
    assert_eq!(code, 0);
    let (code, out, err) =
        go(&["run-conformance", "--pack", pack, "--artifact", class, "--state", st, "--commitment", r16, "--facts", f]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("PASSED") && out.contains("synthetic:"), "{out}");
    let seed_dir = std::fs::read_dir(state.join(&root[..32]))
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("seed-"))
        .unwrap()
        .path();
    let ev = seed_dir.join("evidence.borsh");
    let evs = ev.to_str().unwrap();
    let (code, out, err) = go(&[
        "verify-conformance",
        "--pack",
        pack,
        "--artifact",
        class,
        "--state",
        st,
        "--commitment",
        r16,
        "--facts",
        f,
        "--evidence",
        evs,
    ]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("PASS") && out.contains("SYNTHETIC") && out.contains("not full-scope fidelity"), "{out}");
    let (code, out, _) = go(&[
        "verify-conformance",
        "--pack",
        pack,
        "--artifact",
        class,
        "--state",
        st,
        "--commitment",
        r16,
        "--facts",
        f,
        "--evidence",
        evs,
        "--no-rerun",
    ]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("NOT A PASS"), "{out}");
    // A tampered evidence file: exit 2.
    let mut bytes = std::fs::read(&ev).unwrap();
    let n = bytes.len();
    bytes[n / 2] ^= 1;
    let forged = seed_dir.join("forged.borsh");
    std::fs::write(&forged, bytes).unwrap();
    let (code, out, _) = go(&[
        "verify-conformance",
        "--pack",
        pack,
        "--artifact",
        class,
        "--state",
        st,
        "--commitment",
        r16,
        "--facts",
        f,
        "--evidence",
        forged.to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "{out}");
    let (code, out, _) = go(&["conformance-status", "--state", st]);
    assert_eq!(code, 0);
    assert!(out.contains("BEACON_UNAVAILABLE") && out.contains("EVIDENCE"), "{out}");
    let _ = std::fs::remove_dir_all(work);
}
