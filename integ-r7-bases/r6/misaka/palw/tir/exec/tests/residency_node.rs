//! **A budgeted IR artifact computes what the mapped one computes** (feature `node`; ADR-0112 I-1 …
//! I-7 for IR classes, `docs/design/palw/tir/runtime-residency.md`).
//!
//! Every program of the node suites (the golden vectors, the corpus models, a program whose
//! committed nodes carry `H`) and a 64-expert mixture, written as PALWTIR1 containers and opened
//! MAPPED and under residencies — at the floor, at a fifth, and holding everything — with every
//! row-addressed param served by rows: the same inventory root and leaf count, the same capture
//! (execution, trace and output roots, every leaf), the same readiness material, the same openings
//! and the same court closes, byte for byte. Under a residency nothing is mapped and no served
//! instance is ever read whole (I-4). The mixture's budget holds a fraction of its experts: it evicts
//! and reads again, never holds more routed rows than the budget leaves (I-2), and a composite
//! candidate of it shares its store (one per parent root) and computes what the mapped candidate
//! does. And the budget's rules: a stated budget below the floor is refused by name (I-3), a default
//! takes a fifth within what is spare and declines to the page cache below the floor (I-7), and the
//! policy's arithmetic (I-5).

#![cfg(feature = "node")]

mod node_common;
mod residency_common;

// The reference crate's builders, once: `node_common` includes them, `residency_common` reads them as
// `crate::tircommon`.
use node_common::tircommon;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT;
use kaspa_consensus_core::palw_improve_composite_v1::palw_tir_composite_ref_v1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::{MapParams, TirProgramV1};
use misaka_palw_tir_exec::node::{
    TirArtifactV1, TirBackendV1, TirCaptureV1, TirClassRunnerV1, TirParamOpenerV1, TirResidencyPolicyV1,
};
use misaka_palw_tir_exec::{TirTierRulesV1, TirTiersV1};
use node_common::{job, layout, programs, tiled};
use residency_common::{many_expert_moe, many_expert_moe_candidate};

const ALL_ROWS: TirTierRulesV1 = TirTierRulesV1 { pin_below_bytes: 0 };

/// A scratch directory of its own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("tir-residency-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(dir: &Path, name: &str, program: &TirProgramV1, params: &MapParams) -> PathBuf {
    let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
    let lay = layout(program, 5, 2, 2, 64);
    let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
        params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
    };
    misaka_palw_tir_artifact::write_container_v1(&path, program, borsh::to_vec(&lay).unwrap(), [2; 64], name.into(), &mut tensor)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    path
}

/// Everything a node derives from an artifact that the chain or a court can see.
#[derive(Debug, PartialEq)]
struct Seen {
    root: (Hash64, u32),
    execution_root: Hash64,
    trace_root: Hash64,
    output_root: Hash64,
    material: Vec<u8>,
    readiness: (Hash64, Vec<Hash64>, usize),
    openings: Vec<Vec<u8>>,
    closes: Vec<Vec<u8>>,
}

fn seen(name: &str, artifact: Arc<TirArtifactV1>, k: usize) -> Seen {
    let root = artifact.inventory_root().unwrap_or_else(|e| panic!("{name}: {e}"));
    let class = artifact.class().unwrap();
    let canonical = palw_tir_canonical_context_v1(&class, class.class_id(&root.0), (4, 3)).unwrap();
    let form = if k.is_multiple_of(3) { PalwPromptIdsFormV1::MerkleV1 } else { PalwPromptIdsFormV1::Flat };
    let backend = TirBackendV1::new(name.into(), artifact.clone(), root.0, canonical, form, 1 << 26).unwrap();
    let (job, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x5A ^ k as u8; 64])).unwrap();
    let outcome = backend.execute(&job, &prompt).unwrap_or_else(|e| panic!("{name}: {e}"));
    let court = PalwCourtParamsV2::new(PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2).unwrap();
    let rules = backend.court_rules(&court);
    let n = TirCaptureV1::decode(&outcome.material).unwrap().binding.step_leaf_count;
    let closes = [0, n / 2, n - 1]
        .iter()
        .map(|i| borsh::to_vec(&backend.cone_close(&outcome.material, *i, &rules).unwrap_or_else(|e| panic!("{name}: {e}"))).unwrap())
        .collect();
    let readiness = if artifact.composite_ref().is_none() {
        let (_, leaf_count) = backend.artifact_root_and_leaf_count().unwrap();
        let draw: Vec<u32> = (0..leaf_count).step_by(5).collect();
        let (r, leaves, opened) = backend.artifact_readiness_material(&draw).unwrap();
        (r, leaves, opened.len())
    } else {
        (root.0, Vec::new(), 0)
    };
    let leaves = root.1;
    let openings = [0, leaves / 3, leaves / 2, leaves - 1]
        .iter()
        .map(|l| borsh::to_vec(&artifact.param_opening(*l).unwrap_or_else(|| panic!("{name}: leaf {l} opens"))).unwrap())
        .collect();
    Seen {
        root,
        execution_root: outcome.execution_root,
        trace_root: outcome.trace_root,
        output_root: outcome.output_root,
        material: outcome.material,
        readiness,
        openings,
        closes,
    }
}

