//! **The node's evidence against F5's court** (feature `node`; RFC-0002 Phase F, F9): every IR
//! court object a node assembles — cone refutations, logits-consistency accusations, decode-token
//! pins — built from the node's own retention by [`TirEvidenceV1`] and adjudicated by the SHIPPED
//! court (`kaspa_consensus_core::palw_tir_court_v1`).
//!
//! Over the admissible programs (the range analysis proves them), under both logits schemes and
//! both prompt forms:
//!
//! * every leaf of every honest node execution, refuted with the evidence the node's store builds
//!   by REPLAY (from the retained resume points), is acquitted — and the replayed preimages are the
//!   execution's own;
//! * a planted lie inside its proven interval is convicted as a computation mismatch at that lane,
//!   and one outside it by PALW-TIR-33 — from the accused's dense capture, and from a challenger's
//!   store over its own honest re-execution plus the accused's disputed leaf, which builds the
//!   byte-identical refutation;
//! * the bisection's prefix states find the planted leaf;
//! * the logits-consistency accusation and the decode-token door acquit the honest run.

#![cfg(feature = "node")]

mod node_common;

use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step_leg::{PalwStepFaultV1, PalwStepTileLeafV1, step_merkle_root_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirCourtRulesV1, check_tir_cone_refutation_v1, check_tir_decode_token_tiled_v1, check_tir_logits_consistency_v1,
    palw_tir_leaf_interval_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{
    PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1,
};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::{DType, MapParams, TirProgramV1};
use misaka_palw_tir_exec::node::{
    TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1, TirRetainedJobV1, TirStepTreeV1, tir_first_divergence_v1,
};
use misaka_palw_tir_exec::{TirParams, TirPlan};
use node_common::{job, layout, programs, tiled};

const CAP: u64 = 1 << 26;

fn rules(form: PalwPromptIdsFormV1) -> PalwTirCourtRulesV1 {
    PalwTirCourtRulesV1 { max_step_leaf_count: CAP, prompt_form: form, limits: DemandLimits::UNLIMITED }
}

/// One admissible program under one logits scheme, prompt form and layout.
struct Case {
    name: String,
    program: TirProgramV1,
    params: MapParams,
    form: PalwPromptIdsFormV1,
    c: u32,
    h: u32,
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (k, (name, program, params)) in programs().into_iter().enumerate() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue; // not a class: the court refuses what the range analysis cannot prove
        }
        // Flat and tiled; the prompt forms and layouts rotate over the programs.
        for (s, p) in [program.clone(), tiled(program)].into_iter().enumerate() {
            let p = TirProgramV1::decode_canonical(&p.encode()).expect("still canonical");
            let form = if (k + s) % 2 == 0 { PalwPromptIdsFormV1::Flat } else { PalwPromptIdsFormV1::MerkleV1 };
            let (c, h) = [(2, 2), (1, 4), (3, 1), (4, 8)][(k + s) % 4];
            let scheme = if s == 0 { "flat" } else { "tiled" };
            out.push(Case {
                name: format!("{name} ({scheme}, {form:?}, C {c}, h {h})"),
                program: p,
                params: params.clone(),
                form,
                c,
                h,
            });
        }
    }
    out
}

/// Everything one case's execution needs, built once.
struct Fixture {
    plan: TirPlan,
    class: PalwTirClassV1,
    artifact_root: Hash64,
    space: PalwTirStepSpaceV1,
    ctx: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    prompt: Vec<u32>,
}

const PREFILL: u32 = 4;
const DECODE: u32 = 3;

fn fixture(case: &Case) -> Fixture {
    let plan = TirPlan::compile(&case.program).unwrap();
    struct Map<'m>(&'m MapParams);
    impl PalwTirTensorSourceV1 for Map<'_> {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<std::borrow::Cow<'_, [u8]>> {
            self.0.tensors.get(&(param, layer)).map(|t| std::borrow::Cow::Owned(t.to_le_bytes()))
        }
    }
    let (artifact_root, _) = palw_tir_inventory_root_v1(&case.program, &Map(&case.params)).expect("an inventory");
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: case.program.encode(),
        layout: layout(&case.program, 7, case.c, case.h, 64),
        tokenizer_id: Hash64::from_bytes([3; 64]),
    };
    let class_id = class.class_id(&artifact_root);
    let space = PalwTirStepSpaceV1::new(&class).unwrap_or_else(|e| panic!("{}: {e}", case.name));
    let prompt: Vec<u32> = (0..PREFILL).map(|i| (i * 5 + 3) % case.program.token_bound).collect();
    let ctx = job(&case.program, PREFILL, DECODE, class_id, &prompt, case.form);
    Fixture { plan, class, artifact_root, space, ctx, prompt }
}

