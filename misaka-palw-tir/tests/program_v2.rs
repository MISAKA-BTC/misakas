//! **Program version 2** (RFC-0003 §I.2.3, spec 04b §15): version 1 unchanged, version 2's view
//! faithful, the `post` write, the inputs' declarations, the normal-form refusals, and the ranges.

mod common;
mod v2common;

use common::models::dense;
use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::interval_v2::{analyze_ranges_v2, output_interval_v2};
use misaka_palw_tir::pipeline::RandomSource;
use misaka_palw_tir::prim::Prim;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, StateKind};
use misaka_palw_tir::program_v2::*;
use misaka_palw_tir::validate_v2::validate_v2;
use misaka_palw_tir::{ConeEnv, DType, Interpreter, MapParams, Ref, RunState, Tensor, TirErrorKind, TirProgramV1};
use v2common::*;

fn kind(p: &TirProgramV2) -> Option<TirErrorKind> {
    validate_v2(p).err().map(|e| e.kind)
}

// ---- Version 1 is untouched --------------------------------------------------------------------------

#[test]
fn a_version_1_program_decodes_through_the_dispatcher_exactly_as_before() {
    for windows in [&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL][..], &[3, HISTORY_BOUND_V1_SMALL, 3][..]] {
        let (p, _) = dense(windows);
        let bytes = p.encode();
        let direct = TirProgramV1::decode_canonical(&bytes).unwrap();
        assert_eq!(TirProgram::decode_canonical(&bytes).unwrap(), TirProgram::V1(direct));
        assert_eq!(
            TirProgramV2::decode_canonical(&bytes).unwrap_err().kind,
            TirErrorKind::NormalForm,
            "a v1 program is not a v2 program"
        );
    }
    assert_eq!(TirProgram::decode_canonical(&[3, 0, 1]).unwrap_err().kind, TirErrorKind::NormalForm, "version 3 is refused");
    assert_eq!(TirProgram::decode_canonical(&[1]).unwrap_err().kind, TirErrorKind::Encoding);
}

#[test]
fn a_version_2_program_round_trips_its_canonical_bytes() {
    for p in [encoder_program(), denoiser_program(), decoder_program()] {
        let bytes = p.encode();
        assert_eq!(bytes[..2], [2, 0], "the version prefix");
        assert_eq!(TirProgramV2::decode_canonical(&bytes).unwrap(), p);
        assert_eq!(TirProgram::decode_canonical(&bytes).unwrap(), TirProgram::V2(p.clone()));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(TirProgramV2::decode_canonical(&trailing).unwrap_err().kind, TirErrorKind::Encoding);
        assert_eq!(TirProgramV2::decode_canonical(&bytes[..bytes.len() - 1]).unwrap_err().kind, TirErrorKind::Encoding);
        let as_v1 = TirProgramV1::decode_canonical(&bytes).unwrap_err().kind;
        assert!(matches!(as_v1, TirErrorKind::NormalForm | TirErrorKind::Encoding), "a v2 program is not a v1 program: {as_v1:?}");
    }
}

// ---- The view is faithful ----------------------------------------------------------------------------

/// Lifting params into external inputs changes no value: the same decoder, run as version 1 with its
/// embedding and a RoPE table as params and as version 2 with them as inputs, commits the same bytes
/// at every commit point of every position.
#[test]
fn lifting_params_into_inputs_changes_no_value() {
    let (p1, gens) = dense(&[3, HISTORY_BOUND_V1_SMALL, 3]);
    let params1 = common::models::materialize(&p1, &gens, 7);
    let lift = |name: &str| p1.param_index(name).unwrap();
    let (tok, cos) = (lift("tok_embd"), lift("rope.g.cos_hi"));
    let p2 = TirProgramV2::from_v1_lifting_params(
        &p1,
        &[
            (tok, InputSource::External { lo: -128, hi: 127 }),
            (cos, InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 }),
        ],
        OutputDecl::Logits { node: p1.logits, scheme_id: p1.logits_scheme_id },
    )
    .unwrap();
    let p2 = TirProgramV2::decode_canonical(&p2.encode()).unwrap();
    let params2 = params_by_name(&p1, &params1, &p2);
    let mut inputs = MapInputs::default();
    inputs.constant.insert(0, params1.tensors[&(tok, None)].clone());
    inputs.constant.insert(1, params1.tensors[&(cos, None)].clone());
    let tokens = [3u32, 17, 0, 23, 9, 9, 4];
    let run1 = Interpreter::new(&p1).unwrap().run(&params1, &tokens).unwrap();
    let run2 = InterpreterV2::new(&p2).unwrap().run(&params2, &inputs, &tokens).unwrap();
    assert_eq!(run1.len(), run2.len());
    for (a, b) in run1.iter().zip(&run2) {
        assert_eq!(a.logits, b.output, "position {}", a.pos);
        assert_eq!(a.commits, b.commits, "position {}", a.pos);
    }
}