/// The node's programs and the mixture, each with its weights.
fn corpus() -> Vec<(String, TirProgramV1, MapParams)> {
    let mut out: Vec<(String, TirProgramV1, MapParams)> = programs()
        .into_iter()
        .filter(|(_, p, _)| analyze_ranges(p).is_ok() && !p.params.is_empty())
        .enumerate()
        .map(|(k, (name, p, params))| {
            let p = if k % 2 == 0 { p } else { TirProgramV1::decode_canonical(&tiled(p).encode()).unwrap() };
            (name, p, params)
        })
        .collect();
    let (moe, gens) = many_expert_moe(64, 2, 4);
    let params = tircommon::models::materialize(&moe, &gens, 21);
    out.push(("many experts".into(), node_common::flat(moe), params));
    out
}

#[test]
fn a_budgeted_artifact_computes_what_the_mapped_one_does_at_the_floor_at_a_fifth_and_whole() {
    let dir = Scratch::new("identity");
    let (mut resident_runs, mut routed_runs, mut gathered_runs) = (0, 0, 0);
    for (k, (name, program, params)) in corpus().into_iter().enumerate() {
        let path = write(&dir.0, &name, &program, &params);
        let mapped = Arc::new(TirArtifactV1::open(&path).unwrap());
        assert!(mapped.is_mapped() && mapped.residency_stats().is_none());
        // The mapped root is the consensus inventory's.
        let consensus = palw_tir_inventory_root_v1(&program, mapped.as_ref() as &dyn PalwTirTensorSourceV1).unwrap();
        assert_eq!(mapped.inventory_root().unwrap(), consensus, "{name}");
        let want = seen(&name, mapped, k);
        let a = TirTiersV1::of(&program, ALL_ROWS).arithmetic();
        let mut policies = vec![TirResidencyPolicyV1::Bytes(a.floor_bytes), TirResidencyPolicyV1::Bytes(a.weight_bytes)];
        if a.fifth_bytes >= a.floor_bytes {
            policies.push(TirResidencyPolicyV1::FifthOfTheWeights);
        }
        for policy in policies {
            let art = Arc::new(TirArtifactV1::open_with_rules(&path, policy, ALL_ROWS).unwrap_or_else(|e| panic!("{name}: {e}")));
            assert!(!art.is_mapped(), "{name} {policy:?}: nothing of a budgeted artifact is mapped");
            let got = seen(&name, art.clone(), k);
            assert_eq!(got, want, "{name} under {policy:?}");
            let stats = art.residency_stats().expect("a residency");
            assert_eq!(stats.whole_reads, 0, "{name} {policy:?}: no served instance read whole");
            assert!(stats.resident_routed_bytes <= stats.routed_capacity_bytes, "{name} {policy:?}: {stats:?}");
            resident_runs += 1;
            routed_runs += (stats.hits + stats.misses > 0) as usize;
            gathered_runs += (stats.gathered_rows > 0) as usize;
        }
    }
    eprintln!("{resident_runs} budgeted runs: {routed_runs} routed rows, {gathered_runs} gathered rows");
    assert!(resident_runs >= 12, "{resident_runs}");
    assert!(routed_runs >= 6 && gathered_runs >= 12, "the row paths ran: {routed_runs} routed, {gathered_runs} gathered");
}