/// Lanes as committed — written raw, so a forgery can hold a lane outside its node's dtype.
fn set_lane(p: &mut PalwStepTileLeafV1, dtype: DType, lane: usize, v: i128) {
    let bytes = if dtype == DType::Idx { (v as u32).to_le_bytes() } else { (v as i32).to_le_bytes() };
    p.values_le[4 * lane..4 * lane + 4].copy_from_slice(&bytes);
}

fn lane(p: &PalwStepTileLeafV1, dtype: DType, lane: usize) -> i128 {
    let b: [u8; 4] = p.values_le[4 * lane..4 * lane + 4].try_into().unwrap();
    if dtype == DType::Idx { u32::from_le_bytes(b) as i128 } else { i32::from_le_bytes(b) as i128 }
}

/// A commitment to `preimages` (honest or not) over the honest trace.
fn commit(f: &Fixture, honest: &TirRetainedJobV1, preimages: &[PalwStepTileLeafV1]) -> (PalwTirStepBindingV1, Vec<Hash64>) {
    let class_id = f.class.class_id(&f.artifact_root);
    let ctx_hash = f.ctx.context_hash();
    let hashes: Vec<Hash64> = preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &class_id, p)).collect();
    let root = step_merkle_root_v1(&hashes).unwrap();
    let mut b = honest.binding.clone();
    b.step_merkle_root = root;
    b.committed_execution_root = palw_tir_execution_root_v1(&ctx_hash, &b.full_logits_trace_root, &class_id, b.step_leaf_count, &root);
    (b, hashes)
}

