//! **RFC-0004 A6: the history dissection of an evaluation claim** (spec 17 §17.8.6.3) at the library level — a real
//! dense decoder with grouped-query attention (`consensus-vectors/tir-v1/programs/dense-gqa-2layer.json`) is the
//! SUBJECT of an ExactMatch generation claim, executed for real and bound to its execution root as the fold binds it;
//! its attention leaf — a commit point whose cone reduces over the history, so never tried whole under the held
//! regime — is argued with F7's own phase (`PalwTirDissectPhaseV1`) over the evaluation's context:
//!
//! * the **accusation names the leaf** and carries nothing else (`named_leaf_close`); the court's check says whether it
//!   is dissected (`palw_eval_named_dissected_leaf_v1`);
//! * the **root claim**'s finalize and element closure are admitted (`check_eval_root_claim_v1`), the site the fold
//!   derives is the site the acceptance layer admits (`palw_eval_root_claim_site_v1`);
//! * an **honest responder** is acquitted at the bottom; a responder whose totals lie — the lie hidden in the first or the
//!   last tile, so every fold checks — is convicted where the dissection narrows it.

#[path = "palw_tir_fixture_common.rs"]
mod common;

use common::{TensorSrc, programs};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_gen_court_v1::palw_gen_dissect_site_v1;
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenExecutionV1, palw_gen_execute_v1};
use kaspa_consensus_core::palw_improve_eval_court_v1::*;
use kaspa_consensus_core::palw_improve_eval_v1::*;
use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwConsensusObjectV2, palw_object_is_eval_v1, palw_object_is_gen_v1, palw_object_is_tir_dissection_move_v1, palw_object_is_tir_v1,
};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1;
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirFoldV1,
    PalwTirRangeClaimV1,
};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams};

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 22, max_terms: 1 << 26 };
/// The layout's history tile (`common::layout`): two rows.
const H_TILE: u64 = 2;

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

/// A real evaluation claim of the golden decoder: its run, its binding, the facts the chain holds of it, and its
/// dissected leaf.
struct Claim {
    class_id: Hash64,
    artifact_root: Hash64,
    program_bytes: Vec<u8>,
    layout_digest: Hash64,
    job: PalwEvalJobV1,
    prompt: Vec<u32>,
    params: Params,
    execution: PalwGenExecutionV1,
    binding: PalwEvalBindingV1,
    step_root: Hash64,
    output_root: Hash64,
    /// The stream stage's attention commit point at its last position — the dissected leaf, as a global index.
    leaf: u64,
}

impl Claim {
    fn facts(&self) -> PalwEvalClaimFactsV1<'_> {
        PalwEvalClaimFactsV1 {
            class_id: &self.class_id,
            execution_root: &self.binding.committed_execution_root,
            trace_root: &self.step_root,
            output_root: &self.output_root,
            work_leaves: self.binding.step_leaf_count,
            job: &self.job,
            program: &self.program_bytes,
            layout_digest: self.layout_digest,
            artifact_root: self.artifact_root,
        }
    }

    fn evidence(&self) -> PalwEvalEvidenceV1<'_> {
        PalwEvalEvidenceV1 {
            facts: self.facts(),
            params: &self.params,
            execution: &self.execution,
            binding: &self.binding,
            prompt: &self.prompt,
            composite: None,
        }
    }
}