#[test]
fn a_budget_holding_a_fraction_of_the_experts_evicts_reads_again_and_never_holds_more_than_it_leaves() {
    let dir = Scratch::new("experts");
    let (moe, gens) = many_expert_moe(64, 2, 4);
    let program = node_common::flat(moe);
    let params = tircommon::models::materialize(&program, &gens, 33);
    let path = write(&dir.0, "experts", &program, &params);
    let a = TirTiersV1::of(&program, ALL_ROWS).arithmetic();
    assert!(a.routed_capacity(a.fifth_bytes) * 6 < a.routed_bytes, "a fifth holds under a sixth of the experts: {a:?}");
    // Fifteen positions: each routes two of 64 experts in each of four layers.
    let run = |art: &TirArtifactV1, check: &mut dyn FnMut()| {
        let class = art.class().unwrap();
        let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).unwrap();
        let (root, _) = art.inventory_root().unwrap();
        let class_id = class.class_id(&root);
        let prompt: Vec<u32> = (0..12).map(|i| (i * 7 + 3) % 48).collect();
        let ctx = job(&program, 12, 4, class_id, &prompt, PalwPromptIdsFormV1::Flat);
        TirClassRunnerV1::new(&space, art.plan(), art.params(), class_id)
            .unwrap()
            .run(&ctx, &prompt, u64::MAX, false, &mut |_| check())
            .unwrap()
    };
    let mapped = TirArtifactV1::open(&path).unwrap();
    let want = run(&mapped, &mut || {});
    let open_bytes = a.weight_bytes;
    for (label, policy) in
        [("the floor", TirResidencyPolicyV1::Bytes(a.floor_bytes)), ("a fifth", TirResidencyPolicyV1::FifthOfTheWeights)]
    {
        let art = TirArtifactV1::open_with_rules(&path, policy, ALL_ROWS).unwrap();
        let s0 = art.residency_stats().unwrap();
        assert_eq!(s0.bytes_read, open_bytes, "{label}: opening reads every byte once");
        let mut worst = 0u64;
        let got = run(&art, &mut || worst = worst.max(art.residency_stats().unwrap().resident_routed_bytes));
        assert_eq!(got, want, "{label}: the mapped artifact's job, leaf for leaf");
        let s1 = art.residency_stats().unwrap();
        eprintln!("{label}: {s1:?}");
        assert!(worst <= s1.routed_capacity_bytes, "{label}: I-2, {worst} > {}", s1.routed_capacity_bytes);
        if policy == TirResidencyPolicyV1::Bytes(a.floor_bytes) {
            assert_eq!(s1.routed_capacity_bytes, a.routed_token_bytes, "at the floor the cache holds one token's routed rows");
        }
        assert!(s1.evictions > 0, "{label}: {s1:?}");
        assert_eq!(s1.bytes_read - s0.bytes_read, s1.routed_bytes_read + s1.gathered_bytes, "{label}: every read is counted");
        assert_eq!(s1.whole_reads, 0);
        // The same job again: a budget this tight has evicted most of what the first run read, so it
        // misses and reads again — every row byte-identical to the mapping's.
        let again = run(&art, &mut || {});
        assert_eq!(again, want, "{label}: again");
        let s2 = art.residency_stats().unwrap();
        assert!(s2.misses > s1.misses && s2.routed_bytes_read > s1.routed_bytes_read, "{label}: re-read: {s1:?} → {s2:?}");
    }
    // A budget that holds every expert reads each once: the second run is all hits.
    let whole =
        TirArtifactV1::open_with_rules(&path, TirResidencyPolicyV1::Bytes(a.weight_bytes + a.in_flight_bytes), ALL_ROWS).unwrap();
    assert_eq!(run(&whole, &mut || {}), want);
    let s1 = whole.residency_stats().unwrap();
    assert_eq!(run(&whole, &mut || {}), want);
    let s2 = whole.residency_stats().unwrap();
    assert_eq!((s2.misses, s2.evictions), (s1.misses, 0), "nothing evicted, nothing missed the second time: {s2:?}");
    assert!(s2.hits > s1.hits);
}

