//! Real first-layer input RMS, Q/K/V projections and per-head Q/K RMS on the
//! native typed TIR executor, for the isolated source-row fixture.
use super::SourceRows;
use kaspa_consensus_core::{
    Hash64,
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1},
};
use misaka_palw_tir::{
    DType, Ref, TensorType,
    builder::ProgramBuilder,
    library::Narrowing,
    program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN},
};
use misaka_palw_tir_exec::{Buf, NodeValue, Slice, StepSink, TirExecutor, node::TirArtifactV1};
use misaka_palw_tir_lower::qf1::{AttentionInputParams, attention_input};
use misaka_palw_tir_lower::{
    quant::{mul_shift, quantize_rows},
    weights::widen_floats,
};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

const WIDTH: usize = 4096;
const GAINS: [(&str, usize, &str); 3] = [
    ("input_layernorm", WIDTH, "00695bad97c2abc77a9d1ce57e4d4a5e5c2b574387fcb4029ef5ce12239b2530"),
    ("self_attn.q_norm", 128, "ad88a3013b2d8ecd138c36460751296c56bed2eff2d5d3a66377db04fd9e0799"),
    ("self_attn.k_norm", 128, "aaf5042c20082b5c13daed62ad9627f426cda5edc38eca48bd3f956da4eb05cd"),
];
const PROJECTIONS: [(&str, usize, &str); 3] = [
    ("self_attn.q_proj", 4096, "fd56b85bf301661c8655ed517928304d25df7d6b3ea3d9be85c021159d61bad8"),
    ("self_attn.k_proj", 1024, "3ce9fdf6ee30ef2f24ad3629b506415ca91fe40f790e568a9ea3fea2978feee0"),
    ("self_attn.v_proj", 1024, "767a6c48457974bb02dd90aeec270f4d68289527a353c3c28964ab2069a966ee"),
];

fn source(path: &Path, name: &str, elements: usize, pin: &str) -> Vec<f32> {
    let bytes = fs::read(path.join(format!("model.layers.0.{name}.weight.bf16"))).unwrap();
    assert_eq!(bytes.len(), 2 * elements);
    assert_eq!(faster_hex::hex_string(&Sha256::digest(&bytes)), pin);
    widen_floats("BF16", &bytes).unwrap()
}

fn parameter(p: &mut ProgramBuilder, inputs: &mut Vec<Buf>, name: &str, shape: &[u32], data: Buf) -> Ref {
    let r = p.param(name, data.dtype(), shape, false);
    assert_eq!(r, Ref::Param(inputs.len() as u16));
    inputs.push(data);
    r
}

#[derive(Default)]
struct Inspection {
    position: usize,
    digests: [Sha256; 6],
}

impl StepSink for Inspection {
    fn node(&mut self, value: &NodeValue<'_>) {
        if value.layer != Some(0) {
            return;
        }
        let (name, width) =
            [("input_norm", 4096), ("q", 4096), ("k", 1024), ("v", 1024), ("q_norm", 4096), ("k_norm", 1024)][self.position];
        let Slice::I16(lanes) = value.data else { panic!("QF1 committed value must contain i16 codes") };
        assert_eq!(lanes.len(), width, "{name} lane count");
        self.digests[self.position].update(lanes.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>());
        self.position += 1;
    }
}

