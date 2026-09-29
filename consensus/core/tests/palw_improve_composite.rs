//! **RFC-0004 §6.3 (PALW-MIP-15): a composite artifact in the IR court.**
//!
//! Every corpus class is re-registered as a composite of itself: its params split at `p` into a
//! parent section (params `0..p`, committed under a parent root) and an adapter section (params `p..`,
//! a tree of their own), its artifact root the composite root over the two, its class id Phase F's
//! formula over that. Then, on the honest run and on forgeries:
//!
//! * every leaf is ACQUITTED through closes whose parameters ride as sub-root openings — the
//!   parent's leaves at their own indices under the parent root, the adapter's rebased to its
//!   section — and every single-lane forgery sampled is CONVICTED through them;
//! * a one-root multiproof is refused for a composite class, and a composite opening for a one-root
//!   class; every malformed composite opening is refused, never a panic, never a conviction;
//! * the carriage's first two forms encode exactly as the `Option` they replace, and the composite
//!   reference weighs what the close sizing charges for it.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOpeningV1, artifact_leaf_v1, artifact_root_v1, palw_artifact_multiproof_v1,
};
use kaspa_consensus_core::palw_improve_composite_v1::{PalwTirCompositeOpeningV1, PalwTirCompositeRefV1, palw_tir_composite_split_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use kaspa_consensus_core::palw_step_leg::{PalwStepFaultV1, PalwStepOpeningV1, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwStepRefuteError};
use kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_COMPOSITE_REF_BYTES_V1;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, PalwTirEvidenceStoreV1, PalwTirParamOpeningV1, build_tir_cone_refutation_v1, check_tir_cone_refutation_v1,
};

const PARENT_CLASS: Hash64 = Hash64::from_bytes([0x9C; 64]);

/// The fixture as a composite of itself split at `p`: its reference and the split leaf.
fn composite(mut f: Fixture, p: u32) -> (Fixture, PalwTirCompositeRefV1, u32) {
    let program = f.class.decode_program().expect("decodes");
    let split = palw_tir_composite_split_v1(&program, p).expect("a split").parent_leaves;
    let hashes: Vec<Hash64> = f.ops.iter().map(artifact_leaf_v1).collect();
    let r = PalwTirCompositeRefV1 {
        parent_class: PARENT_CLASS,
        parent_root: artifact_root_v1(&hashes[..split as usize]).expect("a parent section"),
        adapter_root: artifact_root_v1(&hashes[split as usize..]).expect("an adapter section"),
        p,
    };
    f.artifact_root = r.artifact_root();
    f.class_id = f.class.class_id(&f.artifact_root);
    f.ctx.shape_profile_id = f.class_id;
    (f, r, split)
}

/// Every corpus fixture as a composite split at its middle param.
fn composites() -> Vec<(Fixture, PalwTirCompositeRefV1, u32)> {
    fixtures()
        .into_iter()
        .map(|f| {
            let n = f.class.decode_program().expect("decodes").params.len() as u32;
            assert!(n >= 2, "{}: two sections need two params", f.name);
            composite(f, n / 2)
        })
        .collect()
}

/// The two sections' multiproofs of `leaves` (the class's inventory indices).
fn sections(f: &Fixture, split: u32, leaves: &[u32]) -> (Option<PalwArtifactMultiproofV1>, Option<PalwArtifactMultiproofV1>) {
    let hashes: Vec<Hash64> = f.ops.iter().map(artifact_leaf_v1).collect();
    let (s, all) = (split as usize, hashes.as_slice());
    let pick = |range: std::ops::Range<usize>, base: u32| {
        let opened: Vec<_> =
            leaves.iter().filter(|l| range.contains(&(**l as usize))).map(|l| (*l - base, f.ops[*l as usize].clone())).collect();
        if opened.is_empty() { None } else { palw_artifact_multiproof_v1(&all[range.clone()], &opened) }
    };
    (pick(0..s, 0), pick(s..all.len(), split))
}