#[test]
fn the_node_builds_what_the_court_acquits_and_convicts() {
    let (mut acquitted, mut convicted, mut tir33, mut same_object, mut logits, mut pins) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let all = cases();
    assert!(all.len() >= 10, "{} cases", all.len());
    for case in &all {
        let f = fixture(case);
        let name = &case.name;
        let rules = rules(case.form);
        let tparams = TirParams::from_map(&f.plan, &case.params).unwrap();
        let src = TirParamsSourceV1(&tparams);
        let inventory = TirHeldInventoryV1::build(&case.program, &src).unwrap();
        assert_eq!(inventory.tree.root(), f.artifact_root, "{name}: the held inventory is the consensus inventory");
        let runner = TirClassRunnerV1::new(&f.space, &f.plan, &tparams, f.class.class_id(&f.artifact_root)).unwrap();
        let honest = runner.retain(&f.class, f.artifact_root, &f.ctx, &f.prompt, CAP).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut dense = Vec::new();
        runner.run(&f.ctx, &f.prompt, CAP, true, &mut |l| dense.push(l.preimage.clone())).unwrap();
        let own = TirEvidenceV1::own(&runner, &honest, &inventory, case.form, CAP).unwrap();
        let intervals = analyze_ranges(&f.space.program).unwrap();

        // Every honest leaf, from the replaying store, acquitted; the replay is the run.
        for i in 0..honest.binding.step_leaf_count {
            use kaspa_consensus_core::palw_tir_court_v1::PalwTirEvidenceStoreV1;
            assert_eq!(own.step_leaf(i).as_ref(), Some(&dense[i as usize]), "{name}: replayed leaf {i}");
            let r = own.cone_refutation(i, &rules).unwrap_or_else(|e| panic!("{name}: leaf {i}: {e}"));
            assert_eq!(check_tir_cone_refutation_v1(&r, &rules), Err(PalwStepRefuteError::NoFaultFound), "{name}: honest leaf {i}");
            let leaf = f.space.leaf_at(&f.ctx, i).unwrap();
            *kinds
                .entry(match leaf.kind {
                    PalwTirLeafKindV1::Commit { .. } => "commit",
                    PalwTirLeafKindV1::State { .. } => "checkpoint",
                    PalwTirLeafKindV1::HistTile { .. } => "history tile",
                })
                .or_default() += 1;
            acquitted += 1;
        }

        // Planted lies: inside the interval (a mismatch) and outside it (PALW-TIR-33).
        let n = honest.binding.step_leaf_count;
        let stride = (n / 24).max(1);
        for i in (0..n).step_by(stride as usize) {
            let leaf = f.space.leaf_at(&f.ctx, i).unwrap();
            let iv = palw_tir_leaf_interval_v1(&f.space, &intervals, &leaf).unwrap();
            let k = leaf.value_count as usize / 2;
            let v = lane(&dense[i as usize], leaf.dtype, k);
            let inside = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w));
            let outside = iv.hi.checked_add(1).filter(|w| *w <= i32::MAX as i128 && leaf.dtype != DType::Idx);
            for (forged, outside_lie) in [(inside, false), (outside, true)] {
                let Some(w) = forged else { continue };
                let mut lie = dense.clone();
                set_lane(&mut lie[i as usize], leaf.dtype, k, w);
                let (binding, hashes) = commit(&f, &honest, &lie);
                assert_eq!(tir_first_divergence_v1(&f.ctx, &honest.leaf_hashes, &hashes), Some(i), "{name}: the ladder finds the lie");
                // The accused's own capture.
                let accused = TirEvidenceV1::dense(
                    &runner,
                    &binding,
                    &f.prompt,
                    &honest.logits_rows,
                    &honest.generated,
                    &lie,
                    &inventory,
                    case.form,
                    CAP,
                )
                .unwrap();
                let r = accused.cone_refutation(i, &rules).unwrap_or_else(|e| panic!("{name}: forged leaf {i}: {e}"));
                let verdict = check_tir_cone_refutation_v1(&r, &rules).unwrap_or_else(|e| panic!("{name}: forged leaf {i}: {e}"));
                if outside_lie {
                    assert_eq!(verdict.fault, PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: k as u32 }, "{name}: {i}");
                    tir33 += 1;
                } else {
                    assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: k as u32 }, "{name}: {i}");
                    convicted += 1;
                }
                // A challenger's store: its own honest re-execution and the accused's disputed leaf.
                let opening = TirStepTreeV1::full(&hashes).opening(i).unwrap();
                let challenger = TirEvidenceV1::challenger(
                    &runner,
                    &binding,
                    &honest,
                    &opening,
                    lie[i as usize].clone(),
                    &honest.logits_rows,
                    &honest.generated,
                    &inventory,
                    case.form,
                    CAP,
                )
                .unwrap_or_else(|e| panic!("{name}: challenger at {i}: {e}"));
                let rc = challenger.cone_refutation(i, &rules).unwrap_or_else(|e| panic!("{name}: challenger at {i}: {e}"));
                assert_eq!(rc, r, "{name}: leaf {i}: the challenger builds the accused's canonical refutation");
                same_object += 1;
            }
        }

        // The logits-consistency accusation over every logits tile, and the decode-token door.
        let post = (f.space.occurrences().len() - 1) as u32;
        for i in 0..n {
            let l = f.space.leaf_at(&f.ctx, i).unwrap();
            if matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == f.space.program.logits)
            {
                let a = own.logits_consistency(i).unwrap_or_else(|e| panic!("{name}: logits leaf {i}: {e}"));
                assert_eq!(check_tir_logits_consistency_v1(&a, &rules), Err(PalwStepRefuteError::NoFaultFound), "{name}: {i}");
                logits += 1;
            }
        }
        if Hash64::from_bytes(f.space.program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
            let vocab = honest.logits_rows[0].len() as u32;
            for row in 0..DECODE {
                for beat in 0..vocab.min(6) {
                    let pin = own.decode_token_pin(row, beat).unwrap();
                    assert_eq!(check_tir_decode_token_tiled_v1(&honest.binding, &pin, &rules), Err(PalwStepRefuteError::NoFaultFound));
                    pins += 1;
                }
            }
        }
    }
    eprintln!(
        "{} cases: {acquitted} honest leaves acquitted ({kinds:?}); {convicted} lies convicted as mismatches, {tir33} by PALW-TIR-33, \
         {same_object} challenger refutations byte-identical to the accused's; {logits} logits accusations and {pins} decode pins acquitted",
        all.len()
    );
    assert_eq!(kinds.len(), 3, "commit tiles, checkpoints and history tiles");
    assert!(convicted > 50 && tir33 > 20 && logits > 0 && pins > 0);
}

