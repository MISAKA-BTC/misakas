//! Hand-written `config.json` files of real checkpoints (`tests/configs/real/`), one or more per
//! listed architecture, written from the published configs (fields I was unsure of are listed in
//! `docs/design/palw/tir/hf-coverage.md`). Each must parse to the expected spec, build an HL
//! program, and bind every param to an HF tensor name — or be refused with the expected reason.

use misaka_palw_tir_lower::hl::{self, HlProgram, Op};
use misaka_palw_tir_lower::spec::*;
use misaka_palw_tir_lower::{LowerError, hf_weights, parse_config_str};
use std::path::Path;

fn load(name: &str) -> Result<ArchSpec, LowerError> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real").join(format!("{name}.json"));
    parse_config_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
}

fn ok(name: &str) -> (ArchSpec, HlProgram) {
    let s = load(name).unwrap_or_else(|e| panic!("{name}: {e}"));
    let p = hl::build_program(&s).unwrap_or_else(|e| panic!("{name}: {e}"));
    hf_weights::bind(&s, &p).unwrap_or_else(|e| panic!("{name}: bind: {e}"));
    (s, p)
}

fn refused(name: &str, needle: &str) {
    match load(name) {
        Err(LowerError::NotLowerable(s)) => assert!(s.contains(needle), "{name}: refusal `{s}` does not mention `{needle}`"),
        other => panic!("{name}: expected NOT_LOWERABLE({needle}), got {other:?}"),
    }
}

fn attn(s: &ArchSpec, l: usize) -> &AttnSpec {
    match &s.layers[l].mixer {
        Mixer::Attention(a) => a,
        m => panic!("layer {l} is {m:?}"),
    }
}

fn rope(a: &AttnSpec) -> &crate_rope::RopeSpec {
    match &a.position {
        Position::Rope(r) => r,
        p => panic!("no rope: {p:?}"),
    }
}
use misaka_palw_tir_lower::rope as crate_rope;

fn kinds(p: &HlProgram) -> usize {
    p.blocks.len() - 2
}

#[test]
fn llama_3_1_and_3_2() {
    let (s, p) = ok("llama-3.1-8b");
    assert_eq!((s.num_layers(), attn(&s, 0).heads, attn(&s, 0).kv_heads, attn(&s, 0).head_dim), (32, 32, 8, 128));
    assert_eq!(rope(attn(&s, 0)).freqs.rope_type, "llama3");
    assert_eq!(kinds(&p), 1);
    let (s, _) = ok("llama-3.2-1b");
    assert!(s.head.tied);
    assert_eq!(attn(&s, 0).head_dim, 64);
}

#[test]
fn mistral_sliding_window_and_nemo_head_dim() {
    let (s, _) = ok("mistral-7b-v0.1");
    assert_eq!(attn(&s, 0).window, Some(4096));
    let (s, _) = ok("mistral-nemo-12b");
    assert_eq!((attn(&s, 0).window, attn(&s, 0).head_dim), (None, 128), "Nemo: 32 heads × 128 ≠ hidden 5120");
}

#[test]
fn qwen2_biases_and_disabled_sliding_window() {
    let (s, _) = ok("qwen2.5-7b-instruct");
    let a = attn(&s, 0);
    assert!(a.q_bias && a.k_bias && a.v_bias && !a.o_bias);
    assert!(s.layers.iter().all(|l| matches!(&l.mixer, Mixer::Attention(a) if a.window.is_none())), "use_sliding_window=false");
    let (s, _) = ok("qwen2.5-0.5b");
    assert!(s.head.tied);
    let (s, _) = ok("qwen3-8b");
    assert_eq!(attn(&s, 0).qk_norm.map(|q| q.scope), Some(QkNormScope::PerHeadShared));
}