#[test]
fn a_composite_candidate_shares_its_parents_store_and_runs_what_the_mapped_candidate_runs() {
    let dir = Scratch::new("composite");
    let (parent, gens) = many_expert_moe(64, 2, 4);
    let parent = node_common::flat(parent);
    let (candidate, cgens) = many_expert_moe_candidate(64, 2, 4);
    let candidate = node_common::flat(candidate);
    let p = parent.params.len() as u32;
    let pparams = tircommon::models::materialize(&parent, &gens, 41);
    // The candidate's parent params are the parent's, byte for byte; its adapter its own.
    let mut cparams = tircommon::models::materialize(&candidate, &cgens, 41);
    for (key, t) in &pparams.tensors {
        cparams.tensors.insert(*key, t.clone());
    }
    let ppath = write(&dir.0, "parent", &parent, &pparams);
    let mapped_parent = TirArtifactV1::open(&ppath).unwrap();
    let (parent_root, _) = mapped_parent.inventory_root().unwrap();
    let parent_class = mapped_parent.class().unwrap().class_id(&parent_root);
    let src = misaka_palw_tir_exec::node::TirParamsSourceV1(
        &misaka_palw_tir_exec::TirParams::from_map(&misaka_palw_tir_exec::TirPlan::compile(&candidate).unwrap(), &cparams).unwrap(),
    );
    let r = palw_tir_composite_ref_v1(parent_class, &candidate, p, &src).unwrap();
    assert_eq!(r.parent_root, parent_root);
    let spath = dir.0.join("candidate.palwtirs");
    let lay = layout(&candidate, 5, 2, 2, 64);
    misaka_palw_tir_artifact::write_section_v1(
        &spath,
        &candidate,
        p,
        borsh::to_vec(&lay).unwrap(),
        [2; 64],
        "cand".into(),
        &mut |j, l| cparams.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}")),
    )
    .unwrap();
    let mapped = Arc::new(TirArtifactV1::open_composite(&ppath, &spath, &r, Some(parent_root)).unwrap());
    assert!(mapped.is_mapped());
    let want = seen("candidate", mapped, 1);
    // The parent under a budget; two candidates over it share its store.
    let a = TirTiersV1::of(&parent, TirTierRulesV1::default()).arithmetic();
    let resident_parent = TirArtifactV1::open_with_residency(&ppath, TirResidencyPolicyV1::Bytes(a.weight_bytes)).unwrap();
    let store = resident_parent.weight_store().expect("a residency").clone();
    let one = Arc::new(TirArtifactV1::open_composite_over(&resident_parent, &spath, &r).unwrap());
    let two = Arc::new(TirArtifactV1::open_composite(&ppath, &spath, &r, Some(parent_root)).unwrap());
    // The adapter: a logits bias of 48 `i32`.
    let adapter_bytes = 48 * 4;
    for (label, cand) in [("over the held parent", &one), ("through the process's stores", &two)] {
        assert!(!cand.is_mapped(), "{label}");
        assert!(Arc::ptr_eq(cand.weight_store().unwrap(), &store), "{label}: one store per parent root");
        assert_eq!(cand.own_pinned_bytes(), adapter_bytes, "{label}: the adapter pinned, and nothing of the parent's twice");
    }
    assert_eq!(seen("candidate", one.clone(), 1), want, "the candidate over its parent's store");
    assert_eq!(seen("candidate", two, 1), want, "the second candidate too");
    assert_eq!(store.stats().whole_reads, 0, "the candidate pinned the embedding it reads whole instead of falling back");
    // Under the tier rules that serve every row-addressed param, the candidate's tied embedding is
    // read whole where the parent's store serves it by rows: the candidate pins it for itself.
    let tight = TirTiersV1::of(&parent, ALL_ROWS).arithmetic();
    let rows_parent = TirArtifactV1::open_with_rules(&ppath, TirResidencyPolicyV1::Bytes(tight.floor_bytes), ALL_ROWS).unwrap();
    let cand = Arc::new(TirArtifactV1::open_composite_over(&rows_parent, &spath, &r).unwrap());
    let embedding = (parent.param_index("tok_embd").unwrap(), None);
    assert!(rows_parent.weight_store().unwrap().serves(embedding.0, embedding.1), "the parent's store gathers the embedding");
    assert_eq!(
        cand.own_pinned_bytes(),
        48 * 16 + adapter_bytes,
        "the candidate pinned the embedding, and only it, beside its adapter"
    );
    assert_eq!(seen("candidate", cand.clone(), 1), want, "at the parent's floor");
    assert_eq!(cand.weight_store().unwrap().stats().whole_reads, 0);
}

