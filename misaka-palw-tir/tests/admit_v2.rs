//! **Admission of version-2 programs and pipelines** (spec 04b §15.9): the view's analyses with the
//! inputs' intervals and openings, the post-written states, the per-stage and per-job ceilings.

mod v2common;

use misaka_palw_tir::TirErrorKind;
use misaka_palw_tir::admit::{LeafV1, ParamLeafV1, TirAdmitError, TirAdmitInputsV1, TirCeilingsV1};
use misaka_palw_tir::admit_v2::*;
use misaka_palw_tir::program_v2::*;
use v2common::*;

fn inputs() -> TirAdmitInputsV1 {
    TirAdmitInputsV1 { tile_len: 64, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() }
}

fn toy_bytes() -> (Vec<u8>, Vec<Vec<u8>>) {
    let (p, progs) = toy_pipeline();
    (p.encode(), progs.iter().map(|x| x.encode()).collect())
}

#[test]
fn every_toy_program_is_admitted_with_its_inputs_read_as_declared() {
    for p in [encoder_program(), denoiser_program(), decoder_program()] {
        let a = tir_admit_program_v2(&p, &inputs()).unwrap();
        assert_eq!(a.inputs.len(), p.inputs.len());
        for (d, i) in p.inputs.iter().zip(&a.inputs) {
            let (lo, hi) = d.interval();
            assert_eq!((i.interval.lo, i.interval.hi), (lo, hi));
            let standalone = if d.is_external() { ParamLeafV1::Committed } else { ParamLeafV1::Derived };
            assert_eq!(i.leaf, standalone, "a program admitted on its own reads its inputs conservatively");
        }
        // PALW-TIR-33's domains are the program's own node indices.
        let post = p.schedule.post as usize;
        assert_eq!(a.view.intervals[post].len(), p.blocks[post].nodes.len());
    }
}

#[test]
fn a_post_written_state_is_a_leaf_with_no_replay() {
    let p = denoiser_program();
    let a = tir_admit_program_v2(&p, &inputs()).unwrap();
    let latent = p.states.iter().position(|s| s.name == "den.latent").unwrap() as u16;
    assert_eq!(a.post_written, vec![latent]);
    assert!(a.view.states.iter().all(|s| s.state != latent), "the committed write is the next position's value: no C_j");
    // The latent's output interval is its saturation range.
    let iv = a.view.intervals[p.schedule.post as usize][p.output.node() as usize];
    assert_eq!((iv.lo, iv.hi), (LATENT_LO as i128, LATENT_HI as i128));
}

#[test]
fn inputs_are_opened_as_their_bindings_say() {
    let (pb, progs) = toy_bytes();
    let a = tir_admit_pipeline_v1(&pb, &progs, &inputs(), &TirJobCeilingsV1::open_v1()).unwrap();
    let den = &a.stages[1].admission;
    let leaves: Vec<ParamLeafV1> = den.inputs.iter().map(|i| i.leaf).collect();
    assert_eq!(
        leaves,
        vec![
            ParamLeafV1::Derived,   // the R noise
            ParamLeafV1::Committed, // the encoder's rows (StageRows)
            ParamLeafV1::Derived,   // their count (StageRowCount)
            ParamLeafV1::JobData,   // the guidance (JobScalar)
            ParamLeafV1::JobData,   // the step-set index (JobScalar)
            ParamLeafV1::Derived,   // the per-step R jitter
        ]
    );
    // pre's text vector reads the encoder's rows: a committed operand, 4 bytes a lane.
    let pre = den.program.schedule.pre;
    let t_node = den.program.blocks[pre as usize].carry_out[2];
    let cone = den.view.cones.iter().find(|c| c.block == pre && c.node == t_node).unwrap();
    let rows = LeafV1::Param(den.first_input_param + IN_COND);
    assert!(cone.leaves.contains(&rows));
    let standalone = tir_admit_program_v2(&den.program, &inputs()).unwrap();
    let alone = standalone.view.cones.iter().find(|c| c.block == pre && c.node == t_node).unwrap();
    assert_eq!(cone.operands, 1, "under the pipeline only the rows are a committed operand");
    assert_eq!(alone.operands, 2, "on its own the row count is read conservatively, as committed too");
    // pre's x reads the noise, derived: it opens no noise bytes, where a standalone admission would not differ
    // (a random input is derived either way) — but the row count is derived only under the pipeline.
    let x_node = den.program.blocks[pre as usize].carry_out[0];
    let x_cone = den.view.cones.iter().find(|c| c.block == pre && c.node == x_node).unwrap();
    assert!(x_cone.leaves.contains(&LeafV1::Param(den.first_input_param + IN_NOISE)));
    assert_eq!(cone.tile_opened_bytes + 4, alone.tile_opened_bytes, "the derived row count opens nothing under the pipeline");
}