#[test]
fn gemma_family() {
    let (s, _) = ok("gemma-7b");
    assert_eq!(s.embedding.scale, (3072f64).sqrt());
    // transformers 5 reads `hidden_act` (exact gelu) for Gemma-1 and flags the disagreement.
    assert!(matches!(&s.layers[0].ffn, Ffn::Mlp(m) if m.act == Act::Gelu));
    assert!(matches!(s.confidence, Confidence::Unsure(_)));
    let (s, p) = ok("gemma-2-9b");
    assert_eq!((attn(&s, 0).window, attn(&s, 1).window), (Some(4096), None));
    assert_eq!((attn(&s, 0).softcap, s.head.softcap), (Some(50.0), Some(30.0)));
    assert!((attn(&s, 0).scale - 1.0 / 16.0).abs() < 1e-12);
    assert_eq!(kinds(&p), 2);
    let (s, p) = ok("gemma-3-1b-it");
    let local = rope(attn(&s, 0));
    let global = rope(attn(&s, 5));
    assert_eq!((attn(&s, 0).window, attn(&s, 5).window), (Some(512), None));
    assert_eq!((local.freqs.theta, global.freqs.theta), (10000.0, 1_000_000.0));
    assert_eq!(kinds(&p), 2);
    assert_eq!(p.rope_tables.len(), 2);
    let (s, _) = ok("gemma-3-4b-it");
    assert_eq!(s.num_layers(), 34);
    assert_eq!(rope(attn(&s, 5)).freqs.rope_type, "linear");
    assert_eq!(attn(&s, 0).window, Some(1024));
    assert!(s.hf.names["embed"].starts_with("language_model.model."));
}

#[test]
fn llava_text_decoder_is_llama_with_defaults() {
    let (s, _) = ok("llava-1.5-7b-hf");
    assert_eq!((s.num_layers(), s.hidden_size, s.vocab_size), (32, 4096, 32064), "LlamaConfig defaults fill the diffed text_config");
}

#[test]
fn phi_family() {
    let (s, _) = ok("phi-2");
    let a = attn(&s, 0);
    assert_eq!(rope(a).rotary_dim, 32, "partial 0.4 of head_dim 80");
    assert!(matches!(s.layers[0].residual, Residual::Parallel { ffn_norm: None, .. }));
    assert!(s.head.bias);
    let (s, _) = ok("phi-3.5-mini-instruct");
    let r = rope(attn(&s, 0));
    assert_eq!(r.freqs.rope_type, "longrope");
    assert_eq!(r.freqs.longrope.as_ref().map(|l| l.original_max), Some(4096));
    assert!((r.freqs.attention_factor - (1.0 + 32f64.ln() / 4096f64.ln()).sqrt()).abs() < 1e-12);
    assert_eq!(s.hf.qkv, QkvLayout::FusedConcat);
    let (s, _) = ok("phi-4");
    assert_eq!((attn(&s, 0).heads, attn(&s, 0).kv_heads), (40, 10));
}

#[test]
fn gpt2_neo_neox_j() {
    let (s, _) = ok("gpt2");
    assert!(s.hf.conv1d_weights && s.head.tied);
    assert_eq!(s.embedding.positions.as_ref().map(|p| p.rows), Some(1024));
    let (s, p) = ok("gpt-neo-125m");
    assert_eq!((attn(&s, 0).window, attn(&s, 1).window, attn(&s, 0).scale), (None, Some(256), 1.0));
    assert_eq!(kinds(&p), 2);
    let (s, _) = ok("pythia-1.4b");
    assert_eq!(rope(attn(&s, 0)).rotary_dim, 32);
    assert_eq!(s.hf.qkv, QkvLayout::FusedPerHead);
    let (s, _) = ok("gpt-j-6b");
    assert_eq!(rope(attn(&s, 0)).style, crate_rope::RopeStyle::Interleaved);
    assert!(s.head.bias);
}

#[test]
fn falcon_variants() {
    let (s, _) = ok("falcon-7b");
    assert_eq!((attn(&s, 0).kv_heads, s.hf.qkv), (1, QkvLayout::FusedConcat));
    assert!(matches!(s.layers[0].residual, Residual::Parallel { ffn_norm: None, .. }));
    let (s, _) = ok("falcon-40b");
    assert_eq!((attn(&s, 0).kv_heads, s.hf.qkv), (8, QkvLayout::FusedPerKvGroup));
    assert!(matches!(s.layers[0].residual, Residual::Parallel { ffn_norm: Some(_), .. }));
    let (s, _) = ok("falcon-rw-1b");
    assert!(matches!(&attn(&s, 0).position, Position::Alibi(a) if a.scaled_by_softmax_scale && a.bf16_bias));
}