fn claim() -> Claim {
    let (_, mut program, weights, tokens) =
        programs().into_iter().find(|(name, ..)| name == "dense-gqa-2layer").expect("the golden decoder");
    // The tiled logits scheme the evaluation's score and decode read, as the IR fixture does.
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let program = TirProgramV1::decode_canonical(&program.encode()).expect("still canonical under the tiled scheme");
    let layout = common::layout(&program, 12);
    let (artifact_root, _) = palw_tir_inventory_root_v1(&program, &TensorSrc(&weights)).expect("an inventory root");
    let class_id = Hash64::from_bytes([0x33; 64]);
    let seed = palw_improve_eval_seed_v1(&Hash64::from_bytes([0x44; 64]), 7);
    let job = PalwEvalJobV1 {
        line_id: Hash64::from_bytes([0x11; 64]),
        epoch: 3,
        item: 7,
        subject: PalwEvalSubjectV1::Candidate(Hash64::from_bytes([0x22; 64])),
        kind: PalwScoringKindV1::ExactMatch,
        part: 0,
        mode: PalwEvalModeV1::Generate { seed, max_new: 4, stop_ids: vec![] },
    };
    let stage = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
    let subject = PalwEvalSubjectClassV1 { class_id, artifact_root, program: &program, layout: &layout };
    let ctx = palw_improve_eval_context_v1(&job, &subject, stage).expect("the context derives");
    let prompt: Vec<u32> = tokens.iter().take(3).copied().collect();
    let params = Params(vec![weights]);
    let probe = PipelineJob { prompt: prompt.clone(), scalars: ctx.scalars.clone(), ..PipelineJob::default() };
    let decode = ctx.decode.clone().expect("a generating job decodes");
    let execution =
        palw_gen_execute_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &probe, &decode, ctx.seed).expect("an honest run");
    let binding =
        PalwEvalBindingV1::of(&job, class_id, &layout, &execution.claim, execution.space.leaf_count(), &prompt, stage, vec![], vec![]);
    let sp = &execution.space.stages[0];
    let dissected = sp
        .leaves()
        .iter()
        .rfind(|l| palw_gen_dissect_site_v1(sp, &l.coord).is_some())
        .expect("the attention's commit points are dissected");
    let leaf = execution.space.global_index(&dissected.coord).expect("a leaf of the space");
    Claim {
        class_id,
        artifact_root,
        program_bytes: program.encode(),
        layout_digest: palw_improve_eval_layout_digest_v1(&layout),
        job,
        prompt,
        params,
        step_root: binding.step_root(),
        output_root: binding.generated_root(),
        execution,
        binding,
        leaf,
    }
}

/// Play the dissection from the phase `phase` to its bottom: each round the responder posts `children_of`'s children,
/// the challenger names `choose`'s child.
fn play(
    mut phase: PalwTirDissectPhaseV1,
    mut children_of: impl FnMut(&PalwTirDissectPhaseV1) -> Vec<PalwTirRangeClaimV1>,
    mut choose: impl FnMut(&PalwTirDissectPhaseV1, &[PalwTirRangeClaimV1]) -> u8,
) -> PalwTirDissectPhaseV1 {
    let mut daa = 10u64;
    while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
        let children = children_of(&phase);
        let round = PalwTirDissectRoundV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, children: children.clone() };
        phase.apply_round(&round, daa, 100).unwrap_or_else(|e| panic!("round {}: {e}", phase.round()));
        daa += 1;
        let child = choose(&phase, &children);
        let choice = PalwTirDissectChoiceV1 {
            version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            session_id: phase.session_id(),
            round: phase.round(),
            child,
        };
        phase.apply_choice(&choice, daa, 100).unwrap_or_else(|e| panic!("choice: {e}"));
        daa += 1;
    }
    assert_eq!(phase.turn(), PalwBisectTurnV1::Terminal);
    phase
}

#[test]
fn an_accusation_names_a_dissected_leaf_and_the_court_says_so() {
    let c = claim();
    let (ev, facts) = (c.evidence(), c.facts());
    let named = ev.named_leaf_close(c.leaf).expect("a named leaf");
    assert!(named.operands.is_empty() && named.prompt_ids.is_empty(), "the leaf and nothing else");
    assert_eq!(
        palw_eval_named_dissected_leaf_v1(&named, &facts),
        Ok(Some(c.leaf)),
        "a dissected leaf: its cone reduces over the history"
    );
    // The first leaf (the embedding) is closed, not dissected.
    let closed = ev.named_leaf_close(0).expect("a named leaf");
    assert_eq!(palw_eval_named_dissected_leaf_v1(&closed, &facts), Ok(None), "a closed leaf is no dissection");
    // A leaf that is not under its stage's root names nothing.
    let mut off_root = named.clone();
    off_root.disputed.lanes_le[0] ^= 1;
    assert!(palw_eval_named_dissected_leaf_v1(&off_root, &facts).is_err(), "not under its stage's root");
    // A binding that is not the claim's names nothing.
    let other_root = Hash64::from_bytes([0x66; 64]);
    let mut other_facts = c.facts();
    other_facts.execution_root = &other_root;
    assert!(palw_eval_named_dissected_leaf_v1(&named, &other_facts).is_err(), "another claim's binding");
}