#[test]
fn the_domain_tables_agree_with_misaka_palw_gen() {
    use misaka_palw_gen::rand::{GAUSS_Q24_V1_MAX as GMAX, GAUSS_Q24_V1_MIN as GMIN, RAND_DOMAINS_V1, RandStepRuleV1};
    assert_eq!((GAUSS_Q24_V1_MIN, GAUSS_Q24_V1_MAX), (GMIN as i64, GMAX as i64));
    let as_inputs: Vec<(u16, u8, RandStepRule)> = RAND_DOMAINS_V1
        .iter()
        .filter(|d| d.id != 0)
        .map(|d| {
            let rule = match d.step {
                RandStepRuleV1::Zero => RandStepRule::Zero,
                RandStepRuleV1::PerStep => RandStepRule::PerStep,
                RandStepRuleV1::Declared => RandStepRule::Declared,
                RandStepRuleV1::None => panic!("only domain 0 has no step"),
            };
            (d.id, d.word_bits as u8, rule)
        })
        .collect();
    assert_eq!(as_inputs, RANDOM_INPUT_DOMAINS_V2.to_vec(), "normal form's table is R's, domain 0 excepted");
    assert_eq!(random_input_domain_v2(0), None, "text selection is not a program input");
}

// ---- The post write ------------------------------------------------------------------------------------

fn run_denoiser(p: &TirProgramV2, steps: u32) -> (Vec<misaka_palw_tir::interp_v2::StepOutputV2>, RunState, MapInputs) {
    let params = materialize_v2(p, 11);
    let random = GenRandom { seed: [0x2a; 32], position: 0 };
    let mut rng = Lcg(99);
    let inputs = denoiser_inputs(&random, cond_rows(&mut rng), 3, 24, 1, steps);
    let interp = InterpreterV2::new(p).unwrap();
    let mut state = RunState::default();
    let out = (0..steps).map(|_| interp.step(&params, &inputs, &mut state, 0).unwrap()).collect();
    (out, state, inputs)
}

#[test]
fn the_denoiser_writes_its_latent_in_post() {
    let p = denoiser_program();
    let (steps, state, inputs) = run_denoiser(&p, 3);
    let latent = p.states.iter().position(|s| s.name == "den.latent").unwrap() as u16;
    let (pre, x) = (p.schedule.pre, pre_carry(&p, 0));
    let x_of = |s: &misaka_palw_tir::interp_v2::StepOutputV2| {
        s.commits.iter().find(|c| c.block == pre && c.node == x).map(|c| c.value.clone()).expect("pre's x is committed")
    };
    // Position 0 starts from the noise: Q24 → Q12, flattened.
    let noise = inputs.constant[&IN_NOISE].clone();
    let q12: Vec<i128> = noise
        .data
        .iter()
        .map(|v| {
            let m = v.abs();
            let q = m / 4096 + i128::from(2 * (m % 4096) >= 4096);
            if *v < 0 { -q } else { q }
        })
        .collect();
    assert_eq!(x_of(&steps[0]).data, q12, "the initial latent is the R noise at position 0");
    // Every later position starts from the previous position's written latent.
    for pos in 1..steps.len() {
        assert_eq!(x_of(&steps[pos]), steps[pos - 1].output, "position {pos} reads what position {} wrote", pos - 1);
    }
    assert_eq!(state.fixed[&(latent, None)], steps[2].output, "the run state holds the last write");
    assert!(
        steps[2].output.data.iter().all(|v| (LATENT_LO as i128..=LATENT_HI as i128).contains(v)),
        "saturated to the state's range"
    );
    assert_eq!(state.pos, 3);
}

