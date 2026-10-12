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

// ---------------------------------------------------------------------------------------------
// PALW-TIR-42 (G15): an input is held to its declared interval, not its dtype alone
// ---------------------------------------------------------------------------------------------

/// Every `(pos, input, element)` an honest evaluation of `target` at `ctx` reads, deduplicated.
fn inputs_read(r: &Run, ctx: DemandContext, target: u16, n: usize) -> std::collections::BTreeSet<(u32, u16, usize)> {
    let elements: Vec<usize> = (0..n).collect();
    let mut src = source(r);
    src.base.nodes.remove(&(ctx, target));
    eval_demanded_v2(
        &r.p,
        &r.info,
        &DemandRequest { target: DemandTarget::Node { ctx, node: target }, elements: &elements },
        &mut src,
        &LIMITS,
    )
    .unwrap();
    src.input_requests.iter().copied().collect()
}

/// **A source that answers an input outside its declared interval fails the evaluation `Operand`,
/// by name — wherever the input is read, at both ends of the interval — and the values at the ends
/// themselves are accepted.** The first evaluator held an input to its dtype only: a value of the
/// dtype outside the interval was evaluated, past the bounds admission proved over the declared
/// intervals.
#[test]
fn an_input_outside_its_declared_interval_is_refused_by_name_and_the_ends_are_accepted() {
    let r = denoiser_run();
    let intervals = input_intervals_v2(&r.p);
    let mut by_interval = 0usize;
    let mut by_dtype = 0usize;
    let mut accepted_ends = 0usize;
    for (pos, step) in r.steps.iter().enumerate() {
        for c in &step.commits {
            let ctx = DemandContext { pos: pos as u32, occurrence: commit_occurrence_v2(&r.p, c.block, c.layer) };
            let n = c.value.data.len();
            let elements: Vec<usize> = (0..n).collect();
            let read = inputs_read(&r, ctx, c.node, n);
            // Per input, the first, a middle and the last element it was asked for.
            let mut picked: std::collections::BTreeMap<(u32, u16), Vec<usize>> = Default::default();
            for (p, k, idx) in &read {
                picked.entry((*p, *k)).or_default().push(*idx);
            }
            for ((p, k), idxs) in picked {
                let (lo, hi) = intervals[k as usize];
                let dtype = r.p.inputs[k as usize].dtype;
                for idx in [idxs[0], idxs[idxs.len() / 2], idxs[idxs.len() - 1]] {
                    for (value, in_interval) in [(lo, true), (hi, true), (hi + 1, false), (lo - 1, false)] {
                        let mut src = source(&r);
                        src.base.nodes.remove(&(ctx, c.node));
                        src.inputs.get_mut(&(k, Some(p))).expect("the run's input")[idx] = value;
                        let out = eval_demanded_v2(
                            &r.p,
                            &r.info,
                            &DemandRequest { target: DemandTarget::Node { ctx, node: c.node }, elements: &elements },
                            &mut src,
                            &LIMITS,
                        );
                        if in_interval {
                            assert!(
                                out.is_ok(),
                                "input {k} element {idx} at {p}: the interval's own end {value} is accepted: {out:?}"
                            );
                            accepted_ends += 1;
                        } else {
                            let Err(DemandError::Tir(t)) = out else {
                                panic!("input {k} element {idx} at {p}: {value} outside [{lo}, {hi}] must be refused")
                            };
                            assert_eq!(t.kind, TirErrorKind::Operand, "{t}");
                            if dtype.contains(value) {
                                assert!(t.msg.contains("outside its interval"), "refused by name: {}", t.msg);
                                by_interval += 1;
                            } else {
                                by_dtype += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(accepted_ends > 0 && by_interval > 0, "the toy denoiser has inputs whose interval is narrower than their dtype");
    eprintln!("G15: {accepted_ends} interval ends accepted, {by_interval} refused by interval, {by_dtype} by dtype");
}

/// **The demand bounds hold**: with every element of every input at one end of its declared interval,
/// every commit point of every position recomputes — no `Overflow`, no refusal — to a value inside its
/// node's proven interval (`analyze_ranges_v2`), which is the bound admission proved over the cones.
/// That is what a source holding the interval makes true; the test above is what makes it a refusal
/// when a source does not.
#[test]
fn the_demand_bounds_hold_with_every_input_at_an_end_of_its_interval() {
    let r = denoiser_run();
    let ranges = misaka_palw_tir::interval_v2::analyze_ranges_v2(&r.p).expect("the program's interval analysis");
    let intervals = input_intervals_v2(&r.p);
    for end in [0usize, 1] {
        for (pos, step) in r.steps.iter().enumerate() {
            for c in &step.commits {
                let ctx = DemandContext { pos: pos as u32, occurrence: commit_occurrence_v2(&r.p, c.block, c.layer) };
                let elements: Vec<usize> = (0..c.value.data.len()).collect();
                let mut src = source(&r);
                src.base.nodes.remove(&(ctx, c.node));
                for ((k, _), v) in src.inputs.iter_mut() {
                    let (lo, hi) = intervals[*k as usize];
                    v.iter_mut().for_each(|x| *x = if end == 0 { lo } else { hi });
                }
                let (vals, _) = eval_demanded_v2(
                    &r.p,
                    &r.info,
                    &DemandRequest { target: DemandTarget::Node { ctx, node: c.node }, elements: &elements },
                    &mut src,
                    &LIMITS,
                )
                .unwrap_or_else(|e| panic!("position {pos} node {} with inputs at end {end}: {e:?}", c.node));
                let bound = ranges[c.block as usize][c.node as usize];
                assert!(
                    vals.iter().all(|v| bound.contains(*v)),
                    "position {pos} node {} left its proven interval {bound:?}: {vals:?}",
                    c.node
                );
            }
        }
    }
}

/// A causal program whose history reduction reads an EXTERNAL input in `[0, 8]`: the window of the
/// encoder, each row scaled by the input before the sum over `H`. Its reduction is the range
/// evaluator's target (the history dissection's arithmetic), and the input is read inside it.
fn window_scaled() -> (TirProgramV2, u16) {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::program_v2::{InputSource, OutputDecl};
    use misaka_palw_tir::{DType, Ref};
    let mut pb = ProgramBuilder::new(TOK, HISTORY_BOUND_V1_SMALL);
    let emb = pb.param("w.embed", DType::I16, &[TOK, D], false);
    let scale = pb.param("w.scale", DType::I32, &[], false);
    let hist = pb.hist_state("w.window", DType::I16, &[D], 4, false);
    let (pre, reduce) = {
        let mut b = pb.block("w.pre", vec![]);
        let e = b.gather(emb, Ref::Input(0), 0, 0);
        let w = b.hist_append(hist, e);
        let w32 = b.cast(w, DType::I32);
        let scaled = b.mul(w32, scale, DType::I32);
        let sum = b.reduce_sum(scaled, 0, DType::I32);
        let Ref::Node(reduce) = sum else { panic!("a node") };
        let s = b.reshape_fixed(sum, &[D]);
        let s = b.clamp(s, i16::MIN as i64, i16::MAX as i64, DType::I16);
        (b.finish(&[s]), reduce)
    };
    let b0 = &pb.blocks[pre as usize];
    let carry: Vec<_> = b0.carry_out.iter().map(|n| b0.nodes[*n as usize].out.clone()).collect();
    let (post, row) = {
        let mut b = pb.block("w.post", carry);
        let r = b.cast(Ref::CarryIn(0), DType::I16);
        b.commit(r);
        let Ref::Node(row) = r else { panic!("a node") };
        (b.finish(&[]), row)
    };
    let v1 = pb.finish(pre, vec![], post, row);
    let p = TirProgramV2::from_v1_lifting_params(&v1, &[(1, InputSource::External { lo: 0, hi: 8 })], OutputDecl::Rows { node: row })
        .expect("the program lifts");
    (p, reduce)
}

/// The range evaluator (the history dissection's arithmetic) holds inputs to their intervals too: the
/// reduction over `H` of rows each scaled by an input in `[0, 8]` is evaluated over a range when the
/// input is answered inside its interval, and refused `Operand`, by name, when it is not.
#[test]
fn the_range_evaluator_holds_an_input_to_its_interval() {
    use misaka_palw_tir::demand::DemandRangeRequest;
    use misaka_palw_tir::{DType, Tensor};
    let (p, reduce) = window_scaled();
    let info = validate_v2(&p).unwrap();
    let params = materialize_v2(&p, 100);
    let mut inputs = MapInputs::default();
    inputs.constant.insert(0, Tensor::scalar(DType::I32, 3).unwrap());
    let r = run(p, params, inputs, vec![1, 5, 6, 7, 2]);
    let ctx = DemandContext { pos: 3, occurrence: 0 };
    let elements: Vec<usize> = (0..D as usize).collect();
    let request = DemandRangeRequest { ctx, target: reduce, elements: &elements, supplied: &[], range: Some((0, 4)) };
    let honest = eval_demanded_range_v2(&r.p, &info, &request, &mut source(&r), &LIMITS).expect("an honest input is accepted");
    for bad in [9i128, -1] {
        let mut src = source(&r);
        src.inputs.get_mut(&(0, Some(3))).expect("the scale at position 3")[0] = bad;
        let Err(DemandError::Tir(t)) = eval_demanded_range_v2(&r.p, &info, &request, &mut src, &LIMITS) else {
            panic!("{bad} is outside [0, 8]");
        };
        assert_eq!(t.kind, TirErrorKind::Operand, "{t}");
        assert!(t.msg.contains("outside its interval [0, 8]"), "refused by name: {}", t.msg);
    }
    // The interval's own ends are accepted.
    for end in [0i128, 8] {
        let mut src = source(&r);
        src.inputs.get_mut(&(0, Some(3))).unwrap()[0] = end;
        eval_demanded_range_v2(&r.p, &info, &request, &mut src, &LIMITS)
            .unwrap_or_else(|e| panic!("{end} is the interval's end: {e:?}"));
    }
    let _ = honest;
}