/// A store answering the composite form.
struct CompositeStore<'a> {
    inner: Store<'a>,
    r: PalwTirCompositeRefV1,
    split: u32,
}

impl PalwTirEvidenceStoreV1 for CompositeStore<'_> {
    fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        self.inner.step_leaf(index)
    }
    fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        self.inner.step_opening(index)
    }
    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        self.inner.step_range_siblings(first, count)
    }
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.inner.param_opening(leaf)
    }
    fn param_carriage(&self, leaves: &[u32]) -> Option<PalwTirParamOpeningV1> {
        let (parent, adapter) = sections(self.inner.f, self.split, leaves);
        Some(PalwTirParamOpeningV1::Composite(Box::new(PalwTirCompositeOpeningV1 { artifact: self.r, parent, adapter })))
    }
    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        self.inner.prompt_token_ids()
    }
    fn prompt_ids_opening(&self, tile: u32) -> Option<PalwPromptIdsOpeningV1> {
        self.inner.prompt_ids_opening(tile)
    }
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
        self.inner.decode_pin()
    }
}

fn refute_composite(f: &Fixture, x: &Execution, r: &PalwTirCompositeRefV1, split: u32, leaf: u64) -> PalwTirConeRefutationV1 {
    let store = CompositeStore { inner: Store { f, x }, r: *r, split };
    build_tir_cone_refutation_v1(&x.binding, leaf, &store, &RULES).unwrap_or_else(|e| panic!("{}: leaf {leaf}: {e}", f.name))
}

fn composite_of(r: &PalwTirConeRefutationV1) -> &PalwTirCompositeOpeningV1 {
    match &r.params {
        PalwTirParamOpeningV1::Composite(c) => c,
        other => panic!("not a composite carriage: {other:?}"),
    }
}

#[test]
fn every_honest_leaf_of_a_composite_class_is_acquitted_through_its_sub_roots() {
    let (mut parent_only, mut adapter_only, mut both, mut none, mut total) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for (f, r, split) in composites() {
        let x = f.honest();
        for i in 0..f.leaves.len() {
            let close = refute_composite(&f, &x, &r, split, i as u64);
            assert_eq!(check_tir_cone_refutation_v1(&close, &RULES), Err(PalwStepRefuteError::NoFaultFound), "{} leaf {i}", f.name);
            match &close.params {
                PalwTirParamOpeningV1::None => none += 1,
                PalwTirParamOpeningV1::Composite(c) => match (&c.parent, &c.adapter) {
                    (Some(_), None) => parent_only += 1,
                    (None, Some(_)) => adapter_only += 1,
                    (Some(_), Some(_)) => both += 1,
                    (None, None) => panic!("{} leaf {i}: a composite opening of nothing", f.name),
                },
                PalwTirParamOpeningV1::Single(_) => panic!("{} leaf {i}: a one-root carriage from a composite store", f.name),
            }
            total += 1;
        }
    }
    eprintln!("{total} closes: {parent_only} parent-only, {adapter_only} adapter-only, {both} both sections, {none} reading no param");
    assert!(parent_only > 0 && adapter_only > 0 && both > 0, "every shape of a composite carriage is adjudicated");
    assert!(total > 900);
}

#[test]
fn a_forgery_of_a_composite_class_is_convicted_through_its_sub_roots() {
    let mut convicted = 0usize;
    for (f, r, split) in composites() {
        let stride = (f.leaves.len() / 60).max(1);
        for i in (0..f.leaves.len()).step_by(stride) {
            let leaf = &f.leaves[i];
            let lane = leaf.value_count as usize / 2;
            let v = f.values[i][lane];
            let iv = f.interval(leaf);
            let Some(forged) = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w)) else { continue };
            let mut values = f.values.clone();
            values[i][lane] = forged;
            let x = f.commit(&values, &f.rows, &f.generated);
            let close = refute_composite(&f, &x, &r, split, i as u64);
            let verdict = check_tir_cone_refutation_v1(&close, &RULES).unwrap_or_else(|e| panic!("{} leaf {i}: {e}", f.name));
            assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: lane as u32 }, "{} leaf {i}", f.name);
            convicted += 1;
        }
    }
    assert!(convicted > 150, "{convicted} forgeries convicted");
}