#[test]
fn a_post_write_is_a_commit_point_and_a_separate_output_leaves_the_write_as_is() {
    // NF-29: a post write that is not a commit point is refused, output or not.
    assert_eq!(kind(&denoiser_variant(false, false)), Some(TirErrorKind::NormalForm), "an uncommitted write beside the output");
    assert_eq!(kind(&denoiser_variant(false, true)), Some(TirErrorKind::NormalForm), "an uncommitted write as the output");
    // A committed write with a separate output node runs as the write-as-output program does: the same
    // states and outputs, and one more commit point (the separate output).
    let (a, sa, _) = run_denoiser(&denoiser_program(), 3);
    let (b, sb, _) = run_denoiser(&denoiser_variant(true, false), 3);
    assert_eq!(sa.fixed, sb.fixed);
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.output, y.output);
        assert_eq!(x.commits.len() + 1, y.commits.len());
    }
}

#[test]
fn a_failed_step_writes_nothing() {
    let p = denoiser_program();
    let params = materialize_v2(&p, 11);
    let random = GenRandom { seed: [1; 32], position: 0 };
    let mut rng = Lcg(5);
    let mut inputs = denoiser_inputs(&random, cond_rows(&mut rng), 2, 16, 0, 3);
    let interp = InterpreterV2::new(&p).unwrap();
    let mut state = RunState::default();
    interp.step(&params, &inputs, &mut state, 0).unwrap();
    let before = state.clone();
    inputs.at.insert((IN_GUIDANCE, 1), Tensor::scalar(DType::I32, 161).unwrap());
    assert_eq!(interp.step(&params, &inputs, &mut state, 0).unwrap_err().kind, TirErrorKind::Operand, "161 is outside [0, 160]");
    assert_eq!(state, before, "nothing was written, and the position did not advance");
}

// ---- Inputs are held to their declarations --------------------------------------------------------------

#[test]
fn input_values_are_held_to_their_declarations() {
    let p = denoiser_program();
    let params = materialize_v2(&p, 3);
    let random = GenRandom { seed: [9; 32], position: 0 };
    let mut rng = Lcg(1);
    let good = denoiser_inputs(&random, cond_rows(&mut rng), 4, 16, 1, 1);
    let interp = InterpreterV2::new(&p).unwrap();
    let step = |inputs: &MapInputs| interp.step(&params, inputs, &mut RunState::default(), 0).map(|_| ()).map_err(|e| e.kind);
    assert_eq!(step(&good), Ok(()));
    let with = |k: u16, t: Tensor| {
        let mut m = good.clone();
        m.constant.insert(k, t);
        m
    };
    let mut missing = good.clone();
    missing.constant.remove(&IN_COND_LEN);
    assert_eq!(step(&missing), Err(TirErrorKind::Missing));
    assert_eq!(step(&with(IN_COND_LEN, Tensor::scalar(DType::I32, 1).unwrap())), Err(TirErrorKind::Operand), "wrong dtype");
    assert_eq!(step(&with(IN_COND_LEN, Tensor::scalar(DType::Idx, 6).unwrap())), Err(TirErrorKind::Operand), "outside [0, 5]");
    assert_eq!(
        step(&with(IN_STEPS_IDX, Tensor::new(DType::Idx, vec![1], vec![0]).unwrap())),
        Err(TirErrorKind::Operand),
        "wrong shape"
    );
    let too_big = Tensor::new(DType::I32, vec![3, 2, 2], vec![GAUSS_Q24_V1_MAX as i128 + 1; 12]).unwrap();
    assert_eq!(step(&with(IN_NOISE, too_big)), Err(TirErrorKind::Operand), "a Normal input outside the table's range");
}

