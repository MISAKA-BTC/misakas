//! **Demand evaluation of version-2 programs** (spec 04b §15.4): every commit point of every position
//! of the toy stages recomputed from committed leaves alone equals the run; the latent written in
//! `post` is a leaf; inputs are read at their position; a refused input fails `Missing`.

mod v2common;

use misaka_palw_tir::TirErrorKind;
use misaka_palw_tir::demand::{DemandContext, DemandError, DemandLimits, DemandRequest, DemandTarget};
use misaka_palw_tir::demand_v2::*;
use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs, StepOutputV2};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::validate_v2::{ProgramInfoV2, validate_v2};
use misaka_palw_tir::{MapParams, RunState};
use v2common::*;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

struct Run {
    p: TirProgramV2,
    info: ProgramInfoV2,
    params: MapParams,
    inputs: MapInputs,
    tokens: Vec<u32>,
    steps: Vec<StepOutputV2>,
}

fn run(p: TirProgramV2, params: MapParams, inputs: MapInputs, tokens: Vec<u32>) -> Run {
    let info = validate_v2(&p).unwrap();
    let interp = InterpreterV2::new(&p).unwrap();
    let mut state = RunState::default();
    let steps = tokens.iter().map(|t| interp.step(&params, &inputs, &mut state, *t).unwrap()).collect();
    Run { p, info, params, inputs, tokens, steps }
}

fn denoiser_run() -> Run {
    let p = denoiser_program();
    let params = materialize_v2(&p, 11);
    let random = GenRandom { seed: [0x2a; 32], position: 0 };
    let mut rng = Lcg(99);
    let inputs = denoiser_inputs(&random, cond_rows(&mut rng), 3, 24, 1, 4);
    run(p, params, inputs, vec![0; 4])
}

fn encoder_run() -> Run {
    let p = encoder_program();
    let params = materialize_v2(&p, 100);
    run(p, params, MapInputs::default(), vec![1, 5, 6, 7, 2, 9])
}

fn source(r: &Run) -> MapSourceV2 {
    MapSourceV2::from_run(&r.p, &r.info, &r.params, &r.inputs, &r.tokens, &r.steps)
}

/// Every commit point of every position, recomputed element by element from committed leaves (its
/// own value never supplied): equal to the run.
fn every_commit_point_recomputes(r: &Run) {
    for (pos, step) in r.steps.iter().enumerate() {
        for c in &step.commits {
            let ctx = DemandContext { pos: pos as u32, occurrence: commit_occurrence_v2(&r.p, c.block, c.layer) };
            let n = c.value.data.len();
            let elements: Vec<usize> = (0..n).collect();
            let mut src = source(r);
            // The target's own value is never read from the source: remove it to be sure.
            src.base.nodes.remove(&(ctx, c.node));
            let (vals, _) = eval_demanded_v2(
                &r.p,
                &r.info,
                &DemandRequest { target: DemandTarget::Node { ctx, node: c.node }, elements: &elements },
                &mut src,
                &LIMITS,
            )
            .unwrap_or_else(|e| panic!("position {pos} {ctx:?} node {}: {e:?}", c.node));
            assert_eq!(vals, c.value.data, "position {pos} {ctx:?} node {}", c.node);
        }
    }
}

#[test]
fn every_denoiser_commit_point_recomputes_from_leaves() {
    every_commit_point_recomputes(&denoiser_run());
}

#[test]
fn every_encoder_commit_point_recomputes_from_leaves() {
    every_commit_point_recomputes(&encoder_run());
}