#[test]
fn one_root_and_composite_carriages_do_not_cross() {
    let refused = |v: Result<_, PalwStepRefuteError>| matches!(v, Err(PalwStepRefuteError::InputSetNotCanonical(_)));
    let mut crossed = 0usize;
    for (f, r, split) in composites() {
        let x = f.honest();
        let Some(i) = (0..f.leaves.len()).find(|i| !refute_composite(&f, &x, &r, split, *i as u64).params.is_none()) else { continue };
        let close = refute_composite(&f, &x, &r, split, i as u64);
        // The class's leaves as ONE multiproof over the whole inventory: not the class's root.
        let leaves: Vec<u32> = {
            let c = composite_of(&close);
            let mut l: Vec<u32> = c.parent.iter().flat_map(|p| p.opened.iter().map(|(i, _)| *i)).collect();
            l.extend(c.adapter.iter().flat_map(|p| p.opened.iter().map(|(i, _)| *i + split)));
            l
        };
        let hashes: Vec<Hash64> = f.ops.iter().map(artifact_leaf_v1).collect();
        let opened: Vec<_> = leaves.iter().map(|l| (*l, f.ops[*l as usize].clone())).collect();
        let mut single = close.clone();
        single.params = PalwTirParamOpeningV1::Single(palw_artifact_multiproof_v1(&hashes, &opened).expect("a multiproof"));
        assert!(refused(check_tir_cone_refutation_v1(&single, &RULES)), "{}: a one-root carriage of a composite class", f.name);
        // The composite opening against the one-root class the fixture was: not its artifact.
        let plain = fixtures().into_iter().find(|g| g.name == f.name).expect("the fixture");
        let px = plain.honest();
        let mut onto = refute(&plain, &px, i as u64);
        onto.params = close.params.clone();
        assert!(refused(check_tir_cone_refutation_v1(&onto, &RULES)), "{}: a composite carriage of a one-root class", f.name);
        crossed += 1;
    }
    assert_eq!(crossed, 5);
}