#[test]
fn a_stated_budget_below_the_floor_is_refused_by_name() {
    let dir = Scratch::new("floor");
    let (moe, gens) = many_expert_moe(64, 2, 4);
    let program = node_common::flat(moe);
    let path = write(&dir.0, "floor", &program, &tircommon::models::materialize(&program, &gens, 1));
    let a = TirTiersV1::of(&program, ALL_ROWS).arithmetic();
    let err = TirArtifactV1::open_with_rules(&path, TirResidencyPolicyV1::Bytes(a.floor_bytes - 1), ALL_ROWS).map(|_| ()).unwrap_err();
    for term in [a.floor_bytes, a.pinned_bytes, a.routed_token_bytes, a.in_flight_bytes] {
        assert!(err.contains(&term.to_string()), "{err}");
    }
    assert!(err.contains("floor") && err.contains("pinned set"), "{err}");
    assert!(TirArtifactV1::open_with_rules(&path, TirResidencyPolicyV1::Bytes(a.floor_bytes), ALL_ROWS).is_ok(), "the floor opens");
}

#[test]
fn a_default_budget_is_a_fifth_within_what_is_spare_and_below_the_floor_the_page_cache() {
    let dir = Scratch::new("default");
    let (moe, gens) = many_expert_moe(64, 2, 4);
    let program = node_common::flat(moe);
    let path = write(&dir.0, "default", &program, &tircommon::models::materialize(&program, &gens, 2));
    let a = TirTiersV1::of(&program, ALL_ROWS).arithmetic();
    assert!(a.floor_bytes < a.fifth_bytes);
    let open = |policy| TirArtifactV1::open_with_rules(&path, policy, ALL_ROWS).unwrap();
    let roomy = open(TirResidencyPolicyV1::FifthWithin(u64::MAX));
    assert_eq!(roomy.residency_stats().unwrap().budget_bytes, a.fifth_bytes, "room for a fifth takes a fifth");
    let tight = open(TirResidencyPolicyV1::FifthWithin(a.floor_bytes));
    assert_eq!(tight.residency_stats().unwrap().budget_bytes, a.floor_bytes, "room for less takes what there is");
    let short = open(TirResidencyPolicyV1::FifthWithin(a.floor_bytes - 1));
    assert!(short.is_mapped() && short.residency_stats().is_none(), "below the floor the page cache decides");
    let declined = short.residency_declined().expect("and says why");
    assert_eq!((declined.budget_bytes, declined.floor_bytes, declined.fifth_bytes), (a.floor_bytes - 1, a.floor_bytes, a.fifth_bytes));
    assert_eq!((declined.weight_bytes, declined.routed_bytes), (a.weight_bytes, a.routed_bytes));
    assert_eq!(short.inventory_root().unwrap(), roomy.inventory_root().unwrap(), "the same root either way");
    // A dense class's fifth is far under its floor: the default leaves it on the page cache — never
    // refused, because nobody stated a number.
    let (dense, dgens) = tircommon::models::dense(&[misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL]);
    let dense = node_common::flat(dense);
    let dpath = write(&dir.0, "dense", &dense, &tircommon::models::materialize(&dense, &dgens, 3));
    let art = TirArtifactV1::open_with_residency(&dpath, TirResidencyPolicyV1::FifthOfTheWeights).unwrap();
    assert!(art.is_mapped() && art.residency_declined().is_some());
}

#[test]
fn the_residency_policy_arithmetic() {
    assert_eq!(TirResidencyPolicyV1::FifthOfTheWeights.budget_for(35_727_649_280), Some(7_145_529_856));
    assert_eq!(TirResidencyPolicyV1::FifthOfTheWeights.budget_for(11), Some(3), "a fifth rounds up");
    assert_eq!(TirResidencyPolicyV1::Bytes(7).budget_for(100), Some(7));
    assert_eq!(TirResidencyPolicyV1::FifthWithin(10).budget_for(100), Some(10));
    assert_eq!(TirResidencyPolicyV1::FifthWithin(30).budget_for(100), Some(20));
    assert_eq!(TirResidencyPolicyV1::PageCache.budget_for(100), None, "0 is the page cache");
    assert!(TirResidencyPolicyV1::Bytes(1).is_stated() && !TirResidencyPolicyV1::FifthWithin(1).is_stated());
}

/// Every committed value a step delivers, with its slot, and the step's logits.
#[derive(Default)]
struct Steps {
    commits: Vec<(u32, Vec<i128>)>,
}

impl misaka_palw_tir_exec::StepSink for Steps {
    fn node(&mut self, v: &misaka_palw_tir_exec::NodeValue<'_>) {
        self.commits.push((v.slot, v.data.to_i128s()));
    }
}