#[test]
fn stablelm_starcoder_bigcode() {
    let (s, _) = ok("stablelm-2-1.6b");
    assert_eq!(rope(attn(&s, 0)).rotary_dim, 16);
    assert!(attn(&s, 0).q_bias);
    let (s, _) = ok("starcoder2-3b");
    assert_eq!(attn(&s, 0).window, Some(4096));
    let (s, _) = ok("starcoderbase-1b");
    assert_eq!(attn(&s, 0).kv_heads, 1);
}

#[test]
fn olmo_cohere_granite_nemotron() {
    let (s, _) = ok("olmo-1b-hf");
    assert_eq!(s.final_norm.map(|n| n.gain), Some(Gain::None));
    let (s, _) = ok("olmo-2-1124-7b");
    assert!(matches!(s.layers[0].residual, Residual::Sequential { pre_mixer: None, post_mixer: Some(_), .. }));
    assert_eq!(attn(&s, 0).qk_norm.map(|q| q.scope), Some(QkNormScope::Whole));
    let (s, _) = ok("c4ai-command-r-v01");
    assert_eq!((s.head.logit_scale, rope(attn(&s, 0)).style), (0.0625, crate_rope::RopeStyle::Interleaved));
    let (s, p) = ok("c4ai-command-r7b-12-2024");
    assert!(matches!(attn(&s, 3).position, Position::None), "Cohere2 global layers are NoPE");
    assert_eq!(attn(&s, 0).window, Some(4096));
    assert_eq!(kinds(&p), 2);
    let (s, _) = ok("granite-3.1-8b-instruct");
    assert_eq!((s.embedding.scale, attn(&s, 0).scale, s.head.logit_scale), (12.0, 0.0078125, 1.0 / 16.0));
    assert!(matches!(s.layers[0].residual, Residual::Sequential { multiplier, .. } if multiplier == 0.22));
    let (s, _) = ok("minitron-4b-base");
    assert!(matches!(&s.layers[0].ffn, Ffn::Mlp(m) if !m.gated && m.act == Act::Relu2));
    assert_eq!(s.final_norm.map(|n| n.gain), Some(Gain::OnePlusW));
}

#[test]
fn remote_code_llama_likes_are_lowered_but_marked_unsure() {
    for n in ["internlm2.5-7b-chat", "minicpm-2b-sft-bf16", "exaone-3.5-2.4b-instruct"] {
        let (s, _) = ok(n);
        assert!(matches!(s.reference, Reference::RemoteCode { .. }), "{n}");
        assert!(matches!(s.confidence, Confidence::Unsure(_)), "{n}: no fixture exists offline, so it must say so");
    }
    let (s, _) = ok("internlm2.5-7b-chat");
    assert_eq!(s.hf.qkv, QkvLayout::FusedPerKvGroup);
    assert!(rope(attn(&s, 0)).freqs.dynamic.is_some());
    let (s, _) = ok("minicpm-2b-sft-bf16");
    assert_eq!(s.embedding.scale, 12.0);
    assert!((s.head.pre_scale - 256.0 / 2304.0).abs() < 1e-12);
}

#[test]
fn exaone4_and_smollm3() {
    let (s, _) = ok("exaone-4.0-1.2b");
    assert!(
        s.layers.iter().all(|l| matches!(&l.mixer, Mixer::Attention(a) if matches!(a.position, Position::Rope(_)))),
        "no sliding window ⇒ rope everywhere"
    );
    let (s, p) = ok("smollm3-3b");
    assert!(matches!(attn(&s, 3).position, Position::None));
    assert!(matches!(attn(&s, 2).position, Position::Rope(_)));
    assert_eq!(kinds(&p), 2);
}

