//! **`WEIGHT_ROTATION_HADAMARD_V1`: a checkpoint stored in a block-Hadamard-rotated basis computes the model** (COV-P1P2,
//! 2026-10-08; PrismML's `prism.hadamard` v1, PrismML-Eng/llama.cpp @`7dffb158`, read with the user's approval).
//!
//! The fixtures (`tests/fixtures/gguf/prism_rotated/{plain,rotated}`) are one tiny Qwen3.5 written twice by
//! `tools/gen_prism_rotated_fixture.py` — a pure-Python fold that shares no code with this crate: `plain` with float weights,
//! `rotated` with 27 weights stored as `W' = W·diag(s)·H` on their input axis, the token table as `H·(s ⊙ z)`, the GDN output
//! projection folded in grouped value-head order, `ssm_alpha`/`ssm_beta` unrotated (as in the real file), and the declaration
//! beside them. The reader applies the declared transform as an activation-side op (`Op::BlockLinear`), never folding it into the
//! weights, so a stored weight stays the class's integers; the two files must compute the same function.
use misaka_palw_tir_lower::LowerError;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::gguf::GgufModel;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::{fidelity, hl};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture(which: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gguf/prism_rotated").join(which).join("model.gguf")
}

/// The float logits of `model` on `tokens`.
fn float_logits(model: &GgufModel, tokens: &[usize]) -> (fidelity::Prepared, Vec<Vec<f32>>) {
    let prep = model.prepare(&LowerOpts::default()).expect("prepared");
    let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, model).expect("params");
    assert!(unused.is_empty() || unused.iter().all(|u| u.starts_with("rotation.")), "unread: {unused:?}");
    let got = Session::new(&prep.hl, &params).run(tokens).expect("run");
    (prep, got)
}

#[test]
fn the_rotated_file_computes_the_plain_model() {
    let plain = GgufModel::open(&fixture("plain")).expect("plain");
    let rotated = GgufModel::open(&fixture("rotated")).expect("the rotation is modelled");
    assert!(plain.rotation().is_none());
    let r = rotated.rotation().expect("declared");
    assert_eq!((r.block, r.weights.len(), r.gdn_v_grouped), (32, 27, true));
    let tokens: Vec<usize> = vec![3, 17, 200, 5, 64, 9, 128, 31, 250, 2, 77, 140];
    let (pp, a) = float_logits(&plain, &tokens);
    let (rp, b) = float_logits(&rotated, &tokens);
    // The spec says which projections read a rotated activation — found from the binding, not a role table.
    let roles: Vec<&str> = rp.spec.hf.input_rotations.keys().map(String::as_str).collect();
    for want in ["gdn.q", "gdn.k", "gdn.v", "gdn.z", "gdn.out", "mlp.gate", "mlp.up", "mlp.down", "head"] {
        assert!(roles.contains(&want), "{want} not among {roles:?}");
    }
    assert!(!roles.contains(&"gdn.a") && !roles.contains(&"gdn.b"), "ssm_alpha/beta are stored unrotated: {roles:?}");
    assert!(rp.spec.hf.embed_rotation.is_some());
    assert!(pp.spec.hf.input_rotations.is_empty());
    // The rotation is an op of the program, one per rotated activation of a block (q/k/v share theirs).
    let blocks = |p: &hl::HlProgram| {
        p.blocks.iter().flat_map(|b| b.nodes.iter()).filter(|n| matches!(n.op, hl::Op::BlockLinear { .. })).count()
    };
    assert_eq!(blocks(&pp.hl), 0);
    assert!(blocks(&rp.hl) > 0);
    // The same function, to f32 rounding.
    let scale = a.iter().flatten().fold(1.0f32, |m, v| m.max(v.abs()));
    let worst = a.iter().zip(&b).flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| (p - q).abs())).fold(0.0f32, f32::max);
    eprintln!("rotated vs plain, float: max |Δ| {worst:.3e} (scale {scale:.2})");
    assert!(worst <= 1e-4 * scale, "the rotated file is another model: max |Δ| {worst}");
}

#[test]
fn the_integer_program_of_the_rotated_file_follows_its_float_reference() {
    let rotated = GgufModel::open(&fixture("rotated")).expect("rotated");
    let plain = GgufModel::open(&fixture("plain")).expect("plain");
    let eval = fidelity::random_sequences(256, 3, 24, 4242);
    let run = |m: &GgufModel| {
        let prep = m.prepare(&LowerOpts::default()).expect("prepared");
        misaka_palw_tir::interval::analyze_ranges(&prep.lowered.program).expect("the range analysis proves the program");
        let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, m).expect("params");
        let loader = Resident(Arc::new(params));
        let calib = fidelity::random_sequences(256, 6, 32, 7);
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrated");
        let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialised");
        let float = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).expect("float");
        let int: Vec<Vec<Vec<f64>>> = eval
            .iter()
            .map(|s| fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}))
            .collect::<Result<_, _>>()
            .expect("int");
        (fidelity::compare(&float, &int, &eval), float)
    };
    let (mr, fr) = run(&rotated);
    let (mp, fp) = run(&plain);
    eprintln!(
        "rotated: top-1 {:.3} KL {:.5}; plain: top-1 {:.3} KL {:.5}",
        mr.top1_agreement, mr.kl_mean, mp.top1_agreement, mp.kl_mean
    );
    assert!(mr.top1_agreement >= 0.9 && mr.kl_mean <= 0.02, "rotated integer program: top-1 {} KL {}", mr.top1_agreement, mr.kl_mean);
    // The two float references are one model.
    let worst = fr
        .iter()
        .flatten()
        .zip(fp.iter().flatten())
        .flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| (p - q).abs()))
        .fold(0.0f32, f32::max);
    assert!(worst <= 1e-3, "float references differ by {worst}");
}

/// Every way a declaration can be one the producer would read differently is refused by name.
#[test]
fn a_rotation_the_reader_cannot_apply_exactly_is_refused_by_name() {
    let bytes = std::fs::read(fixture("rotated")).unwrap();
    let open = |b: &[u8]| -> Result<GgufModel, LowerError> {
        let f = misaka_palw_tir_lower::gguf::GgufFile::parse(
            b,
            Some(b.len() as u64),
            Path::new("x.gguf"),
            misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin(),
        )?;
        GgufModel::from_file(f).and_then(|m| m.spec().map(|_| m))
    };
    open(&bytes).expect("the fixture reads");
    // A string-level edit of one metadata value of the same length keeps every offset.
    let edit = |from: &str, to: &str| -> Vec<u8> {
        assert_eq!(from.len(), to.len());
        let at = bytes.windows(from.len()).position(|w| w == from.as_bytes()).unwrap_or_else(|| panic!("`{from}` not in the fixture"));
        let mut b = bytes.clone();
        b[at..at + to.len()].copy_from_slice(to.as_bytes());
        b
    };
    for (from, to, needle) in [
        ("normalized-sylvester-walsh-hadamard", "normalized-sylvester-walsh-hadamarX", "transform"),
        ("input-last-dimension", "output-lastdimension", "axis"),
        ("explicit", "implicit", "sign mode"),
        // the rotated set loses one layer's down projection (its name no longer matches a tensor)
        ("blk.3.ffn_down.weight", "blk.3.ffn_dowX.weight", "not found"),
    ] {
        match open(&edit(from, to)) {
            Err(LowerError::NotLowerable(m)) => assert!(m.contains(needle) && m.contains("prism.hadamard"), "{from}: {m}"),
            Err(other) => panic!("{from}: {other:?}"),
            Ok(_) => panic!("{from}: read"),
        }
    }
}