#[test]
fn a_malformed_composite_opening_is_refused_never_a_panic() {
    let mut adjudicated = 0usize;
    for (f, r, split) in composites() {
        let x = f.honest();
        let Some(i) = (0..f.leaves.len()).find(|i| {
            let c = refute_composite(&f, &x, &r, split, *i as u64);
            matches!(&c.params, PalwTirParamOpeningV1::Composite(c) if c.parent.is_some() && c.adapter.is_some())
        }) else {
            continue;
        };
        let honest = refute_composite(&f, &x, &r, split, i as u64);
        assert_eq!(check_tir_cone_refutation_v1(&honest, &RULES), Err(PalwStepRefuteError::NoFaultFound));
        let c = composite_of(&honest).clone();
        let mut mutations: Vec<(String, PalwTirCompositeOpeningV1)> = Vec::new();
        let mut add = |what: &str, edit: &dyn Fn(&mut PalwTirCompositeOpeningV1)| {
            let mut m = c.clone();
            edit(&mut m);
            mutations.push((what.into(), m));
        };
        let flip = |h: &mut Hash64| {
            let mut b = h.as_bytes();
            b[0] ^= 1;
            *h = Hash64::from_bytes(b);
        };
        add("another parent class", &|m| flip(&mut m.artifact.parent_class));
        add("another parent root", &|m| flip(&mut m.artifact.parent_root));
        add("another adapter root", &|m| flip(&mut m.artifact.adapter_root));
        add("p + 1", &|m| m.artifact.p += 1);
        add("p - 1", &|m| m.artifact.p -= 1);
        add("p past every param", &|m| m.artifact.p = u32::MAX);
        add("the roots swapped", &|m| std::mem::swap(&mut m.artifact.parent_root, &mut m.artifact.adapter_root));
        add("the sections swapped", &|m| std::mem::swap(&mut m.parent, &mut m.adapter));
        add("nothing opened", &|m| {
            m.parent = None;
            m.adapter = None;
        });
        add("the parent section dropped", &|m| m.parent = None);
        add("the adapter section dropped", &|m| m.adapter = None);
        add("an empty parent proof", &|m| m.parent.as_mut().unwrap().opened.clear());
        add("the parent tree's size + 1", &|m| m.parent.as_mut().unwrap().leaf_count += 1);
        add("the adapter tree's size - 1", &|m| m.adapter.as_mut().unwrap().leaf_count -= 1);
        add("an adapter leaf not rebased", &|m| {
            let a = m.adapter.as_mut().unwrap();
            a.opened[0].0 += split;
        });
        add("an adapter leaf moved into the parent section", &|m| {
            let moved = m.adapter.as_mut().unwrap().opened.remove(0);
            m.parent.as_mut().unwrap().opened.push((moved.0 + split, moved.1));
        });
        add("a parent operand's byte changed", &|m| m.parent.as_mut().unwrap().opened[0].1.bytes[0] ^= 1);
        add("an adapter operand's byte changed", &|m| m.adapter.as_mut().unwrap().opened[0].1.bytes[0] ^= 1);
        add("an adapter operand renamed", &|m| m.adapter.as_mut().unwrap().opened[0].1.tensor_name.push('x'));
        add("a parent sibling dropped", &|m| {
            m.parent.as_mut().unwrap().siblings.pop();
        });
        add("an adapter sibling added", &|m| m.adapter.as_mut().unwrap().siblings.push(Hash64::from_bytes([7; 64])));
        for (what, m) in mutations {
            let mut close = honest.clone();
            close.params = PalwTirParamOpeningV1::Composite(Box::new(m));
            let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check_tir_cone_refutation_v1(&close, &RULES)))
                .unwrap_or_else(|_| panic!("{}: {what}: the court panicked", f.name));
            assert!(verdict.is_err(), "{}: {what}: a malformed composite opening convicted an honest executor: {verdict:?}", f.name);
            assert_ne!(verdict, Err(PalwStepRefuteError::NoFaultFound), "{}: {what}: a malformed composite opening acquitted", f.name);
            adjudicated += 1;
        }
    }
    assert!(adjudicated >= 5 * 20, "{adjudicated} malformed openings refused");
}

/// The carriage's first two forms are the `Option` they replaced, byte for byte; the composite
/// reference weighs what the close sizing charges.
#[test]
fn the_carriage_keeps_every_old_close_s_bytes() {
    let f = fixtures().into_iter().next().expect("a fixture");
    let x = f.honest();
    let close = (0..f.leaves.len()).map(|i| refute(&f, &x, i as u64)).find(|c| !c.params.is_none()).expect("a close reading params");
    let proof = close.params.single().cloned().expect("one root");
    assert_eq!(borsh::to_vec(&close.params).unwrap(), borsh::to_vec(&Some(proof.clone())).unwrap());
    assert_eq!(borsh::to_vec(&PalwTirParamOpeningV1::None).unwrap(), borsh::to_vec(&None::<PalwArtifactMultiproofV1>).unwrap());
    let back: Option<PalwArtifactMultiproofV1> = borsh::from_slice(&borsh::to_vec(&close.params).unwrap()).unwrap();
    assert_eq!(back, Some(proof));
    let r = PalwTirCompositeRefV1 { parent_class: PARENT_CLASS, parent_root: PARENT_CLASS, adapter_root: PARENT_CLASS, p: 7 };
    assert_eq!(borsh::to_vec(&r).unwrap().len() as u64, PALW_TIR_COMPOSITE_REF_BYTES_V1);
    // The composite form is tag 2, which an `Option` refuses.
    let composite = PalwTirParamOpeningV1::Composite(Box::new(PalwTirCompositeOpeningV1 { artifact: r, parent: None, adapter: None }));
    let bytes = borsh::to_vec(&composite).unwrap();
    assert_eq!(bytes[0], 2);
    assert!(borsh::from_slice::<Option<PalwArtifactMultiproofV1>>(&bytes).is_err());
}