/// **The 64-expert mixture held at its floor and three composite candidates of it** (RFC-0004 §6.3) —
/// the same program, three adapters (three seeds' logit biases) — opened over the parent's store, each
/// with its whole weights as a map (what the reference reads).
struct MoeBatch {
    _dir: Scratch,
    _parent: TirArtifactV1,
    store: Arc<misaka_palw_tir_exec::node::TirWeightStoreV1>,
    candidate: TirProgramV1,
    members: Vec<(TirArtifactV1, MapParams)>,
}

fn moe_batch(name: &str) -> MoeBatch {
    let dir = Scratch::new(name);
    let (parent, gens) = many_expert_moe(64, 2, 4);
    let parent = node_common::flat(parent);
    let (candidate, cgens) = many_expert_moe_candidate(64, 2, 4);
    let candidate = node_common::flat(candidate);
    let p = parent.params.len() as u32;
    let pparams = tircommon::models::materialize(&parent, &gens, 51);
    let ppath = write(&dir.0, "parent", &parent, &pparams);
    let a = TirTiersV1::of(&parent, ALL_ROWS).arithmetic();
    let resident = TirArtifactV1::open_with_rules(&ppath, TirResidencyPolicyV1::Bytes(a.floor_bytes), ALL_ROWS).unwrap();
    let (parent_root, _) = resident.inventory_root().unwrap();
    let parent_class = resident.class().unwrap().class_id(&parent_root);
    let store = resident.weight_store().unwrap().clone();
    let mut members = Vec::new();
    for seed in [61u64, 62, 63] {
        let mut cparams = tircommon::models::materialize(&candidate, &cgens, seed);
        for (key, t) in &pparams.tensors {
            cparams.tensors.insert(*key, t.clone());
        }
        let plan = misaka_palw_tir_exec::TirPlan::compile(&candidate).unwrap();
        let held = misaka_palw_tir_exec::TirParams::from_map(&plan, &cparams).unwrap();
        let r = palw_tir_composite_ref_v1(parent_class, &candidate, p, &misaka_palw_tir_exec::node::TirParamsSourceV1(&held)).unwrap();
        let spath = dir.0.join(format!("candidate-{seed}.palwtirs"));
        let lay = layout(&candidate, 5, 2, 2, 64);
        misaka_palw_tir_artifact::write_section_v1(
            &spath,
            &candidate,
            p,
            borsh::to_vec(&lay).unwrap(),
            [2; 64],
            "c".into(),
            &mut |j, l| cparams.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}")),
        )
        .unwrap();
        members.push((TirArtifactV1::open_composite_over(&resident, &spath, &r).unwrap(), cparams));
    }
    assert!(members.iter().all(|(c, _)| Arc::ptr_eq(c.weight_store().unwrap(), &store)), "one store for the batch");
    assert!(
        members.iter().all(|(c, _)| c.own_pinned_bytes() == 48 * 16 + 48 * 4),
        "each candidate pins its adapter and the embedding its tied head reads whole — the experts are the store's"
    );
    MoeBatch { _dir: dir, _parent: resident, store, candidate, members }
}