#[test]
fn a_cone_reads_only_the_inputs_its_closure_needs() {
    let p = denoiser_program();
    let params = materialize_v2(&p, 3);
    let interp = InterpreterV2::new(&p).unwrap();
    let pre = p.schedule.pre;
    let latent = p.states.iter().position(|s| s.name == "den.latent").unwrap() as u16;
    // `x = Select(pos == 0, noise, latent)` at position 1: its closure reads the noise input and the
    // latent, never the conditioning.
    let mut env = ConeEnv { pos: 1, ..Default::default() };
    env.fixed.insert(latent, Tensor::new(DType::I32, vec![LAT as usize], vec![77; LAT as usize]).unwrap());
    let mut only_noise = MapInputs::default();
    only_noise
        .constant
        .insert(IN_NOISE, GenRandom { seed: [3; 32], position: 0 }.random(1, RandomDist::Normal, 0, &[3, 2, 2]).unwrap());
    let x = interp.eval_cone(pre, None, pre_carry(&p, 0), &params, &only_noise, &env).unwrap();
    assert_eq!(x.data, vec![77; LAT as usize], "past position 0 the Select keeps the latent");
    // The text vector (the last node of pre) reads the conditioning: without it, Missing.
    let t = (p.blocks[pre as usize].nodes.len() - 1) as u16;
    assert_eq!(interp.eval_cone(pre, None, t, &params, &only_noise, &env).unwrap_err().kind, TirErrorKind::Missing);
}

// ---- Ranges use the declared intervals -----------------------------------------------------------------

#[test]
fn ranges_rest_on_the_declared_intervals() {
    let p = denoiser_program();
    let iv = output_interval_v2(&p).unwrap();
    assert_eq!((iv.lo, iv.hi), (LATENT_LO as i128, LATENT_HI as i128), "the output is the saturated latent");
    analyze_ranges_v2(&encoder_program()).unwrap();
    let dec = output_interval_v2(&decoder_program()).unwrap();
    assert_eq!((dec.lo, dec.hi), (0, 255), "the decoder's pixels are proved in [0, 255]");
    // The same program with every input taking its dtype's full range — the view as version 1 sees
    // it — cannot be proved: guidance × velocity would overflow i64. The declared intervals are
    // what makes the program admissible.
    let view = validate_v2(&p).unwrap().view;
    let refused = analyze_ranges(&view).unwrap_err().kind;
    assert!(
        matches!(refused, TirErrorKind::Index | TirErrorKind::Overflow),
        "only the declared intervals discharge the step-set gather's index obligation and the guidance product: {refused:?}"
    );
}

// ---- Normal form ---------------------------------------------------------------------------------------