const SESSION: Hash64 = Hash64::from_bytes([0x5E; 64]);

fn close_bytes(mut proof: kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2) -> u64 {
    use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
    proof.tir_strip_program_v1();
    let object = PalwConsensusObjectV2::CourtClosed { session_id: SESSION, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof };
    borsh::to_vec(&object).unwrap().len() as u64
}

/// **`TirCloseDemandV1` across the sub-roots** (RFC-0004 §6.3): the carried-close bound in the
/// composite form is never below the close the court's own builders make with a composite store —
/// a whole tile's cone close, a dissected leaf's root claim, and its bottom played to the first and
/// to the last child — over every leaf of every corpus composite.
#[test]
fn every_built_composite_close_is_within_its_bound() {
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
    use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1;
    use kaspa_consensus_core::palw_tir_close_size_v1::{
        PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, PalwTirCloseSizingV1, PalwTirParamFormV1, palw_tir_worst_closes_work_v1,
    };
    use kaspa_consensus_core::palw_tir_court_v1::{
        PalwTirInventoryIndexV1, build_tir_dissect_bottom_v1, build_tir_dissect_round_v1, build_tir_root_claim_v1,
    };
    use kaspa_consensus_core::palw_tir_dissect_v1::{
        PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, palw_tir_dissect_site_v1,
    };
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
    let mut measured = 0usize;
    for (f, r, split) in composites() {
        let intervals = f.intervals.as_ref().expect("admissible");
        let x = f.honest();
        let store = CompositeStore { inner: Store { f: &f, x: &x }, r, split };
        let inventory = PalwTirInventoryIndexV1::new(&f.space.program).expect("an inventory");
        let longest = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(
            &f.class,
            f.class_id,
            (1, f.class.layout.max_context),
        )
        .expect("the longest job");
        let sizing = |form| PalwTirCloseSizingV1 { form, court: true, cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, stop_above: None };
        let (bounds, _) =
            palw_tir_worst_closes_work_v1(&f.space, &inventory, &longest, &sizing(PalwTirParamFormV1::Composite { p: r.p }))
                .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let (single, _) = palw_tir_worst_closes_work_v1(&f.space, &inventory, &longest, &sizing(PalwTirParamFormV1::Multiproof))
            .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let bound_of = |block: u8, node: u16| bounds.iter().find(|b| b.block == block && b.node == node).expect("every commit point");
        let mut worst: std::collections::BTreeMap<(u8, u16), (u64, u64)> = Default::default();
        for (i, leaf) in f.leaves.iter().enumerate() {
            let PalwTirLeafKindV1::Commit { block, node, .. } = leaf.kind else { continue };
            let b = bound_of(block, node);
            let index = i as u64;
            let (actual, root_actual) = match (b.dissected, palw_tir_dissect_site_v1(&f.space, intervals, leaf)) {
                (true, Some(site)) if leaf.position >= 1 => {
                    let mut root = build_tir_root_claim_v1(&x.binding, index, &store, &RULES).expect("the root claim");
                    palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
                    let object = PalwConsensusObjectV2::CourtTirRootClaimed {
                        session_id: SESSION,
                        root: Box::new(root.clone()),
                        arity: 2,
                        signature: vec![0; 4_627],
                    };
                    let root_bytes = borsh::to_vec(&object).unwrap().len() as u64;
                    let honest = build_tir_root_claim_v1(&x.binding, index, &store, &RULES).expect("the root claim");
                    let mut close = 0u64;
                    for last in [false, true] {
                        let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &honest, 2, 0, 10).expect("opens");
                        let mut t = 1;
                        while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                            let round =
                                build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("a round");
                            phase.apply_round(&round, t, 10).expect("folds");
                            let child = if last { (phase.child_ranges().len() - 1) as u8 } else { 0 };
                            let choice = PalwTirDissectChoiceV1 {
                                version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                                session_id: SESSION,
                                round: phase.round(),
                                child,
                            };
                            phase.apply_choice(&choice, t + 1, 10).expect("a choice");
                            t += 2;
                        }
                        let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("the bottom");
                        close = close.max(close_bytes(PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom) }));
                    }
                    (close, root_bytes)
                }
                _ => {
                    let refutation = refute_composite(&f, &x, &r, split, index);
                    (close_bytes(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) }), 0)
                }
            };
            assert!(
                actual <= b.close_bytes,
                "{}: leaf {i} (block {block} node {node}): built {actual} B, bound {}",
                f.name,
                b.close_bytes
            );
            assert!(
                root_actual <= b.root_claim_bytes,
                "{}: leaf {i}: root claim {root_actual} B, bound {}",
                f.name,
                b.root_claim_bytes
            );
            let e = worst.entry((block, node)).or_default();
            e.0 = e.0.max(actual);
            e.1 = e.1.max(root_actual);
            measured += 1;
        }
        for ((block, node), (actual, _)) in &worst {
            let (b, s) = (bound_of(*block, *node), single.iter().find(|s| s.block == *block && s.node == *node).expect("sized"));
            eprintln!(
                "{:>24} block {block} node {node:>3}: built ≤ {actual:>7} B, composite bound {:>7} B (one-root {:>7} B, +{})",
                f.name,
                b.close_bytes,
                s.close_bytes,
                b.close_bytes as i64 - s.close_bytes as i64,
            );
        }
    }
    assert!(measured > 100, "{measured} closes measured");
}