#[test]
fn the_latent_written_in_post_is_a_leaf_at_the_next_position() {
    let r = denoiser_run();
    let latent = r.p.states.iter().position(|s| s.name == "den.latent").unwrap() as u16;
    let n = LAT as usize;
    let elements: Vec<usize> = (0..n).collect();
    for pos in 0..r.steps.len() as u32 {
        // State after `pos`: the committed post write at `pos`.
        let mut src = source(&r);
        let (vals, work) = eval_demanded_v2(
            &r.p,
            &r.info,
            &DemandRequest { target: DemandTarget::StateAfter { pos, state: latent, layer: None }, elements: &elements },
            &mut src,
            &LIMITS,
        )
        .unwrap();
        assert_eq!(vals, r.steps[pos as usize].output.data, "the write at {pos}");
        assert_eq!((work.elements, work.terms), (0, 0), "a committed write is a leaf: nothing is computed");
    }
    // pre's x at position 2 reads the latent at the start of 2 — the post write at 1, a leaf: the
    // source answers `Replay`, and the evaluation reads node (1, post, write).
    let pre = r.p.schedule.pre;
    let x = pre_carry(&r.p, 0);
    let ctx = DemandContext { pos: 2, occurrence: 0 };
    let mut src = source(&r);
    src.base.nodes.remove(&(ctx, x));
    let (vals, _) = eval_demanded_v2(
        &r.p,
        &r.info,
        &DemandRequest { target: DemandTarget::Node { ctx, node: x }, elements: &elements },
        &mut src,
        &LIMITS,
    )
    .unwrap();
    assert_eq!(vals, r.steps[1].output.data);
    let post_occ = (r.p.schedule.layers.len() + 1) as u16;
    assert!(src.base.requests.iter().any(|q| matches!(q, misaka_palw_tir::demand::MapSourceRequest::Node { ctx, node, .. } if ctx.pos == 1 && ctx.occurrence == post_occ && *node == r.p.output.node())));
    let _ = pre;
}

#[test]
fn inputs_are_read_at_their_position() {
    let r = denoiser_run();
    let post_occ = (r.p.schedule.layers.len() + 1) as u16;
    let write = r.p.output.node();
    for pos in 0..r.steps.len() as u32 {
        let mut src = source(&r);
        let ctx = DemandContext { pos, occurrence: post_occ };
        src.base.nodes.remove(&(ctx, write));
        eval_demanded_v2(
            &r.p,
            &r.info,
            &DemandRequest { target: DemandTarget::Node { ctx, node: write }, elements: &[0] },
            &mut src,
            &LIMITS,
        )
        .unwrap();
        let jitter: Vec<u32> = src.input_requests.iter().filter(|(_, k, _)| *k == IN_JITTER).map(|(p, _, _)| *p).collect();
        assert!(!jitter.is_empty() && jitter.iter().all(|p| *p == pos), "the per-step jitter at {pos}: {jitter:?}");
    }
}

#[test]
fn a_refused_input_fails_missing_and_an_unread_one_is_never_asked() {
    let r = denoiser_run();
    let x = pre_carry(&r.p, 0);
    let n = LAT as usize;
    let elements: Vec<usize> = (0..n).collect();
    // At position 0 x is the noise: withholding it fails the evaluation, whatever reason is given.
    let mut src = source(&r);
    src.withheld_inputs.insert(IN_NOISE, TirErrorKind::Operand);
    let ctx0 = DemandContext { pos: 0, occurrence: 0 };
    src.base.nodes.remove(&(ctx0, x));
    let e = eval_demanded_v2(
        &r.p,
        &r.info,
        &DemandRequest { target: DemandTarget::Node { ctx: ctx0, node: x }, elements: &elements },
        &mut src,
        &LIMITS,
    )
    .unwrap_err();
    assert!(matches!(e, DemandError::Tir(ref t) if t.kind == TirErrorKind::Missing), "{e:?}");
    // At position 2 the Select reads only the latent: the withheld noise is never asked.
    let mut src = source(&r);
    src.withheld_inputs.insert(IN_NOISE, TirErrorKind::Operand);
    let ctx2 = DemandContext { pos: 2, occurrence: 0 };
    src.base.nodes.remove(&(ctx2, x));
    eval_demanded_v2(
        &r.p,
        &r.info,
        &DemandRequest { target: DemandTarget::Node { ctx: ctx2, node: x }, elements: &elements },
        &mut src,
        &LIMITS,
    )
    .unwrap();
    assert!(src.input_requests.iter().all(|(_, k, _)| *k != IN_NOISE));
}