#[test]
fn every_version_2_rule_refuses_by_name() {
    let base = denoiser_program();
    assert_eq!(kind(&base), None);
    let mut cases: Vec<(&str, TirProgramV2, TirErrorKind)> = Vec::new();
    let mut push = |what: &'static str, f: &dyn Fn(&mut TirProgramV2), k: TirErrorKind| {
        let mut p = base.clone();
        f(&mut p);
        cases.push((what, p, k));
    };
    use TirErrorKind::{NormalForm, Shape};
    push("version 1", &|p| p.version = 1, NormalForm);
    push("prim set", &|p| p.prim_set_id[0] ^= 1, NormalForm);
    push(
        "17 inputs",
        &|p| {
            for i in 0..11 {
                p.inputs.push(InputDecl {
                    name: format!("extra{i}"),
                    dtype: DType::I32,
                    shape: vec![1],
                    source: InputSource::External { lo: 0, hi: 1 },
                });
            }
        },
        NormalForm,
    );
    push("an input named like a param", &|p| p.inputs[1].name = p.params[0].name.clone(), NormalForm);
    push("two inputs with one name", &|p| p.inputs[1].name = p.inputs[2].name.clone(), NormalForm);
    push("an empty input name", &|p| p.inputs[1].name = String::new(), NormalForm);
    push("rank 5", &|p| p.inputs[1].shape = vec![1, 1, 1, 5, 4], NormalForm);
    push("an i64 external input", &|p| p.inputs[3].dtype = DType::I64, NormalForm);
    push("lo > hi", &|p| p.inputs[3].source = InputSource::External { lo: 5, hi: 4 }, NormalForm);
    push("an interval outside its dtype", &|p| p.inputs[2].source = InputSource::External { lo: -1, hi: 5 }, NormalForm);
    push(
        "domain 0 is text selection",
        &|p| p.inputs[0].source = InputSource::Random { domain: 0, dist: RandomDist::Normal, per_step: false },
        NormalForm,
    );
    push(
        "an unregistered domain",
        &|p| p.inputs[0].source = InputSource::Random { domain: 8, dist: RandomDist::Normal, per_step: false },
        NormalForm,
    );
    push(
        "Normal over 32-bit words",
        &|p| p.inputs[0].source = InputSource::Random { domain: 7, dist: RandomDist::Normal, per_step: false },
        NormalForm,
    );
    push("a Normal input that is not i32", &|p| p.inputs[0].dtype = DType::I16, NormalForm);
    push(
        "Uniform bits that are not the domain's",
        &|p| {
            p.inputs[0].dtype = DType::Idx;
            p.inputs[0].source = InputSource::Random { domain: 1, dist: RandomDist::Uniform { bits: 8 }, per_step: false };
        },
        NormalForm,
    );
    push(
        "per_step on a one-shot domain",
        &|p| p.inputs[0].source = InputSource::Random { domain: 1, dist: RandomDist::Normal, per_step: true },
        NormalForm,
    );
    push(
        "no per_step on a per-step domain",
        &|p| p.inputs[5].source = InputSource::Random { domain: 2, dist: RandomDist::Normal, per_step: false },
        NormalForm,
    );
    push(
        "one domain drawn twice",
        &|p| p.inputs[5].source = InputSource::Random { domain: 1, dist: RandomDist::Normal, per_step: false },
        NormalForm,
    );
    push(
        "a reference to no input",
        &|p| {
            let pre = p.schedule.pre as usize;
            for n in &mut p.blocks[pre].nodes {
                for r in &mut n.inputs {
                    if *r == TirProgramV2::input_ref(IN_NOISE as usize) {
                        *r = Ref::Input(2 + 6);
                    }
                }
            }
        },
        NormalForm,
    );
    push(
        "an unused input",
        &|p| {
            p.inputs.push(InputDecl {
                name: "unused".into(),
                dtype: DType::I32,
                shape: vec![1],
                source: InputSource::External { lo: 0, hi: 1 },
            })
        },
        NormalForm,
    );
    push(
        "a Logits program writes nothing in post",
        &|p| p.output = OutputDecl::Logits { node: p.output.node(), scheme_id: [0; 64] },
        NormalForm,
    );
    push(
        "post writes a per-layer state",
        &|p| p.states.iter_mut().find(|s| s.name == "den.latent").unwrap().per_layer = true,
        NormalForm,
    );
    push(
        "pre and post both write the latent",
        &|p| {
            let latent = p.states.iter().position(|s| s.name == "den.latent").unwrap() as u16;
            let pre = p.schedule.pre as usize;
            let x = p.blocks[pre].carry_out[0];
            let out = p.blocks[pre].nodes[x as usize].out.clone();
            p.blocks[pre].nodes.push(misaka_palw_tir::program::Node {
                prim: Prim::StateWrite { state: latent },
                inputs: vec![Ref::Node(x)],
                out,
                commit: false,
            });
        },
        NormalForm,
    );
    push(
        "a post-written state nobody reads",
        &|p| {
            let pre = p.schedule.pre as usize;
            let latent = p.states.iter().position(|s| s.name == "den.latent").unwrap() as u16;
            for n in &mut p.blocks[pre].nodes {
                for r in &mut n.inputs {
                    if *r == Ref::State(latent) {
                        *r = Ref::Node(1); // the flattened noise stands in
                    }
                }
            }
        },
        NormalForm,
    );
    push("the output is not committed", &|p| *p = denoiser_variant(false, true), NormalForm);
    push(
        "a post write of the wrong dtype",
        &|p| {
            let latent = p.states.iter().position(|s| s.name == "den.latent").unwrap();
            p.states[latent].dtype = DType::I16;
            p.states[latent].kind = StateKind::Fixed { lo: -100, hi: 100 };
        },
        Shape,
    );
    push("the output node does not exist", &|p| p.output = OutputDecl::Final { node: 999 }, NormalForm);
    for (what, p, want) in &cases {
        assert_eq!(kind(p), Some(*want), "{what}");
    }
    // The encoder is a Rows program: a history append in post is refused (NF-19 holds for post).
    let mut enc = encoder_program();
    let post = enc.schedule.post as usize;
    let hist = enc.states.iter().position(|s| matches!(s.kind, StateKind::Hist { .. })).unwrap() as u16;
    let carry_ty = enc.blocks[post].carry_in[0].clone();
    let mut h_out = carry_ty.clone();
    h_out.shape.insert(0, misaka_palw_tir::Dim::H);
    enc.blocks[post].nodes.push(misaka_palw_tir::program::Node {
        prim: Prim::HistAppend { state: hist },
        inputs: vec![Ref::CarryIn(0)],
        out: h_out,
        commit: false,
    });
    assert!(kind(&enc).is_some(), "post appends to no history");
}