#[test]
fn the_root_claim_is_admitted_at_the_named_leaf_and_the_fold_derives_the_same_site() {
    let c = claim();
    let (ev, facts) = (c.evidence(), c.facts());
    let root = ev.root_claim(c.leaf, &LIMITS).expect("a root claim");
    assert!(root.elements.iter().any(|l| !l.is_empty()), "the finalize reads the reductions");
    let site = check_eval_root_claim_v1(&root, &facts, c.leaf, &LIMITS).expect("the honest root claim is admitted");
    let derived = palw_eval_root_claim_site_v1(&root, &facts, c.leaf).expect("the fold's site");
    assert_eq!(site, derived, "the acceptance layer and the fold derive one site");
    assert!(check_eval_root_claim_v1(&root, &facts, c.leaf + 1, &LIMITS).is_err(), "another leaf");
    assert!(palw_eval_root_claim_site_v1(&root, &facts, c.leaf + 1).is_err(), "another leaf, at the fold");
    eprintln!(
        "evaluation attention leaf {}: {} reductions {:?} ({:?}), H = {}, {} claimed values",
        c.leaf,
        site.reductions.len(),
        site.reductions,
        site.folds,
        site.history_positions,
        root.elements.iter().map(Vec::len).sum::<usize>()
    );

    // The claim is exactly the closure: a value the dissection never reads is refused, and so is a claim missing one.
    let r = root.elements.iter().position(|l| !l.is_empty()).unwrap();
    let unread = (0..site.counts[r] as u32).find(|e| !root.elements[r].contains(e)).expect("an element the tile does not read");
    let mut more = root.clone();
    let at = more.elements[r].partition_point(|e| *e < unread);
    more.elements[r].insert(at, unread);
    more.totals.partials[r].insert(at, root.totals.partials[r][0]);
    let extra = check_eval_root_claim_v1(&more, &facts, c.leaf, &LIMITS);
    assert!(matches!(&extra, Err(why) if why.contains("never reads")), "{extra:?}");
    let mut fewer = root.clone();
    fewer.elements[r].remove(0);
    fewer.totals.partials[r].remove(0);
    assert!(check_eval_root_claim_v1(&fewer, &facts, c.leaf, &LIMITS).is_err());

    // A finalize that is not the claim's: its binding names another job.
    let mut elsewhere = root.clone();
    let other_job = PalwEvalJobV1 { item: 8, ..c.job.clone() };
    let mut other_facts = c.facts();
    other_facts.job = &other_job;
    assert!(check_eval_root_claim_v1(&elsewhere, &other_facts, c.leaf, &LIMITS).is_err(), "another job's finalize");
    elsewhere.version += 1;
    assert!(check_eval_root_claim_v1(&elsewhere, &facts, c.leaf, &LIMITS).is_err(), "another phase version");

    // The object: a dissection move of the evaluation court, appended after §17.0's last tag, signed by the responder.
    let session = Hash64::from_bytes([5; 64]);
    let object = PalwConsensusObjectV2::CourtEvalRootClaimed {
        session_id: session,
        root: Box::new(root.clone()),
        arity: 2,
        signature: vec![1],
    };
    assert!(
        borsh::to_vec(&object).unwrap()[0] > 82,
        "appended after the last tag of §17.0's table; the integration assigns its number"
    );
    assert!(
        palw_object_is_eval_v1(&object) && palw_object_is_tir_dissection_move_v1(&object),
        "an evaluation court move, a dissection move"
    );
    assert!(!palw_object_is_gen_v1(&object) && !palw_object_is_tir_v1(&object), "and neither the generative court's nor the IR's");
    assert!(
        kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_object_v1(&object),
        "a court move a halt must let through"
    );
    assert!(kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).is_ok());
    let unsigned = PalwConsensusObjectV2::CourtEvalRootClaimed {
        session_id: session,
        root: Box::new(root.clone()),
        arity: 2,
        signature: Vec::new(),
    };
    assert!(
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&unsigned).is_err(),
        "unsigned: the challenger could write it"
    );
    let message = palw_eval_root_claim_message_v1(&session, &root);
    assert_ne!(message, palw_eval_root_claim_message_v1(&Hash64::from_bytes([6; 64]), &root), "the session");
    assert_ne!(message, palw_eval_root_claim_message_v1(&session, &more), "the claim");
}