#[test]
fn legacy_alibi_models() {
    let (s, _) = ok("bloom-560m");
    assert!(s.embedding.norm.is_some());
    assert!(matches!(&attn(&s, 0).position, Position::Alibi(a) if !a.scaled_by_softmax_scale));
    let (s, _) = ok("mpt-7b");
    assert!(matches!(&attn(&s, 0).position, Position::Alibi(_)));
    assert!(matches!(s.reference, Reference::RemoteCode { .. }));
    let (s, _) = ok("opt-125m");
    assert_eq!(s.embedding.positions.as_ref().map(|p| (p.rows, p.offset)), Some((2050, 2)));
    let (s, _) = ok("opt-350m");
    assert!(s.embedding.proj_in && s.head.proj_out && s.final_norm.is_none());
    assert!(matches!(s.layers[0].residual, Residual::PostNorm { .. }));
}

fn moe_of(s: &ArchSpec, l: usize) -> &MoeSpec {
    match &s.layers[l].ffn {
        Ffn::Moe(m) => m,
        f => panic!("layer {l}: {f:?}"),
    }
}

#[test]
fn moe_models() {
    let (s, _) = ok("mixtral-8x7b-v0.1");
    assert_eq!((moe_of(&s, 0).experts, moe_of(&s, 0).top_k, moe_of(&s, 0).router.normalize), (8, 2, true));
    let (s, _) = ok("qwen1.5-moe-a2.7b");
    let m = moe_of(&s, 0);
    assert!(m.shared.as_ref().map(|x| x.sigmoid_gate).unwrap_or(false) && !m.router.normalize);
    let (s, _) = ok("qwen3-30b-a3b");
    assert!(moe_of(&s, 0).router.normalize);
    let (s, _) = ok("olmoe-1b-7b-0924");
    assert_eq!(moe_of(&s, 0).top_k, 8);
    let (s, _) = ok("granite-3.1-3b-a800m");
    assert_eq!(moe_of(&s, 0).router.scoring, Scoring::TopKThenSoftmax);
    assert_eq!(s.hf.experts, MlpLayout::FusedGateFirst);
}

#[test]
fn deepseek_mla_and_routing() {
    let (s, p) = ok("deepseek-v2-lite");
    assert!(matches!(&s.layers[0].ffn, Ffn::Mlp(_)) && matches!(&s.layers[1].ffn, Ffn::Moe(_)));
    let Mixer::Mla(m) = &s.layers[0].mixer else { panic!() };
    assert_eq!((m.q_lora_rank, m.kv_lora_rank, m.qk_rope_head_dim), (None, 512, 64));
    // yarn_apply_mscale: (0.1·0.707·ln 40 + 1)² on 1/√192.
    let ms = 0.1 * 0.707 * 40f64.ln() + 1.0;
    assert!((m.scale - ms * ms / 192f64.sqrt()).abs() < 1e-12);
    assert_eq!(moe_of(&s, 1).router.groups, None, "greedy");
    assert_eq!(kinds(&p), 2);
    refused("deepseek-v3-fp8", "quant_method=fp8");
    let (s, _) = ok("deepseek-v3-bf16");
    let r = &moe_of(&s, 3).router;
    assert_eq!((r.scoring, r.selection_bias, r.scale), (Scoring::Sigmoid, true, 2.5));
    assert_eq!(r.groups.map(|g| (g.n_group, g.topk_group, g.score)), Some((8, 4, GroupScore::Top2Sum)));
    assert!(matches!(&s.layers[2].ffn, Ffn::Mlp(_)));
}

#[test]
fn gpt_oss_quantised_is_refused_and_bf16_export_lowers() {
    refused("gpt-oss-20b-mxfp4", "quant_method=mxfp4");
    let (s, p) = ok("gpt-oss-20b-bf16");
    let a = attn(&s, 0);
    assert!(a.sinks && a.window == Some(128) && attn(&s, 1).window.is_none());
    assert_eq!(rope(a).freqs.rope_type, "yarn");
    assert!(matches!(moe_of(&s, 0).glu, Glu::ClampedSwiGlu { .. }));
    assert_eq!(kinds(&p), 2);
}