/// **G9's audit, on the IR side**: every declaration whose dimensions are each legal (`≤ 2^24`) and
/// whose element count is past `u64` is refused by name (`NormalForm`) by the validator — through
/// the bytes too — and never a multiply-overflow panic. The count is a saturating product checked
/// against its cap before any other use (`check_static_shape`, `check_param_shape`,
/// `check_tensor_type`).
#[test]
fn an_element_count_past_u64_is_refused_by_name_and_never_panics() {
    let base = denoiser_program();
    assert_eq!(kind(&base), None);
    let big = 1u32 << 24;
    let huge = vec![big; 4];
    type Edit = Box<dyn Fn(&mut TirProgramV2)>;
    let cases: Vec<(&str, Edit)> = vec![
        ("an input", Box::new({
            let huge = huge.clone();
            move |p| p.inputs[1].shape = huge.clone()
        })),
        ("a param", Box::new({
            let huge = huge.clone();
            move |p| p.params[0].shape = huge.clone()
        })),
        ("a state", Box::new({
            let huge = huge.clone();
            move |p| p.states[0].shape = huge.clone()
        })),
        ("a node's type", Box::new({
            let huge = huge.clone();
            move |p| {
                use misaka_palw_tir::types::Dim;
                p.blocks[0].nodes[0].out.shape = huge.iter().map(|d| Dim::Fixed(*d)).collect();
            }
        })),
    ];
    for (what, edit) in cases {
        let mut p = base.clone();
        edit(&mut p);
        // A declaration is a normal-form refusal, a node's type a shape one: both by name.
        assert!(matches!(kind(&p), Some(TirErrorKind::NormalForm | TirErrorKind::Shape)), "{what}: {:?}", kind(&p));
        // Through the bytes: the decoder refuses it the same way.
        let err = TirProgramV2::decode_canonical(&p.encode()).expect_err(what);
        assert!(matches!(err.kind, TirErrorKind::NormalForm | TirErrorKind::Shape), "{what}: {err:?}");
    }
}

#[test]
fn a_parameterless_decode_is_a_valid_empty_params_program() {
    // The decoder declares no param at all: inputs alone are enough.
    let d = decoder_program();
    assert!(d.params.is_empty() && d.inputs.len() == 1);
    let interp = InterpreterV2::new(&d).unwrap();
    let mut inputs = MapInputs::default();
    inputs
        .constant
        .insert(0, Tensor::new(DType::I32, vec![LAT as usize], (0..LAT as i128).map(|i| i * 700 - 4096).collect()).unwrap());
    let out = interp.run_positions(&MapParams::default(), &inputs, 1).unwrap();
    assert_eq!(out[0].output.shape, vec![2, 2, 3]);
    assert!(out[0].output.data.iter().all(|v| (0..=255).contains(v)));
}
