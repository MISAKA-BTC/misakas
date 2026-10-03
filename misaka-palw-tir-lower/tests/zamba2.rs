//! **`ATTN_SHARED_BLOCK_V1`, `EMBED_CARRY_V1`, `LAYER_PRE_BRANCH_V1`, `LINEAR_LOWRANK_ADAPTER_V1`** (FR-13): Zamba2's hybrid layers run a transformer block
//! SHARED across depths. The equality with `transformers`' `Zamba2ForCausalLM` is the tiny fixture's (`hf_fixtures::zamba2`, `fidelity_tiny::zamba2`) and the
//! corpus entry `zamba2`; here the properties that make it a feature: the shared weights are ONE set of integer tensors in the artifact, every depth keeps its own
//! KV history and adapters, the program is the same on the three implementations, and the spec follows the config's tying.

mod common;

use common::three_ways;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{Mixer, ModelSpec};
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{fidelity, hl, parse_config_str};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/zamba2")
}

fn config() -> String {
    std::fs::read_to_string(dir().join("config.json")).unwrap()
}

fn spec_of(cfg: &str) -> ModelSpec {
    parse_config_str(cfg).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_zamba2_config_reads_as_mamba_layers_with_a_shared_branch_on_the_hybrid_ones() {
    let s = spec_of(&config());
    assert!(s.embed_carry);
    let groups: Vec<Option<(usize, usize, usize)>> =
        s.layers.iter().map(|l| l.pre_branch.as_ref().map(|p| (p.group, p.weights_layer, p.ordinal))).collect();
    // Hybrid layers 1, 3, 4: ordinals 0, 1, 2 over two shared blocks (0, 1, 0); block 0's weights are stored at layer 1, block 1's at layer 3.
    assert_eq!(groups, [None, Some((0, 1, 0)), None, Some((1, 3, 1)), Some((0, 1, 2)), None]);
    assert!(s.layers.iter().all(|l| matches!(l.mixer, Mixer::Mamba2(_))));
    let pb = s.layers[1].pre_branch.as_ref().unwrap();
    assert_eq!(pb.attn.in_dim, Some(2 * s.hidden_size));
    assert!((pb.attn.scale - (pb.attn.head_dim as f64 / 2.0).powf(-0.5)).abs() < 1e-15, "the scale is (head_dim / 2)^-1/2");
    assert_eq!(pb.lowrank.map(|l| (l.rank, l.attn)), Some((4, true)));
    let ids: Vec<&str> = s.features().iter().map(|u| u.id.0).collect();
    for want in ["ATTN_SHARED_BLOCK_V1", "EMBED_CARRY_V1", "LAYER_PRE_BRANCH_V1", "LINEAR_LOWRANK_ADAPTER_V1", "TENSOR_NAME_ALTERNATIVES_V1"] {
        assert!(ids.contains(&want), "{want} in {ids:?}");
    }
}

#[test]
fn the_shared_weights_are_global_params_and_the_adapters_and_the_projection_are_each_layers_own() {
    let s = spec_of(&config());
    let p = hl::build_program(&s).unwrap();
    let find = |n: &str| p.params.iter().find(|d| d.name == n).unwrap_or_else(|| panic!("no param `{n}`"));
    for g in ["pb0", "pb1"] {
        for role in ["attn.q.w", "attn.o.w", "mlp.gate.w", "mlp.down.w", "in_norm.gain", "mid_norm.gain"] {
            assert!(!find(&format!("{g}.{role}")).per_layer, "{g}.{role} is shared");
        }
    }
    assert!(find("pb.out.w").per_layer, "the D x D projection is the layer's own");
    // Three hybrid layers, three adapters: named by ordinal, bound to their own tensors, and one block each (the program has a block per depth).
    for ord in 0..3 {
        assert!(find(&format!("pb{}.d{ord}.mlp.gate.lora_a", [0, 1, 0][ord])).per_layer);
    }
    assert!(p.carries.iter().any(|c| c.name == "e0"));
}

#[test]
fn a_shared_block_is_one_set_of_integer_tensors_in_the_artifact_and_each_depth_has_its_own_history() {
    let cfg = config();
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap();
    let names: Vec<&str> = prep.lowered.program.params.iter().map(|d| d.name.as_str()).collect();
    // The shared q projection exists once per group, not once per use: block 0 is used by two layers.
    let q: Vec<&&str> = names.iter().filter(|n| n.starts_with("pb0.attn.q.w")).collect();
    assert_eq!(q.len(), 1, "{q:?}");
    let q1: Vec<&&str> = names.iter().filter(|n| n.starts_with("pb1.attn.q.w")).collect();
    assert_eq!(q1.len(), 1, "{q1:?}");
    // Every hybrid depth keeps its own KV history (a per-layer state of the block it runs).
    let hist = prep.lowered.program.states.iter().filter(|s| s.name.contains("pb0.attn.k_hist")).count();
    assert!(hist >= 1);
    // pre, post, the plain Mamba layers' block and one block per hybrid depth.
    assert_eq!(prep.lowered.program.blocks.len(), 2 + 1 + 3);
}

#[test]
fn the_zamba2_fixture_program_is_the_same_on_the_reference_ref2_and_exec() {
    let cfg = config();
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap();
    let ck = Checkpoint::open(&dir()).unwrap();
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
    let loader = Resident(Arc::new(params));
    let quiet = |_: usize, _: usize| {};
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24, 11);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
    let eval = fidelity::random_sequences(prep.hl.vocab, 2, 16, 97);
    let n = three_ways(&prep.lowered.program, &mat.params, &eval).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(n, 32);
}

#[test]
fn an_untied_zamba2_config_has_independent_copies_because_transformers_ties_the_blocks_through_tie_word_embeddings() {
    // transformers 5.17 ties the shared blocks only when `tie_word_embeddings` is set: untied, every hybrid layer holds a block of its own, read from its
    // own module and with its own adapters.
    let cfg = config().replace("\"tie_word_embeddings\": true", "\"tie_word_embeddings\": false");
    let s = spec_of(&cfg);
    let groups: Vec<Option<(usize, usize, usize)>> =
        s.layers.iter().map(|l| l.pre_branch.as_ref().map(|p| (p.group, p.weights_layer, p.ordinal))).collect();
    assert_eq!(groups, [None, Some((0, 1, 0)), None, Some((1, 3, 1)), Some((2, 4, 2)), None]);
}

#[test]
fn a_zamba2_config_without_a_layer_list_must_be_the_54_layer_default() {
    let mut v: serde_json::Value = serde_json::from_str(&config()).unwrap();
    v.as_object_mut().unwrap().remove("layers_block_type");
    v.as_object_mut().unwrap().remove("hybrid_layer_ids");
    let e = parse_config_str(&v.to_string()).unwrap_err().to_string();
    assert!(e.contains("54"), "{e}");
    // The default pattern: hybrid layers 6, 12, ..., 42, 47 and 51 of 54.
    v.as_object_mut().unwrap().insert("num_hidden_layers".into(), 54.into());
    v.as_object_mut().unwrap().insert("num_mem_blocks".into(), 2.into());
    let s = spec_of(&v.to_string());
    let hybrid: Vec<usize> = (0..54).filter(|l| s.layers[*l].pre_branch.is_some()).collect();
    assert_eq!(hybrid, [6, 12, 18, 24, 30, 36, 42, 47, 51]);
    assert_eq!(s.layers[12].pre_branch.as_ref().map(|p| (p.group, p.weights_layer)), Some((1, 12)));
    assert_eq!(s.layers[18].pre_branch.as_ref().map(|p| (p.group, p.weights_layer)), Some((0, 6)));
}
