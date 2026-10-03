//! **The diffusers read path**: a diffusers component's `config.json` (`_class_name`, no `architectures`) routed to the lowering
//! that exists for its class, or refused with the capabilities its lowering lacks named — never "the reference implementation
//! cannot be identified".
//!
//! * `SD3Transformer2DModel` (the MMDiT denoiser) and `AutoencoderKL` (the VAE decoder) read at Level B through a built-in Rust
//!   route and, with their weights, lower and admit (`diffusion::probe`, on the reduced SD3 fixture of `tests/diffusers_sd3.rs`);
//! * SD3.5's qk norm and dual attention, FLUX, the class-conditional DiT and the UNets (SD 1.x/2.x and SDXL) are Level C,
//!   each naming registry capabilities that are `Missing`;
//! * an unknown key of a supported class is refused, not ignored.

use misaka_palw_tir_lower::diffusion::probe::{probe_sd3_transformer, probe_vae_decoder};
use misaka_palw_tir_lower::hf_schema::{AdapterSource, DiffusersRoute, Level, ReadOptions, diffusers_class, is_diffusers, read_diffusers};
use misaka_palw_tir_lower::model::{FeatureStatus, Lowering, ReportResult, analyze, feature_info};
use misaka_palw_tir_lower::weights::Checkpoint;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn config(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/diffusers").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("json")
}

fn fixture(part: &str) -> Option<PathBuf> {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-diff/sd3_tiny").join(part);
    if d.join("diffusion_pytorch_model.safetensors").exists() {
        Some(d)
    } else {
        eprintln!("skipped: {} holds no generated fixture (tools/gen_diffusers_sd3_fixture.py)", d.display());
        None
    }
}

#[test]
fn a_diffusers_config_is_recognised_by_its_class_name_not_by_architectures() {
    for n in ["sd3-medium-transformer", "sdxl-vae", "flux1-dev-transformer", "sd1.5-unet"] {
        let c = config(n);
        assert!(is_diffusers(&c), "{n}");
        assert!(diffusers_class(&c).is_some_and(|k| !k.is_empty()), "{n}");
    }
    // A transformers configuration names its class in `architectures`: never a diffusers one.
    let llama = serde_json::json!({"architectures": ["LlamaForCausalLM"], "_class_name": "Whatever"});
    assert!(!is_diffusers(&llama));
    assert!(!is_diffusers(&serde_json::json!({"model_type": "llama"})));
}

#[test]
fn sd3_and_the_vae_read_at_level_b_through_a_built_in_route_with_their_features() {
    for (name, route, features) in [
        (
            "sd3-medium-transformer",
            "diffusers:sd3-transformer",
            vec!["PATCH_EMBED_V1", "EMBED_TIMESTEP_TABLE_V1", "MOD_ADALN_V1", "ATTN_JOINT_STREAMS_V1", "ACT_TABLE_V1", "GEN_SAMPLER_AFFINE_V1"],
        ),
        ("sdxl-vae", "diffusers:autoencoder-kl-decoder", vec!["CONV_DENSE_V1", "NORM_GROUP_SPATIAL_V1", "ACT_TABLE_V1", "GEN_STAGE_VAE_V1"]),
        ("sd3-vae", "diffusers:autoencoder-kl-decoder", vec!["CONV_DENSE_V1", "NORM_GROUP_SPATIAL_V1", "ACT_TABLE_V1", "GEN_STAGE_VAE_V1"]),
    ] {
        let r = analyze(&config(name), None, &ReadOptions::default());
        assert_eq!((r.level, &r.result), (Level::B, &ReportResult::Lowerable), "{name}: {}", r.render());
        assert!(matches!(&r.adapter, AdapterSource::CoreReader { id } if id == route), "{name}: {:?}", r.adapter);
        let ids: Vec<&str> = r.features.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids.len(), features.len(), "{name}: {ids:?}");
        for f in features {
            assert!(ids.contains(&f), "{name}: {f} missing from {ids:?}");
        }
        assert!(r.features.iter().all(|f| f.status == FeatureStatus::Supported), "{name}: {:#?}", r.features);
        assert!(!r.new_consensus_primitive_required && !r.new_court_kernel_required);
    }
    // The routes carry the configuration the lowerers read.
    let r = read_diffusers(&config("sd3-medium-transformer")).expect("sd3 reads");
    let DiffusersRoute::Sd3Transformer(c) = r.route else { panic!("not the SD3 route") };
    assert_eq!((c.width(), c.num_layers, c.grid()), (1536, 24, 64));
    let r = read_diffusers(&config("sdxl-vae")).expect("vae reads");
    let DiffusersRoute::VaeDecoder(c) = r.route else { panic!("not the VAE route") };
    assert_eq!((c.block_out_channels.len(), c.upscale(), c.latent_channels), (4, 8, 4));
}

