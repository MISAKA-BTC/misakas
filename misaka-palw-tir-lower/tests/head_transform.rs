//! **`HEAD_TRANSFORM_V1`** (FR-29): a prediction head before the vocabulary projection — `h → norm(act(dense(h)))`
//! (BERT's `cls.predictions.transform`, ModernBERT-decoder's and RoBERTa's `lm_head`) — as a spec field, three existing
//! HL ops and two weight roles. No transformers here: the tiny `llama` config with seeded synthetic weights; the equality
//! with the transformers classes is the corpus lane's fixtures (`modernbert_decoder` and the BERT lineage in decoder mode).

use misaka_palw_tir_lower::fidelity::prepare_spec;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::spec::{Act, Gain, HeadTransformSpec, ModelSpec, NormKind, NormSpec, OutputSpec};
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::path::Path;

const TOKENS: [usize; 5] = [3, 17, 42, 5, 9];

fn spec(with: bool) -> ModelSpec {
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/tiny/llama.json")).unwrap();
    let mut s = parse_config_str(&cfg).unwrap();
    if with {
        s.head.transform =
            Some(HeadTransformSpec { bias: true, act: Act::Gelu, norm: NormSpec { kind: NormKind::Layer, eps: 1e-5, gain: Gain::W, bias: true } });
        s.hf.names.insert("head.transform.dense".into(), "head.transform.dense".into());
        s.hf.names.insert("head.transform.norm".into(), "head.transform.norm".into());
    }
    s
}

#[test]
fn a_head_transform_is_dense_act_norm_before_the_head() {
    let (plain, with) = (spec(false), spec(true));
    assert!(with.features().iter().any(|f| f.id.0 == "HEAD_TRANSFORM_V1") && !plain.features().iter().any(|f| f.id.0 == "HEAD_TRANSFORM_V1"));
    let (p0, p1) = (hl::build_program(&plain).unwrap(), hl::build_program(&with).unwrap());
    p1.validate().unwrap();
    let names: Vec<&str> = p1.params.iter().map(|d| d.name.as_str()).collect();
    for want in ["head.transform.dense.w", "head.transform.dense.b", "head.transform.norm.gain", "head.transform.norm.bias"] {
        assert!(names.contains(&want), "{want} is not a param: {names:?}");
    }
    assert_eq!(p1.params.len(), p0.params.len() + 4);
    // The binding reads the roles' tensors.
    let b = hf_weights::bind(&with, &p1).unwrap();
    let at = |n: &str| p1.params.iter().position(|d| d.name == n).unwrap();
    assert_eq!(format!("{:?}", b.srcs[at("head.transform.dense.w")]), "Tensor(\"head.transform.dense.weight\")");
    assert_eq!(format!("{:?}", b.srcs[at("head.transform.norm.bias")]), "Tensor(\"head.transform.norm.bias\")");
    // A different function (synthetic weights), and one that lowers.
    let a = Session::new(&p0, &ParamStore::synthetic(&p0, 3)).run(&TOKENS).unwrap();
    let c = Session::new(&p1, &ParamStore::synthetic(&p1, 3)).run(&TOKENS).unwrap();
    assert!(a.iter().zip(&c).any(|(x, y)| x.iter().zip(y).any(|(u, v)| (u - v).abs() > 1e-4)));
    assert!(prepare_spec(with, &LowerOpts::default()).is_ok());
}

#[test]
fn the_transform_belongs_to_a_language_model_head() {
    let mut s = spec(true);
    s.output = OutputSpec::Embedding { proj: None, normalize: false };
    let e = hl::build_program(&s).err().expect("refused").to_string();
    assert!(e.contains("HEAD_TRANSFORM_V1"), "{e}");
}