#[test]
fn the_job_is_every_stage_times_its_max_trip() {
    let (pb, progs) = toy_bytes();
    let a = tir_admit_pipeline_v1(&pb, &progs, &inputs(), &TirJobCeilingsV1::open_v1()).unwrap();
    let (p, _) = toy_pipeline();
    let mut macs = 0u64;
    let mut leaves = 0u64;
    for (s, st) in a.stages.iter().zip(&p.stages) {
        assert_eq!(s.max_trip, st.max_trip);
        assert_eq!(s.job_cost.macs, s.admission.view.position.cost.macs * st.max_trip as u64);
        macs += s.job_cost.macs;
        leaves += s.job_step_leaves;
    }
    assert_eq!((a.job_cost.macs, a.job_step_leaves), (macs, leaves));
    assert_eq!((a.output_interval.lo, a.output_interval.hi), (0, 255), "the image's pixels");
}

#[test]
fn refusals_name_their_rule_and_number() {
    let (pb, progs) = toy_bytes();
    let open = TirJobCeilingsV1::open_v1();
    // Per job: MACs through the matmul stage (512 MACs a position, three positions).
    let (mp, mprogs_d) = matmul_pipeline();
    let (mb, mprogs): (Vec<u8>, Vec<Vec<u8>>) = (mp.encode(), mprogs_d.iter().map(|x| x.encode()).collect());
    let m = tir_admit_pipeline_v1(&mb, &mprogs, &inputs(), &open).unwrap();
    assert_eq!(m.job_cost.macs, 3 * 512);
    assert_eq!(m.stages[0].admission.inputs[0].leaf, ParamLeafV1::JobData, "the job's token ids");
    match tir_admit_pipeline_v1(&mb, &mprogs, &inputs(), &TirJobCeilingsV1 { max_job_macs: 3 * 512 - 1, ..open }) {
        Err(TirAdmitError::Exceeds { limit, value, cap, .. }) => assert_eq!((limit, value, cap), ("max_job_macs", 1536, 1535)),
        other => panic!("{other:?}"),
    }
    let a = tir_admit_pipeline_v1(&pb, &progs, &inputs(), &open).unwrap();
    for (limit, job) in [
        ("max_job_step_leaves", TirJobCeilingsV1 { max_job_step_leaves: a.job_step_leaves - 1, ..open }),
        ("max_job_cone_work", TirJobCeilingsV1 { max_job_cone_work: a.cone_work - 1, ..open }),
    ] {
        match tir_admit_pipeline_v1(&pb, &progs, &inputs(), &job) {
            Err(TirAdmitError::Exceeds { limit: l, at, .. }) => assert_eq!((l, at.as_str()), (limit, "the job")),
            other => panic!("{limit}: {other:?}"),
        }
    }
    // Per stage, named by the stage.
    let tight = TirAdmitInputsV1 { ceilings: TirCeilingsV1 { max_step_leaves: 1, ..TirCeilingsV1::legacy_court_v1() }, ..inputs() };
    match tir_admit_pipeline_v1(&pb, &progs, &tight, &open) {
        Err(TirAdmitError::Exceeds { limit: "max_step_leaves", at, .. }) => assert!(at.starts_with("stage 0 (encode)"), "{at}"),
        other => panic!("{other:?}"),
    }
    // The ranges rest on the declared intervals: guidance up to i32::MAX overflows the update.
    let mut wide = denoiser_program();
    wide.inputs[IN_GUIDANCE as usize].source = InputSource::External { lo: 0, hi: i32::MAX as i64 };
    match tir_admit_program_v2(&wide, &inputs()) {
        Err(TirAdmitError::Program(e)) => assert_eq!(e.kind, TirErrorKind::Overflow),
        other => panic!("{other:?}"),
    }
    // Normal form and the edges, through the pipeline.
    let mut bad = progs.clone();
    bad[1] = {
        let mut d = denoiser_program();
        d.inputs[IN_COND as usize].source = InputSource::External { lo: -100, hi: 100 };
        d.encode()
    };
    assert!(
        matches!(tir_admit_pipeline_v1(&pb, &bad, &inputs(), &open), Err(TirAdmitError::Program(e)) if e.kind == TirErrorKind::NormalForm)
    );
    // The network inputs.
    assert!(matches!(tir_admit_v2(&progs[0], &TirAdmitInputsV1 { tile_len: 0, ..inputs() }), Err(TirAdmitError::Inputs(_))));
    // A version-1 program is not a version-2 program.
    let (v1, _) = {
        let mut pb = misaka_palw_tir::builder::ProgramBuilder::new(2, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let t = b.cast(misaka_palw_tir::Ref::Input(0), misaka_palw_tir::DType::I32);
            b.finish(&[t])
        };
        let post = {
            let mut b = pb.block("post", vec![misaka_palw_tir::TensorType::scalar(misaka_palw_tir::DType::I32)]);
            let l = b.reshape_fixed(misaka_palw_tir::Ref::CarryIn(0), &[1]);
            b.commit(l);
            b.finish(&[])
        };
        (pb.finish(pre, vec![], post, 0), ())
    };
    assert!(matches!(tir_admit_v2(&v1.encode(), &inputs()), Err(TirAdmitError::Program(e)) if e.kind == TirErrorKind::NormalForm));
}