#[test]
fn sd3_5_is_level_c_naming_qk_norm_and_dual_attention() {
    let r = analyze(&config("sd3.5-large-transformer"), None, &ReadOptions::default());
    assert_eq!(r.level, Level::C, "{}", r.render());
    assert!(r.missing.iter().any(|m| m.what == "ATTN_QK_NORM_JOINT_V1"), "{:?}", r.missing);
    let r = analyze(&config("sd3.5-medium-transformer"), None, &ReadOptions::default());
    let names: Vec<&str> = r.missing.iter().map(|m| m.what.as_str()).collect();
    assert!(names.contains(&"ATTN_QK_NORM_JOINT_V1") && names.contains(&"GEN_ATTN_DUAL_V1"), "{names:?}");
}

#[test]
fn the_vae_refuses_what_its_decoder_does_not_lower() {
    let mut c = config("sdxl-vae");
    c["latents_mean"] = serde_json::json!([0.0, 0.0, 0.0, 0.0]);
    let r = analyze(&c, None, &ReadOptions::default());
    assert!(r.missing.iter().any(|m| m.what == "GEN_VAE_LATENT_NORM_V1"), "{:?}", r.missing);
    let mut c = config("sdxl-vae");
    c["up_block_types"] = serde_json::json!(["UpDecoderBlock2D", "AttnUpDecoderBlock2D", "UpDecoderBlock2D", "UpDecoderBlock2D"]);
    let r = analyze(&c, None, &ReadOptions::default());
    assert!(r.missing.iter().any(|m| m.what == "GEN_VAE_ATTN_UP_BLOCK_V1"), "{:?}", r.missing);
    let mut c = config("sdxl-vae");
    c["act_fn"] = serde_json::json!("gelu");
    assert_eq!(analyze(&c, None, &ReadOptions::default()).level, Level::C);
    let mut c = config("sdxl-vae");
    c["norm_num_groups"] = serde_json::json!(7);
    assert_eq!(analyze(&c, None, &ReadOptions::default()).level, Level::C);
}

/// FLUX, the class-conditional DiT and the UNets are Level C with the capabilities their lowering lacks named — each a registry
/// entry that is `Missing`, so the name means something and the vocabulary says what closes it.
#[test]
fn flux_dit_and_the_unets_are_level_c_naming_what_their_lowering_lacks() {
    for (name, want) in [
        ("flux1-dev-transformer", vec!["POS_ROPE_AXES_V1", "ATTN_QK_NORM_JOINT_V1", "GEN_BLOCK_SINGLE_STREAM_V1", "EMBED_GUIDANCE_V1"]),
        ("dit-xl-2-256", vec!["EMBED_CLASS_LABEL_V1", "GEN_BLOCK_ADALN_ZERO_V1", "GEN_OUTPUT_LEARNED_SIGMA_V1", "GEN_SAMPLER_EPS_V1"]),
        ("sd1.5-unet", vec!["GEN_UNET_SKIP_V1", "GEN_RESNET_TIME_COND_V1", "GEN_SPATIAL_TRANSFORMER_V1", "ATTN_CROSS_V1"]),
        ("sdxl-unet", vec!["GEN_UNET_SKIP_V1", "ATTN_CROSS_V1", "EMBED_ADDITION_TEXT_TIME_V1"]),
    ] {
        let r = analyze(&config(name), None, &ReadOptions::default());
        assert_eq!(r.level, Level::C, "{name}: {}", r.render());
        assert!(matches!(&r.result, ReportResult::NotLowerable { reason } if reason.contains("missing:")), "{name}: {:?}", r.result);
        let names: Vec<&str> = r.missing.iter().map(|m| m.what.as_str()).collect();
        for w in want {
            assert!(names.contains(&w), "{name}: {w} not named in {names:?}");
        }
        for m in &r.missing {
            let info = feature_info(&m.what).unwrap_or_else(|| panic!("{name}: `{}` is not in the registry", m.what));
            assert_eq!(info.lowering, Lowering::Missing, "{}", m.what);
        }
    }
    // The SDXL UNet is the SD 1.x one plus a named addition.
    let a = analyze(&config("sd1.5-unet"), None, &ReadOptions::default()).missing.len();
    let b = analyze(&config("sdxl-unet"), None, &ReadOptions::default()).missing.len();
    assert_eq!(b, a + 1);
}