/// **Every object that can carry the composite form is named**, for the acceptance walk to drop it
/// below `palw_improvement_v1`: a close, a dissection bottom, a one-move accusation's proof, a root
/// claim's finalize; the one-root form is never named.
#[test]
fn every_carrier_of_a_composite_opening_is_named() {
    use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2 as P;
    use kaspa_consensus_core::palw_improve_composite_v1::palw_object_carries_composite_opening_v1 as carries;
    use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2 as O, PalwCourtVerdictV2};
    use kaspa_consensus_core::palw_tir_dissect_v1::{PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirRangeClaimV1, PalwTirRootClaimV1};
    let (f, r, split) = composites().into_iter().next().expect("a composite");
    let x = f.honest();
    let i = (0..f.leaves.len())
        .find(|i| !refute_composite(&f, &x, &r, split, *i as u64).params.is_none())
        .expect("a close reading params");
    let composite = refute_composite(&f, &x, &r, split, i as u64);
    let plain_f = fixtures().into_iter().find(|g| g.name == f.name).expect("the fixture");
    let plain = refute(&plain_f, &plain_f.honest(), i as u64);
    for (c, named) in [(&composite, true), (&plain, false)] {
        let closed = |proof| O::CourtClosed { session_id: SESSION, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof };
        assert_eq!(carries(&closed(P::TirCone { refutation: Box::new(c.clone()) })), named);
        assert_eq!(carries(&closed(P::TirDissection { bottom: Box::new(c.clone()) })), named);
        let root = PalwTirRootClaimV1 {
            version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            elements: Vec::new(),
            totals: PalwTirRangeClaimV1 { partials: Vec::new() },
            finalize: Box::new(c.clone()),
        };
        assert_eq!(
            carries(&O::CourtTirRootClaimed { session_id: SESSION, root: Box::new(root), arity: 2, signature: Vec::new() }),
            named
        );
    }
}
