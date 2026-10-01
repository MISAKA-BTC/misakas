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