#[test]
fn an_honest_dissection_of_an_evaluation_claims_attention_leaf_acquits() {
    let c = claim();
    let (ev, facts) = (c.evidence(), c.facts());
    let root = ev.root_claim(c.leaf, &LIMITS).expect("a root claim");
    let site = check_eval_root_claim_v1(&root, &facts, c.leaf, &LIMITS).expect("admitted");
    let phase = PalwTirDissectPhaseV1::open_parts(
        Hash64::from_bytes([5; 64]),
        c.leaf,
        &site,
        root.version,
        &root.elements,
        &root.totals,
        2,
        1,
        100,
    )
    .expect("the phase opens");
    let phase = play(phase, |p| ev.round(p, &LIMITS).expect("honest children").children, |_, _| 0);
    let bottom = ev.bottom(&phase, &LIMITS).expect("the bottom close");
    assert_eq!(
        check_eval_dissect_bottom_v1(&phase, &bottom, &facts, c.leaf, &LIMITS),
        Ok(None),
        "an honest responder is acquitted at the bottom"
    );
    assert!(
        matches!(
            check_eval_dissect_bottom_v1(&phase, &bottom, &facts, c.leaf + 1, &LIMITS),
            Err(PalwEvalCourtErrorV1::NotTheNarrowedLeaf { .. })
        ),
        "a bottom is of the leaf the ladder narrowed to"
    );
}

#[test]
fn a_lie_in_the_totals_of_an_evaluation_claims_dissection_is_convicted_wherever_it_hides() {
    let c = claim();
    let (ev, facts) = (c.evidence(), c.facts());
    let root = ev.root_claim(c.leaf, &LIMITS).expect("a root claim");
    let site = check_eval_root_claim_v1(&root, &facts, c.leaf, &LIMITS).expect("admitted");
    // The last sum reduction's first claimed element (no other reduction reads it, so the rest of the claim stays
    // honest), one more than it is: the lie the responder carries.
    let r = (0..site.folds.len())
        .rev()
        .find(|i| site.folds[*i] == PalwTirFoldV1::Sum && !root.elements[*i].is_empty())
        .expect("a sum reduction");
    let tiles = (site.history_positions as u64).div_ceil(H_TILE);
    for hide in [0, tiles - 1] {
        let mut lying = root.totals.clone();
        lying.partials[r][0] += 1;
        let phase = PalwTirDissectPhaseV1::open_parts(
            Hash64::from_bytes([6; 64]),
            c.leaf,
            &site,
            root.version,
            &root.elements,
            &lying,
            2,
            1,
            100,
        )
        .expect("the phase opens on the lying totals (its finalize is the acceptance layer's)");
        // The responder hides the lie in the child holding tile `hide`, so every fold checks.
        let children_of = |p: &PalwTirDissectPhaseV1| {
            let mut children = ev.round(p, &LIMITS).expect("honest children").children;
            let ranges = p.child_ranges();
            let at = ranges.iter().position(|(first, count)| (*first..first + count).contains(&hide)).unwrap_or(0);
            children[at].partials[r][0] += 1;
            children
        };
        // The challenger names the child its own partials disagree with.
        let choose = |p: &PalwTirDissectPhaseV1, children: &[PalwTirRangeClaimV1]| -> u8 {
            let honest = ev.round(p, &LIMITS).expect("honest children").children;
            children.iter().zip(&honest).position(|(c, h)| c != h).expect("the lie is in one child") as u8
        };
        let phase = play(phase, children_of, choose);
        let bottom = ev.bottom(&phase, &LIMITS).expect("the bottom close");
        let verdict = check_eval_dissect_bottom_v1(&phase, &bottom, &facts, c.leaf, &LIMITS);
        assert!(matches!(verdict, Ok(Some(_))), "the lie in tile {hide}: {verdict:?}");
    }
}