pub(super) fn check(path: &Path, rows: &SourceRows<'_>, tokens: &[u32]) {
    let ids = rows.keys().copied().collect::<Vec<_>>();
    let mut inputs = Vec::new();
    // The enclosing test binds the full token IDs to verified source rows.
    // A compact local gather keeps this diagnostic program small.
    let mut p = ProgramBuilder::new(ids.len() as u32, HISTORY_BOUND_V1_SMALL);
    let table = parameter(
        &mut p,
        &mut inputs,
        "input_rows",
        &[ids.len() as u32, WIDTH as u32],
        Buf::I16(rows.values().flat_map(|(_, row)| row.iter().copied()).collect()),
    );
    let gains = GAINS.map(|(name, n, sha)| {
        let values = source(path, name, n, sha);
        let codes = values
            .iter()
            .map(|v| {
                let g = (f64::from(*v) * (1 << 20) as f64).round();
                assert!(g.is_finite() && g >= i32::MIN as f64 && g <= i32::MAX as f64);
                g as i32
            })
            .collect();
        parameter(&mut p, &mut inputs, name, &[n as u32], Buf::I32(codes))
    });
    let projections = PROJECTIONS.map(|(name, n, sha)| {
        let w = quantize_rows(&source(path, name, n * WIDTH, sha), n, WIDTH, None);
        // Normalized inputs and Q/K/V outputs both use 2^-10.
        let (m, s): (Vec<i64>, Vec<i8>) = w.scales.into_iter().map(mul_shift).unzip();
        let shape = &[n as u32];
        let weight = parameter(&mut p, &mut inputs, name, &[n as u32, WIDTH as u32], Buf::I8(w.codes));
        let mult = parameter(&mut p, &mut inputs, &format!("{name}.multiplier"), shape, Buf::I64(m));
        let shift = parameter(&mut p, &mut inputs, &format!("{name}.shift"), shape, Buf::I8(s));
        (weight, Narrowing::new(mult, shift, None))
    });
    // Q24 * Q20 gain -> Q10 output. Epsilon is 10^-6 in physical
    // units, expressed in Q24 squared input codes with fixed rounding.
    let gain_narrow = Narrowing::new(p.scalar(DType::I64, 1), p.scalar(DType::I32, 34), None);
    let eps = |fraction_bits| ((1_i128 << (24u32 + 2 * fraction_bits)) + 500_000) / 1_000_000;
    let input_eps = p.scalar(DType::I64, eps(15u32));
    let head_eps = p.scalar(DType::I64, eps(10u32));
    let carry = vec![TensorType::fixed(DType::I16, &[WIDTH as u32])];
    let mut pre = p.block("input", vec![]);
    let x = pre.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
    let pre_id = pre.finish(&[x]);
    let mut body = p.block("attention_input", carry.clone());
    let mut operands =
        AttentionInputParams { gains, eps: [input_eps, head_eps, head_eps], gain_narrow: [gain_narrow; 3], projections };
    operands.gains.swap(0, 1);
    assert!(attention_input(&mut body, Ref::CarryIn(0), &operands).is_err());
    operands.gains.swap(0, 1);
    operands.projections.swap(0, 2);
    assert!(attention_input(&mut body, Ref::CarryIn(0), &operands).is_err());
    operands.projections.swap(0, 2);
    let a = attention_input(&mut body, Ref::CarryIn(0), &operands).unwrap();
    for producer in [a.input_norm, a.q, a.k, a.v, a.q_norm, a.k_norm] {
        body.commit(producer);
    }
    let layer = body.finish(&[a.q]);
    let mut post = p.block("observation", carry);
    let lanes = post.slice(Ref::CarryIn(0), 0, 0, ids.len() as u32);
    let tail = post.cast(lanes, DType::I32);
    let Ref::Node(last) = post.commit(tail) else { unreachable!() };
    let post_id = post.finish(&[]);
    let mut program = p.finish(pre_id, vec![layer], post_id, last);
    program.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 8192,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 64).collect(),
        state_tiles: Vec::new(),
    };
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let file = dir.path().join("attention-input.palwtir");
    let tokenizer: Hash64 = super::super::QF1_TEST_TOKENIZER.parse().unwrap();
    misaka_palw_tir_artifact::write_container_v1(
        &file,
        &program,
        borsh::to_vec(&layout).unwrap(),
        tokenizer.as_bytes(),
        format!("isolated QF1 pre-attention measurement; source rows in order: {ids:?}"),
        &mut |j, _| Ok(inputs[j as usize].to_le_bytes()),
    )
    .unwrap();
    drop(inputs);
    let artifact_hash = faster_hex::hex_string(&Sha256::digest(fs::read(&file).unwrap()));
    let artifact = TirArtifactV1::open(&file).unwrap();
    let mut exec = TirExecutor::new(artifact.plan(), artifact.params()).unwrap();
    let mut observer = Inspection::default();
    for id in tokens {
        observer.position = 0;
        exec.step(ids.binary_search(id).unwrap() as u32, &mut observer).unwrap();
        assert_eq!(observer.position, 6);
    }
    let hashes = observer.digests.map(|d| faster_hex::hex_string(&d.finalize()));
    let line = format!(
        "QF1 attention pid={} positions={} layer=0 artifact={artifact_hash} i16_sha256={}\n",
        std::process::id(),
        tokens.len(),
        hashes.join(",")
    );
    std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
}