#[test]
fn hybrids_and_ssms() {
    let (s, p) = ok("qwen3-next-80b-a3b-instruct");
    let gdn_layers = s.layers.iter().filter(|l| matches!(l.mixer, Mixer::GatedDeltaNet(_))).count();
    assert_eq!((gdn_layers, s.num_layers()), (36, 48));
    let Mixer::GatedDeltaNet(g) = &s.layers[0].mixer else { panic!() };
    assert_eq!((g.k_heads, g.v_heads, g.head_map), (16, 32, HeadMap::Group));
    assert!(attn(&s, 3).output_gate);
    assert_eq!(kinds(&p), 2);
    let (s, p) = ok("jamba-v0.1");
    let attn_layers: Vec<usize> = (0..32).filter(|l| matches!(s.layers[*l].mixer, Mixer::Attention(_))).collect();
    assert_eq!(attn_layers, vec![4, 12, 20, 28]);
    assert!(matches!(attn(&s, 4).position, Position::None));
    assert_eq!(kinds(&p), 3, "attn+mlp (attention layers are even, experts odd), mamba+mlp, mamba+moe");
    let (s, _) = ok("mamba-130m-hf");
    assert!(matches!(&s.layers[0].mixer, Mixer::Mamba(m) if m.inner == 1536 && m.dt_rank == 48));
    let (s, _) = ok("falcon-mamba-7b");
    assert!(matches!(&s.layers[0].mixer, Mixer::Mamba(m) if m.bcdt_norm.map(|n| n.gain) == Some(Gain::None)));
    let (s, _) = ok("mamba-codestral-7b-v0.1");
    assert!(matches!(&s.layers[0].mixer, Mixer::Mamba2(m) if m.groups == 8 && m.dt_max.is_infinite()));
    let (s, p) = ok("rwkv-4-169m-pile");
    assert_eq!(s.layers.iter().filter(|l| l.post_scale == 0.5).count(), 2, "rescale_every 6 over 12 layers");
    assert_eq!(kinds(&p), 2);
}

#[test]
fn refusals_name_their_reason() {
    refused("rwkv-6-finch-1b6", "remote code");
    refused("rwkv7-fla-1.5b", "flash-linear-attention");
    refused("t5-small", "encoder–decoder");
    refused("bert-base-uncased", "no lowerer template");
    refused("gemma-3n-e4b", "AltUp");
    refused("llama-3.1-8b-gptq", "quant_method=gptq");
}

#[test]
fn an_unknown_key_or_unimplemented_value_is_refused_not_ignored() {
    let base = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real/llama-3.1-8b.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap();
    v["use_qk_norm"] = serde_json::json!(true);
    assert!(matches!(misaka_palw_tir_lower::parse_config(&v), Err(LowerError::NotLowerable(s)) if s.contains("use_qk_norm")));
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap();
    v["hidden_act"] = serde_json::json!("xielu");
    assert!(matches!(misaka_palw_tir_lower::parse_config(&v), Err(LowerError::NotLowerable(s)) if s.contains("xielu")));
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap();
    v["rope_scaling"] = serde_json::json!({"rope_type": "llama4_chunked", "factor": 2.0});
    assert!(matches!(misaka_palw_tir_lower::parse_config(&v), Err(LowerError::NotLowerable(s)) if s.contains("llama4_chunked")));
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap();
    v["rope_scaling"]["mystery"] = serde_json::json!(1);
    assert!(matches!(misaka_palw_tir_lower::parse_config(&v), Err(LowerError::NotLowerable(s)) if s.contains("mystery")));
}

#[test]
fn every_real_config_has_a_named_expectation() {
    // A config dropped into tests/configs/real must be exercised by a test above.
    let src = include_str!("real_configs.rs");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().trim_end_matches(".json").to_string();
        assert!(src.contains(&format!("\"{n}\"")), "tests/configs/real/{n}.json is not checked by any test");
    }
}

#[test]
fn hl_ops_carry_no_checkpoint_names() {
    // Frontend neutrality: nothing in an HL program may mention an HF tensor name.
    let (s, p) = ok("qwen3-next-80b-a3b-instruct");
    let text = serde_json::to_string(&p).unwrap();
    for name in s.hf.names.values() {
        let stem = name.split('{').next().unwrap_or(name);
        assert!(stem.len() < 6 || !text.contains(stem), "HL program mentions `{stem}`");
    }
    assert!(p.blocks.iter().flat_map(|b| &b.nodes).any(|n| matches!(n.op, Op::GatedDelta { .. })));
}
