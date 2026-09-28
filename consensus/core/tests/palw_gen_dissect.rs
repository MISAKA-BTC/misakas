//! **RFC-0002 F7 composed into the generative court: a real VLM language model's attention,
//! dissected** — tir-lower's lowered tiny LLaVA (`consensus-vectors/tir-v2/pipelines/vlm-llava-tiny.json`,
//! `rfc3/lower` 4c25416a5) run on an FP Job V5; its text stage's dissected leaf argued end to end with
//! F7's own phase (`PalwTirDissectPhaseV1`): the root claim's finalize and element closure, the rounds
//! of partials (each child fold-checked by the phase), the challenger's choices, and the bottom.
//!
//! An honest responder is acquitted at the bottom; a responder whose totals lie — the lie spread over
//! the children so every fold checks — is convicted where the dissection narrows it, whichever
//! child it hides in. The same root claim rides as the chain's object (tag 68), and the site the fold
//! derives is the site the acceptance layer's finalize admits.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_job_v5::PalwFreePromptJobV5;
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFreePromptJobV3};
use kaspa_consensus_core::palw_gen_artifact_v1::{PalwGenInventoryIndexV1, palw_gen_inventory_root_v1, palw_gen_open_leaves_v1};
use kaspa_consensus_core::palw_gen_class_v1::*;
use kaspa_consensus_core::palw_gen_close_v1::*;
use kaspa_consensus_core::palw_gen_court_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::PalwGenLeafKindV1;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_gen_worker_v1::{PalwGenDecodeV1, PalwGenExecutionV1, palw_gen_execute_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirFoldV1,
    PalwTirRangeClaimV1,
};
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{JobImageV1, PipelineJob, PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::tensor::Tensor;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 22, max_terms: 1 << 26 };
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
const H_TILE: u32 = 8;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

struct Fixture {
    class: PalwGenClassV1,
    row: PalwGenClassRecordV1,
    pipeline: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: Params,
    image: JobImageV1,
    prompt: Vec<u32>,
}

fn fixture() -> Fixture {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v2/pipelines/vlm-llava-tiny.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
    let program_bytes: Vec<Vec<u8>> =
        v["programs"].as_array().unwrap().iter().map(|p| unhex(p["program_borsh_hex"].as_str().unwrap())).collect();
    let programs: Vec<TirProgramV2> = program_bytes.iter().map(|b| TirProgramV2::decode_canonical(b).unwrap()).collect();
    let pipeline_bytes = unhex(v["pipeline_borsh_hex"].as_str().unwrap());
    let pipeline = TirPipelineV1::decode_canonical(&pipeline_bytes, &programs).unwrap();
    let params = Params(
        v["programs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&programs)
            .map(|(pj, prog)| {
                let mut m = MapParams::default();
                for e in pj["params"].as_array().unwrap() {
                    let j = e["param"].as_u64().unwrap() as u16;
                    let decl = &prog.params[j as usize];
                    let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
                    let layer = e["layer"].as_u64().map(|l| l as u16);
                    m.tensors
                        .insert((j, layer), Tensor::from_le_bytes(decl.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).unwrap());
                }
                m
            })
            .collect(),
    );
    let layouts = pipeline
        .stages
        .iter()
        .map(|st| {
            let p = &programs[st.program as usize];
            let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
            PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: st.max_trip,
                checkpoint_interval: 4,
                h_tile: H_TILE,
                commit_tiles: vec![16; commits],
                state_tiles: vec![16; p.states.len()],
            }
        })
        .collect();
    let img = &v["job"]["images"][0];
    let image = JobImageV1 {
        h: img["h"].as_u64().unwrap() as u32,
        w: img["w"].as_u64().unwrap() as u32,
        rgb: unhex(img["rgb_hex"].as_str().unwrap()),
    };
    let prompt: Vec<u32> = v["job"]["prompt"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as u32).collect();
    let class = PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: pipeline_bytes,
        programs: program_bytes,
        layouts,
        output: OutputSpecV1::tokens(pipeline.stages[pipeline.output_stage as usize].max_trip),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: 32,
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h: image.h, w: image.w, tile_len: 64, token_equivalents: 1_000_000 }],
        },
        tokenizer_id: Hash64::from_bytes([0x74; 64]),
    };
    let (root, _) = palw_gen_inventory_root_v1(&programs, &params).unwrap();
    let row = palw_gen_class_record_v1(&class, &root).unwrap();
    Fixture { class, row, pipeline, programs, params, image, prompt }
}