/// **RFC-0004's candidates, stepped in lockstep** (`TirLockstepV1`, runtime-residency.md §8): three
/// composite candidates of one parent — the same program, three adapters — over the parent's store at
/// its floor, on the same tokens. Each member's commits, logits and run state are what it computes
/// stepped alone; and where the members run one after another the store reads every token's experts
/// once per candidate, the batch reads them once: the first member's admission of a layer is the
/// others' hits.
#[test]
fn candidates_stepped_in_lockstep_compute_what_each_computes_alone_and_read_the_parents_rows_once() {
    use misaka_palw_tir_exec::{TirExecutor, TirLockstepV1, tir_lockstep_batch_v1};
    let moe = moe_batch("lockstep");
    let store = &moe.store;
    let candidates: Vec<&TirArtifactV1> = moe.members.iter().map(|(c, _)| c).collect();
    let tokens: Vec<u32> = (0..10).map(|i| (i * 11 + 5) % 48).collect();
    // Each candidate alone, one after another.
    let before_alone = store.stats();
    let mut alone: Vec<Vec<(Vec<(u32, Vec<i128>)>, Vec<i128>)>> = Vec::new();
    for c in &candidates {
        let mut exec = TirExecutor::new(c.plan(), c.params()).unwrap();
        let mut steps = Vec::new();
        for t in &tokens {
            let mut sink = Steps::default();
            exec.step(*t, &mut sink).unwrap();
            steps.push((sink.commits, exec.logits().1.to_i128s()));
        }
        alone.push(steps);
    }
    let after_alone = store.stats();
    // The same candidates in lockstep.
    let execs: Vec<TirExecutor<'_>> = candidates.iter().map(|c| TirExecutor::new(c.plan(), c.params()).unwrap()).collect();
    let mut batch = TirLockstepV1::new(execs).unwrap();
    for (k, t) in tokens.iter().enumerate() {
        let mut sinks: Vec<Steps> = (0..candidates.len()).map(|_| Steps::default()).collect();
        let mut refs: Vec<&mut dyn misaka_palw_tir_exec::StepSink> =
            sinks.iter_mut().map(|s| s as &mut dyn misaka_palw_tir_exec::StepSink).collect();
        let results = batch.step(&vec![*t; candidates.len()], &mut refs);
        assert!(results.iter().all(|r| r.is_ok()), "{results:?}");
        for (i, (sink, member)) in sinks.into_iter().zip(batch.members()).enumerate() {
            assert_eq!(sink.commits, alone[i][k].0, "candidate {i} position {k}: the commits it computes alone");
            assert_eq!(member.logits().1.to_i128s(), alone[i][k].1, "candidate {i} position {k}: its logits");
        }
    }
    let after_batch = store.stats();
    let (sequential, lockstep) = (
        after_alone.routed_bytes_read - before_alone.routed_bytes_read,
        after_batch.routed_bytes_read - after_alone.routed_bytes_read,
    );
    eprintln!("routed rows read: {sequential} bytes one candidate after another, {lockstep} in lockstep");
    assert!(lockstep * 2 < sequential, "the batch reads a layer's rows once: {lockstep} vs {sequential}");
    assert_eq!(after_batch.whole_reads, 0);
    // The bound: a layer's admission of every member held at once.
    assert_eq!(
        tir_lockstep_batch_v1(after_batch.routed_capacity_bytes, after_batch.in_flight_bytes, 0, 0),
        4,
        "the floor holds 4 layers' worth"
    );
    assert_eq!(tir_lockstep_batch_v1(1 << 20, 0, 1 << 10, 1 << 12), 4, "memory alone bounds a batch with no routed rows");
    assert_eq!(tir_lockstep_batch_v1(0, 1, 1, 0), 1, "at least one");
}

/// The reference's params of one pipeline: a candidate's whole weights, no stepper.
struct ReferenceParams<'a>(&'a MapParams);

impl misaka_palw_tir::pipeline::PipelineParams for ReferenceParams<'_> {
    fn params(&self, _: u16) -> &dyn misaka_palw_tir::ParamSource {
        self.0
    }
}

/// A node's params of one pipeline: the subject stage on the executor over the held artifact (a
/// fresh stepper, or the hub's seat), the reference's map behind it for any other stage.
struct NodeParams<'a> {
    artifact: &'a TirArtifactV1,
    map: &'a MapParams,
    seat: std::cell::RefCell<Option<misaka_palw_tir_exec::TirLockstepSeatV1>>,
}

impl misaka_palw_tir::pipeline::PipelineParams for NodeParams<'_> {
    fn params(&self, _: u16) -> &dyn misaka_palw_tir::ParamSource {
        self.map
    }
    fn stepper(
        &self,
        _: u16,
        decl: &misaka_palw_tir::program_v2::TirProgramV2,
    ) -> Option<Box<dyn misaka_palw_tir::pipeline::StageStepperV1 + '_>> {
        use misaka_palw_tir::pipeline::StageStepperV1;
        if let Some(seat) = self.seat.borrow_mut().take() {
            return Some(Box::new(seat) as Box<dyn StageStepperV1 + '_>);
        }
        misaka_palw_tir_exec::TirStageStepperV1::for_stage(self.artifact.plan(), self.artifact.params(), decl)
            .map(|s| Box::new(s) as Box<dyn StageStepperV1 + '_>)
    }
}

struct NoRandom;