/// The held step tree answers what the consensus functions answer — and a challenger's partial
/// tree (its own prefix, the accused's disputed leaf and path) opens every range before the
/// disputed leaf exactly as the accused's whole tree does.
#[test]
fn the_step_tree_opens_as_the_consensus_tree() {
    use kaspa_consensus_core::palw_step_leg::{step_merkle_range_siblings_v1, step_opening_v1};
    let h = |i: u64, salt: u8| {
        Hash64::from_bytes([(i as u8) ^ salt; 64]).as_bytes().iter().map(|b| b.wrapping_mul(31)).collect::<Vec<u8>>()
    };
    let leaf = |i: u64, salt: u8| {
        let mut b = [0u8; 64];
        b.copy_from_slice(&h(i, salt));
        b[0..8].copy_from_slice(&i.to_le_bytes());
        b[8] = salt;
        Hash64::from_bytes(b)
    };
    let mut checked = 0usize;
    for n in [1u64, 2, 3, 5, 7, 8, 13, 16, 31, 33, 64, 100] {
        let honest: Vec<Hash64> = (0..n).map(|i| leaf(i, 0)).collect();
        let tree = TirStepTreeV1::full(&honest);
        assert_eq!(tree.root(), Some(step_merkle_root_v1(&honest).unwrap()));
        for i in 0..n {
            assert_eq!(tree.opening(i), Some(step_opening_v1(&honest, i).unwrap()), "n {n} leaf {i}");
        }
        for first in 0..n {
            for count in 1..=(n - first).min(9) {
                assert_eq!(
                    tree.range_siblings(first, count),
                    Some(step_merkle_range_siblings_v1(&honest, first as usize, count as usize).unwrap()),
                    "n {n} range {first}+{count}"
                );
                checked += 1;
            }
        }
        // The accused differs from `l` on; the challenger holds its prefix and the accused's path.
        for l in 0..n {
            let accused: Vec<Hash64> = (0..n).map(|i| if i < l { honest[i as usize] } else { leaf(i, 0xA5) }).collect();
            let opening = step_opening_v1(&accused, l).unwrap();
            let partial = TirStepTreeV1::prefix_with_opening(&honest, n, &opening).unwrap();
            assert_eq!(partial.root(), Some(step_merkle_root_v1(&accused).unwrap()), "n {n} l {l}");
            assert_eq!(partial.opening(l), Some(opening.clone()));
            for first in 0..l {
                for count in 1..=(l - first) {
                    assert_eq!(
                        partial.range_siblings(first, count),
                        Some(step_merkle_range_siblings_v1(&accused, first as usize, count as usize).unwrap()),
                        "n {n} l {l} range {first}+{count}"
                    );
                    checked += 1;
                }
            }
            // An opening that disagrees with the prefix is refused.
            if l > 0 {
                let other: Vec<Hash64> = (0..n).map(|i| leaf(i, 0x3C)).collect();
                let wrong = step_opening_v1(&other, l).unwrap();
                if wrong.siblings.iter().zip(&opening.siblings).any(|(a, b)| a != b) {
                    let bad = TirStepTreeV1::prefix_with_opening(&honest, n, &wrong);
                    assert!(bad.is_err() || bad.unwrap().root() != Some(step_merkle_root_v1(&accused).unwrap()));
                }
            }
        }
    }
    assert!(checked > 5000, "{checked}");
}

/// The held inventory tree is the consensus inventory: its root, and every leaf's opening
/// (`palw_tir_open_leaf_v1`, verified against the root).
#[test]
fn the_inventory_tree_opens_as_the_consensus_inventory() {
    use kaspa_consensus_core::palw_artifact::verify_artifact_opening_v1;
    use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_open_leaf_v1;
    let mut opened = 0usize;
    for (name, program, params) in programs() {
        if program.params.is_empty() {
            continue;
        }
        let plan = TirPlan::compile(&program).unwrap();
        let tparams = TirParams::from_map(&plan, &params).unwrap();
        let src = TirParamsSourceV1(&tparams);
        let held = TirHeldInventoryV1::build(&program, &src).unwrap();
        let (root, count) = palw_tir_inventory_root_v1(&program, &src).unwrap();
        assert_eq!((held.tree.root(), held.tree.leaf_count()), (root, count), "{name}");
        use misaka_palw_tir_exec::node::TirParamOpenerV1;
        for leaf in 0..count {
            let o = held.param_opening(leaf).unwrap();
            assert_eq!(o, palw_tir_open_leaf_v1(&program, &src, leaf).unwrap(), "{name} leaf {leaf}");
            verify_artifact_opening_v1(&o, root).unwrap();
            opened += 1;
        }
        assert!(held.param_opening(count).is_none());
    }
    assert!(opened > 100, "{opened}");
}