#[test]
fn each_stage_may_be_admitted_under_its_own_network_inputs() {
    let (pb, progs) = toy_bytes();
    let open = TirJobCeilingsV1::open_v1();
    let uniform = tir_admit_pipeline_v1(&pb, &progs, &inputs(), &open).unwrap();
    let n = uniform.stages.len();
    let same = tir_admit_pipeline_staged_v1(&pb, &progs, &vec![inputs(); n], &open).unwrap();
    assert_eq!(same, uniform, "the same inputs for every stage are the uniform admission");
    // One stage at a wider tile: its leaves (and only its) change; costs do not.
    let mut staged = vec![inputs(); n];
    staged[1].tile_len = 256;
    let wide = tir_admit_pipeline_staged_v1(&pb, &progs, &staged, &open).unwrap();
    assert!(wide.stages[1].job_step_leaves <= uniform.stages[1].job_step_leaves);
    for s in [0, 2] {
        assert_eq!(wide.stages[s], uniform.stages[s], "stage {s} is admitted as before");
    }
    assert_eq!(wide.job_cost, uniform.job_cost, "a tile length prices nothing (PALW-TIR-16)");
    // One set of inputs per stage, each legal.
    for bad in [vec![inputs(); n - 1], vec![inputs(); n + 1]] {
        assert!(matches!(tir_admit_pipeline_staged_v1(&pb, &progs, &bad, &open), Err(TirAdmitError::Inputs(_))));
    }
    staged[2].h_chunk = 3;
    assert!(matches!(tir_admit_pipeline_staged_v1(&pb, &progs, &staged, &open), Err(TirAdmitError::Inputs(_))));
}