impl misaka_palw_tir::pipeline::RandomSource for NoRandom {
    fn random(&self, _: u16, _: misaka_palw_tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<misaka_palw_tir::Tensor> {
        None
    }
}

/// **Three candidates' subject stages through one hub, over the parent's store** (RFC-0004 §7.2;
/// `misaka_palw_tir_exec::stage`): each candidate's pipeline on its own thread — the stage the reference
/// pipeline runner runs, replaying one prompt and generating from it — computes exactly the run the
/// reference interpreter computes over the candidate's whole weights; one after another on the executor
/// the store reads every position's experts once per candidate, through the hub once for the batch.
#[test]
fn candidates_evaluated_through_one_hub_run_the_reference_and_read_the_parents_rows_once() {
    use misaka_palw_tir::pipeline::{PipelineJob, StageDecl, TextSelectV1, TirPipelineV1, TripRule, run_pipeline, run_text_pipeline};
    use misaka_palw_tir::program_v2::{OutputDecl, TirProgramV2};
    use misaka_palw_tir_exec::{TirLockstepHubV1, TirStageStepperV1};
    let moe = moe_batch("hub");
    let c = &moe.candidate;
    let decl =
        TirProgramV2::from_v1_lifting_params(c, &[], OutputDecl::Logits { node: c.logits, scheme_id: c.logits_scheme_id }).unwrap();
    let pipeline = TirPipelineV1 {
        version: misaka_palw_tir::pipeline::TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "subject".into(),
            program: 0,
            trip: TripRule::TextStream,
            max_trip: 64,
            tokens: None,
            bind: vec![],
        }],
        output_stage: 0,
    };
    let programs = vec![decl.clone()];
    let job = PipelineJob { prompt: (0..10).map(|i| (i * 11 + 5) % 48).collect(), ..Default::default() };
    let select = || {
        let mut n = 0;
        move |_: u32, logits: &misaka_palw_tir::Tensor| {
            n += 1;
            // The arg-max, as a greedy decode selects; the fourth id is the last.
            let id = (0..logits.data.len()).max_by_key(|i| (logits.data[*i], std::cmp::Reverse(*i))).unwrap() as u32;
            if n >= 4 { TextSelectV1::Last(id) } else { TextSelectV1::Next(id) }
        }
    };
    for generate in [false, true] {
        let run = |pp: &dyn misaka_palw_tir::pipeline::PipelineParams| {
            if generate {
                run_text_pipeline(&pipeline, &programs, pp, &NoRandom, &job, &mut select()).unwrap()
            } else {
                (run_pipeline(&pipeline, &programs, pp, &NoRandom, &job).unwrap(), Vec::new())
            }
        };
        let want: Vec<_> = moe.members.iter().map(|(_, map)| run(&ReferenceParams(map))).collect();
        // One after another on the executor.
        let s0 = moe.store.stats();
        for (member, want) in moe.members.iter().zip(&want) {
            let alone = NodeParams { artifact: &member.0, map: &member.1, seat: Default::default() };
            assert_eq!(&run(&alone), want, "a candidate alone on the executor is the reference");
        }
        let s1 = moe.store.stats();
        // Through one hub.
        let steppers = moe.members.iter().map(|(a, _)| TirStageStepperV1::for_stage(a.plan(), a.params(), &decl).unwrap()).collect();
        let (hub, seats) = TirLockstepHubV1::new(steppers).unwrap();
        let (runs, served) = std::thread::scope(|scope| {
            let handles: Vec<_> = moe
                .members
                .iter()
                .zip(seats)
                .map(|(member, seat)| {
                    let run = &run;
                    scope.spawn(move || {
                        let pp = NodeParams { artifact: &member.0, map: &member.1, seat: std::cell::RefCell::new(Some(seat)) };
                        run(&pp)
                    })
                })
                .collect();
            let served = hub.serve();
            (handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>(), served)
        });
        let s2 = moe.store.stats();
        assert_eq!(runs, want, "through the hub, each candidate's run is the reference's (generating: {generate})");
        let (sequential, together) = (s1.routed_bytes_read - s0.routed_bytes_read, s2.routed_bytes_read - s1.routed_bytes_read);
        eprintln!(
            "{}: routed rows read {sequential} bytes one candidate after another, {together} through the hub ({served:?})",
            if generate { "generating" } else { "replaying" }
        );
        if !generate {
            assert!(together * 2 < sequential, "the hub reads a layer's rows once for the batch: {together} vs {sequential}");
            assert_eq!(served.member_steps, 3 * served.rounds, "{served:?}");
        }
        assert_eq!(s2.whole_reads, 0);
    }
}