#[test]
fn an_unknown_key_or_class_is_refused_not_ignored() {
    let mut c = config("sd3-medium-transformer");
    c["rope_scaling"] = serde_json::json!({"type": "linear"});
    let f = read_diffusers(&c).unwrap_err();
    assert!(f.unmapped_config_keys.contains(&"rope_scaling".to_string()), "{:?}", f.unmapped_config_keys);
    let f = read_diffusers(&serde_json::json!({"_class_name": "WanTransformer3DModel"})).unwrap_err();
    assert!(f.error.to_string().contains("WanTransformer3DModel"), "{}", f.error);
    let f = read_diffusers(&serde_json::json!({"model_type": "llama"})).unwrap_err();
    assert!(f.error.to_string().contains("not a diffusers component"), "{}", f.error);
}

/// With the weights: the reduced SD3 fixture's denoiser and decoder are read, lowered and ADMITTED by the probe, and every
/// tensor of the transformer is accounted for (the decoder reads none of the encoder's, by design).
#[test]
fn the_reduced_sd3_checkpoint_is_lowered_and_admitted_by_the_probe() {
    if let Some(d) = fixture("transformer") {
        let cfg: Value = serde_json::from_slice(&std::fs::read(d.join("config.json")).unwrap()).unwrap();
        assert_eq!(analyze(&cfg, None, &ReadOptions::default()).level, Level::B);
        let ck = Checkpoint::open(&d.join("diffusion_pytorch_model.safetensors")).expect("checkpoint");
        let p = probe_sd3_transformer(&cfg, &ck).expect("the denoiser lowers");
        eprintln!("sd3 transformer: {:?}", p.stages);
        assert!(p.ok(), "{p:?}");
        assert!(p.stages[0].nodes > 100);
    }
    if let Some(d) = fixture("vae") {
        let cfg: Value = serde_json::from_slice(&std::fs::read(d.join("config.json")).unwrap()).unwrap();
        assert_eq!(analyze(&cfg, None, &ReadOptions::default()).level, Level::B);
        let ck = Checkpoint::open(&d.join("diffusion_pytorch_model.safetensors")).expect("checkpoint");
        let p = probe_vae_decoder(&cfg, &ck, Some(8)).expect("the decoder lowers");
        eprintln!("vae decoder: {} stages, {:?}", p.stages.len(), p.stages.iter().map(|s| s.nodes).collect::<Vec<_>>());
        assert!(p.ok(), "{p:?}");
        assert!(p.stages.len() >= 8, "the decoder is a chain of stages");
    }
}

/// A checkpoint that does not fit the reading is a refusal by message, not a panic of the float reference.
#[test]
fn a_checkpoint_that_does_not_fit_is_refused_by_message_not_a_panic() {
    let Some(d) = fixture("transformer") else { return };
    let mut cfg: Value = serde_json::from_slice(&std::fs::read(d.join("config.json")).unwrap()).unwrap();
    // More layers than the checkpoint has.
    cfg["num_layers"] = serde_json::json!(5);
    let ck = Checkpoint::open(&d.join("diffusion_pytorch_model.safetensors")).expect("checkpoint");
    let e = probe_sd3_transformer(&cfg, &ck).unwrap_err();
    assert!(e.contains("does not fit"), "{e}");
}