fn v5_job(f: &Fixture) -> PalwFreePromptJobV5 {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/fp-v4/job_v4_encoding.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut v4: PalwFreePromptJobV3 = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| borsh::from_slice::<PalwFreePromptJobV3>(&unhex(c["borsh_hex"].as_str().unwrap())).unwrap())
        .find(|j| j.version == PALW_FP_V4_VERSION)
        .unwrap();
    v4.class_id = f.row.class_id;
    v4.tokenizer_id = f.class.tokenizer_id;
    v4.prompt_tokens = f.prompt.len() as u32;
    v4.prompt_token_ids_hash = prompt_token_ids_commitment_v1(FORM, &f.prompt).unwrap();
    v4.decode_token_limit = 8;
    v4.decode = Some(DecodeConfigV4::NOOP);
    v4.temperature_q = PalwDecodeSamplingV2::GREEDY.temperature_q;
    let input_root = misaka_palw_gen::output::input_image_root_v1(f.image.h, f.image.w, 64, &f.image.rgb).unwrap();
    PalwFreePromptJobV5 {
        v4,
        images: vec![PalwGenImageInputRefV1 { input_root: Hash64::from_bytes(input_root), h: f.image.h, w: f.image.w }],
    }
}

/// The run, its binding, and the claim's dissected leaf: the text stage's dissected commit point
/// at the stream's last position, first tile.
fn execute(f: &Fixture) -> (PalwFreePromptJobV5, PalwGenExecutionV1, PalwGenStepBindingV1, u64) {
    let job = v5_job(f);
    let run_job = PipelineJob { prompt: f.prompt.clone(), images: vec![f.image.clone()], ..PipelineJob::default() };
    let decode = PalwGenDecodeV1::of(&job).unwrap();
    let e =
        palw_gen_execute_v1(&f.pipeline, &f.programs, &f.class.layouts, &f.params, &run_job, &decode, job.v4.sampling_seed).unwrap();
    let binding = PalwGenStepBindingV1::of(&job, &e.claim, e.space.leaf_count());
    let (stage, block, node) = f.row.dissected[0];
    let sp = &e.space.stages[stage as usize];
    let last = sp.trip - 1;
    let leaf = sp
        .leaves()
        .iter()
        .find(|l| {
            l.coord.pos == last
                && matches!(l.coord.kind, PalwGenLeafKindV1::Commit { occurrence, node: n } if n == node && sp.occurrence_block(occurrence) == Some(block))
        })
        .expect("the dissected point's leaf at the last position");
    let index = e.space.global_index(&leaf.coord).unwrap();
    (job, e, binding, index)
}

/// The court's view of the claim (the binding verified against the registry) and the cone close of
/// the leaf at global index `index`: every leaf before it, every image tile and every param leaf.
fn cone(f: &Fixture, e: &PalwGenExecutionV1, binding: &PalwGenStepBindingV1, index: u64) -> PalwGenConeCloseV1 {
    let (s, i) = e.space.locate(index).unwrap();
    let wire = |s: usize, i: usize| PalwGenLeafOpeningV1::of(&e.space, &e.open(s as u8, i as u64).unwrap()).unwrap();
    let mut operands = Vec::new();
    for st in 0..=s as usize {
        let n = if st == s as usize { i as usize } else { e.space.stages[st].leaves().len() };
        operands.extend((0..n).map(|k| wire(st, k)));
    }
    let tiles = misaka_palw_gen::output::input_image_tiles_v1(f.image.h, f.image.w, 64, &f.image.rgb).unwrap();
    let image_tiles = tiles
        .into_iter()
        .enumerate()
        .map(|(t, (bytes, proof))| PalwGenImageTileV1 { image: 0, tile: t as u64, bytes, proof })
        .collect();
    let count = PalwGenInventoryIndexV1::new(&f.programs).unwrap().leaf_count();
    let reads = palw_gen_stage_reads_prompt_v1(&f.pipeline, s as usize);
    PalwGenConeCloseV1 {
        version: PALW_GEN_CLOSE_VERSION_V1,
        binding: binding.clone(),
        prompt_ids: if reads { f.prompt.clone() } else { vec![] },
        disputed: wire(s as usize, i as usize),
        operands,
        image_tiles,
        params: palw_gen_open_leaves_v1(&f.programs, &f.params, 0..count).unwrap(),
    }
}

/// The court's case and the in-memory close of a wire close.
fn with_case<T>(f: &Fixture, close: &PalwGenConeCloseV1, run: impl FnOnce(&PalwGenCourtCaseV1<'_>, &PalwGenCloseV1) -> T) -> T {
    let v =
        match verify_gen_binding_v1(&close.binding, &f.row, &f.row.class_id, &close.binding.committed_execution_root, Some(&f.prompt))
            .unwrap()
        {
            PalwGenBindingOutcomeV1::Verified(v) => v,
            PalwGenBindingOutcomeV1::Convicted(fault) => panic!("the binding convicts: {fault:?}"),
        };
    let opened = |o: &PalwGenLeafOpeningV1| o.opened(&v.space).unwrap();
    let mem = PalwGenCloseV1 {
        disputed: opened(&close.disputed),
        operands: close.operands.iter().map(opened).collect(),
        image_tiles: close.image_tiles.clone(),
        params: close.params.clone(),
    };
    run(&v.case(&f.row), &mem)
}

/// Play the dissection from `root` to its bottom: each round the responder posts `children_of`'s
/// children, the challenger names `choose`'s child. Returns the terminal phase.
fn play(
    f: &Fixture,
    close: &PalwGenConeCloseV1,
    mut phase: PalwTirDissectPhaseV1,
    mut children_of: impl FnMut(&PalwTirDissectPhaseV1) -> Vec<PalwTirRangeClaimV1>,
    mut choose: impl FnMut(&PalwTirDissectPhaseV1, &[PalwTirRangeClaimV1]) -> u8,
) -> PalwTirDissectPhaseV1 {
    let _ = (f, close);
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

/// The honest partials of every child of the phase's disputed range.
fn honest_children(f: &Fixture, close: &PalwGenConeCloseV1, phase: &PalwTirDissectPhaseV1) -> Vec<PalwTirRangeClaimV1> {
    let h = phase.history_positions() as u64;
    phase
        .child_ranges()
        .into_iter()
        .map(|(first, count)| {
            let from = first * H_TILE as u64;
            let to = ((first + count) * H_TILE as u64).min(h);
            with_case(f, close, |case, mem| palw_gen_dissect_partials_v1(case, mem, phase, (from as usize, to as usize), &LIMITS))
                .expect("adjudicable")
                .expect("no conviction on the carriage")
        })
        .collect()
}

#[test]
fn an_honest_dissection_of_a_real_vlm_attention_leaf_acquits() {
    let f = fixture();
    assert_eq!(f.row.dissected.len(), 1, "one dissected point: the attention of the LM's layer");
    let (_, e, binding, index) = execute(&f);
    let close = cone(&f, &e, &binding, index);
    let (elements, totals) =
        with_case(&f, &close, |case, mem| palw_gen_build_root_claim_v1(case, mem, &LIMITS)).expect("a root claim");
    assert!(elements.iter().any(|l| !l.is_empty()), "the finalize reads the reductions");
    let site = with_case(&f, &close, |case, mem| palw_gen_check_root_claim_v1(case, mem, &elements, &totals, &LIMITS))
        .unwrap_or_else(|e| panic!("the honest root claim is admitted: {e}"));
    eprintln!(
        "LLaVA attention leaf {index}: {} reductions {:?} ({:?}), H = {}, {} claimed values",
        site.reductions.len(),
        site.reductions,
        site.folds,
        site.history_positions,
        elements.iter().map(Vec::len).sum::<usize>()
    );
    // The chain's object and the two derivations of its site.
    let root = PalwGenRootClaimV1 {
        version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
        elements: elements.clone(),
        totals: totals.clone(),
        finalize: Box::new(close.clone()),
    };
    let exec = binding.committed_execution_root;
    let admitted = check_gen_root_claim_v1(&root, &f.row, &f.row.class_id, &exec, index, FORM, &LIMITS).expect("admitted");
    let derived = palw_gen_root_claim_site_v1(&root, &f.row, &f.row.class_id, &exec, index).expect("the fold's site");
    assert_eq!(admitted, derived, "the acceptance layer and the fold derive one site");
    assert!(check_gen_root_claim_v1(&root, &f.row, &f.row.class_id, &exec, index + 1, FORM, &LIMITS).is_err(), "another leaf");
    let object = PalwConsensusObjectV2::CourtGenRootClaimed {
        session_id: Hash64::from_bytes([5; 64]),
        root: Box::new(root),
        arity: 2,
        signature: vec![1],
    };
    assert_eq!(borsh::to_vec(&object).unwrap()[0], 68, "tag 68, after the generative registration's 67");
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_gen_v1(&object));
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_tir_dissection_move_v1(&object), "a dissection move: the k-ary fence");
    // The claim is exactly the closure: a value the dissection never reads is refused, and so is a
    // claim missing one it reads.
    let r = elements.iter().position(|l| !l.is_empty()).unwrap();
    let unread = (0..site.counts[r] as u32).find(|e| !elements[r].contains(e)).expect("an element the tile does not read");
    let (mut more, mut more_totals) = (elements.clone(), totals.clone());
    let at = more[r].partition_point(|e| *e < unread);
    more[r].insert(at, unread);
    more_totals.partials[r].insert(at, totals.partials[r][0]);
    let extra = with_case(&f, &close, |case, mem| palw_gen_check_root_claim_v1(case, mem, &more, &more_totals, &LIMITS));
    assert!(matches!(&extra, Err(why) if why.contains("never reads")), "{extra:?}");
    let (mut fewer, mut fewer_totals) = (elements.clone(), totals.clone());
    fewer[r].remove(0);
    fewer_totals.partials[r].remove(0);
    assert!(with_case(&f, &close, |case, mem| palw_gen_check_root_claim_v1(case, mem, &fewer, &fewer_totals, &LIMITS)).is_err());
    // The honest phase, played to its bottom: acquitted.
    let phase = PalwTirDissectPhaseV1::open_parts(
        Hash64::from_bytes([5; 64]),
        index,
        &site,
        PALW_TIR_DISSECT_OBJECT_VERSION_V1,
        &elements,
        &totals,
        2,
        1,
        100,
    )
    .expect("the phase opens");
    let phase = play(&f, &close, phase, |p| honest_children(&f, &close, p), |_, _| 0);
    let verdict = with_case(&f, &close, |case, mem| palw_gen_check_dissect_bottom_v1(case, mem, &phase, &LIMITS));
    assert_eq!(verdict, Ok(PalwGenVerdictV1::Acquitted));
    // And through the consensus-level check of the bottom close (tag 12's).
    assert_eq!(check_gen_dissect_bottom_v1(&phase, &close, &f.row, &f.row.class_id, &exec, index, FORM, &LIMITS), Ok(None));
}

#[test]
fn a_lie_in_the_totals_is_convicted_wherever_it_hides() {
    let f = fixture();
    let (_, e, binding, index) = execute(&f);
    let close = cone(&f, &e, &binding, index);
    let (elements, totals) = with_case(&f, &close, |case, mem| palw_gen_build_root_claim_v1(case, mem, &LIMITS)).unwrap();
    let site = with_case(&f, &close, |case, mem| palw_gen_check_root_claim_v1(case, mem, &elements, &totals, &LIMITS)).unwrap();
    // The last sum reduction's first claimed element (the value sum: no other reduction reads it, so
    // the rest of the claim stays honest), one more than it is: the lie the responder carries.
    let r = (0..site.folds.len()).rev().find(|i| site.folds[*i] == PalwTirFoldV1::Sum && !elements[*i].is_empty()).expect("a sum");
    let tiles = (site.history_positions as u64).div_ceil(H_TILE as u64);
    for hide in [0, tiles - 1] {
        let mut lying = totals.clone();
        lying.partials[r][0] += 1;
        let phase = PalwTirDissectPhaseV1::open_parts(
            Hash64::from_bytes([6; 64]),
            index,
            &site,
            PALW_TIR_DISSECT_OBJECT_VERSION_V1,
            &elements,
            &lying,
            2,
            1,
            100,
        )
        .expect("the phase opens on the lying totals (its finalize is the acceptance layer's)");
        // The responder hides the lie in the child holding tile `hide`, so every fold checks.
        let children_of = |p: &PalwTirDissectPhaseV1| {
            let mut children = honest_children(&f, &close, p);
            let ranges = p.child_ranges();
            let at = ranges.iter().position(|(first, count)| (*first..first + count).contains(&hide)).unwrap_or(0);
            children[at].partials[r][0] += 1;
            children
        };
        // The challenger names the child its own partials disagree with.
        let choose = |p: &PalwTirDissectPhaseV1, children: &[PalwTirRangeClaimV1]| -> u8 {
            let _ = p;
            let honest = honest_children(&f, &close, p);
            children.iter().zip(&honest).position(|(c, h)| c != h).expect("the lie is in one child") as u8
        };
        let phase = play(&f, &close, phase, children_of, choose);
        let verdict = with_case(&f, &close, |case, mem| palw_gen_check_dissect_bottom_v1(case, mem, &phase, &LIMITS));
        assert!(matches!(verdict, Ok(PalwGenVerdictV1::Convicted { .. })), "the lie in tile {hide}: {verdict:?}");
        let exec = binding.committed_execution_root;
        assert!(
            matches!(check_gen_dissect_bottom_v1(&phase, &close, &f.row, &f.row.class_id, &exec, index, FORM, &LIMITS), Ok(Some(_))),
            "tag 12's check convicts too"
        );
    }
}
