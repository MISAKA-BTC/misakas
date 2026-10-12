//! # The diffusers read path: a component's `config.json` (`_class_name`) routed to the lowering that exists for it
//!
//! A transformers configuration names its class in `architectures`; a **diffusers** component (a denoiser, a VAE, a UNet) names it
//! in `_class_name`. Without this reader the architecture report and the corpus saw "config has no `architectures`: the reference
//! implementation cannot be identified" for every image-generation component — a reader gap, not a capability one. Here the class
//! routes to what exists (RFC-0003 §6's first image profile, [`crate::diffusion`]):
//!
//! * `SD3Transformer2DModel` → the MMDiT denoiser stage ([`crate::diffusion::dit`]), read by `Sd3Config::from_json`;
//! * `AutoencoderKL` → the VAE decoder's chain of stages ([`crate::diffusion::vae`]), read by `VaeConfig::from_json`;
//!
//! and every other class a refusal that names what its lowering lacks as registry capabilities (never "unknown model"): Flux's
//! three-axis rotary embedding, qk norm in the joint attention, single-stream blocks and guidance embedding; DiT's class-label
//! embedding, single-stream adaLN-Zero blocks, learned-sigma output and noise-prediction sampler; a UNet's skip connections,
//! time-conditioned resnets, spatial transformers with cross-attention. These are *Rust* routes — the lowerers are functions of
//! `(weights, calibration, configuration)` — so the report names the route (`AdapterSource::CoreReader`), and the check that needs
//! the weights is [`crate::diffusion::probe`].
//!
//! A key that might change the math is refused, not ignored (the rule of every reader of this crate): an unknown key of a
//! supported class is an unmapped key.

use super::{AdapterSource, MissingItem, ReadFailure};
use crate::diffusion::float::Sd3Config;
use crate::diffusion::vae_float::VaeConfig;
use crate::error::LowerError;
use serde_json::Value;

/// The class a diffusers component's configuration names (`_class_name`). `None` for a transformers configuration (it has
/// `architectures`) and for anything without the key.
pub fn diffusers_class(config: &Value) -> Option<&str> {
    if config.get("architectures").is_some() {
        return None;
    }
    config.get("_class_name").and_then(Value::as_str)
}

/// Whether the configuration is a diffusers component's.
pub fn is_diffusers(config: &Value) -> bool {
    diffusers_class(config).is_some()
}

/// A diffusers component an existing lowering reads.
#[derive(Clone, Debug, PartialEq)]
pub enum DiffusersRoute {
    /// `SD3Transformer2DModel`: the denoise stage.
    Sd3Transformer(Sd3Config),
    /// `AutoencoderKL`: the decoder's chain of stages.
    VaeDecoder(VaeConfig),
}

impl DiffusersRoute {
    /// The route's id, as the report's `AdapterSource::CoreReader` names it.
    pub fn id(&self) -> &'static str {
        match self {
            DiffusersRoute::Sd3Transformer(_) => "diffusers:sd3-transformer",
            DiffusersRoute::VaeDecoder(_) => "diffusers:autoencoder-kl-decoder",
        }
    }
}

/// A successful read.
#[derive(Clone, Debug)]
pub struct DiffusersRead {
    pub class: String,
    pub route: DiffusersRoute,
    /// Config keys the reading defaulted.
    pub assumed_defaults: Vec<String>,
}

fn missing(id: &str, why: &str) -> MissingItem {
    MissingItem { what: id.to_string(), why: why.to_string(), general_primitive: None }
}

fn refusal(class: &str, missing: Vec<MissingItem>, unmapped: Vec<String>, detail: &str) -> ReadFailure {
    let names = missing.iter().map(|m| m.what.clone()).collect::<Vec<_>>().join(", ");
    let msg = if names.is_empty() { format!("{class}: {detail}") } else { format!("{class}: {detail} (missing: {names})") };
    ReadFailure { error: LowerError::not_lowerable(msg), adapter: AdapterSource::None, unmapped_config_keys: unmapped, missing }
}

/// The keys of `config` outside `known` (the underscore keys diffusers writes are bookkeeping).
fn unknown_keys(config: &Value, known: &[&str]) -> Vec<String> {
    config
        .as_object()
        .map(|o| o.keys().filter(|k| !k.starts_with('_') && !known.contains(&k.as_str())).cloned().collect())
        .unwrap_or_default()
}

const SD3_KEYS: &[&str] = &[
    "sample_size",
    "patch_size",
    "in_channels",
    "out_channels",
    "num_layers",
    "attention_head_dim",
    "num_attention_heads",
    "joint_attention_dim",
    "caption_projection_dim",
    "pooled_projection_dim",
    "pos_embed_max_size",
    "qk_norm",
    "dual_attention_layers",
];

const VAE_KEYS: &[&str] = &[
    "act_fn",
    "block_out_channels",
    "down_block_types",
    "force_upcast",
    "in_channels",
    "latent_channels",
    "layers_per_block",
    "norm_num_groups",
    "out_channels",
    "sample_size",
    "scaling_factor",
    "shift_factor",
    "up_block_types",
    "use_post_quant_conv",
    "use_quant_conv",
    "mid_block_add_attention",
    "latents_mean",
    "latents_std",
];

/// Read a diffusers component's configuration: the lowering that exists for its class, or a refusal naming what is missing.
pub fn read_diffusers(config: &Value) -> Result<DiffusersRead, ReadFailure> {
    let class = diffusers_class(config).ok_or_else(|| {
        refusal(
            "a configuration without `_class_name`",
            Vec::new(),
            Vec::new(),
            "it is not a diffusers component's (a transformers configuration names its class in `architectures`)",
        )
    })?;
    match class {
        "SD3Transformer2DModel" => {
            let unknown = unknown_keys(config, SD3_KEYS);
            if !unknown.is_empty() {
                return Err(refusal(class, Vec::new(), unknown.clone(), &format!("config key(s) this reading does not model: {} — a key that might change the math is refused, not ignored", unknown.join(", "))));
            }
            let mut gaps = Vec::new();
            if config.get("qk_norm").is_some_and(|q| !q.is_null()) {
                gaps.push(missing("ATTN_QK_NORM_JOINT_V1", "SD3.5's joint attention norms q and k per stream (qk_norm)"));
            }
            if config.get("dual_attention_layers").and_then(Value::as_array).is_some_and(|d| !d.is_empty()) {
                gaps.push(missing("GEN_ATTN_DUAL_V1", "SD3.5's dual-attention layers (a second self-attention over the image stream)"));
            }
            if let (Some(i), Some(o)) = (config.get("in_channels").and_then(Value::as_u64), config.get("out_channels").and_then(Value::as_u64))
                && i != o
            {
                gaps.push(missing("GEN_OUTPUT_LEARNED_SIGMA_V1", "an output of other channels than the input's (a learned variance)"));
            }
            if !gaps.is_empty() {
                return Err(refusal(class, gaps, Vec::new(), "a later profile of the MMDiT denoiser"));
            }
            let assumed = ["out_channels"].iter().filter(|k| config.get(**k).is_none()).map(|k| format!("{k} = in_channels")).collect();
            let cfg = Sd3Config::from_json(config).map_err(|e| refusal(class, Vec::new(), Vec::new(), &e))?;
            Ok(DiffusersRead { class: class.to_string(), route: DiffusersRoute::Sd3Transformer(cfg), assumed_defaults: assumed })
        }
        "AutoencoderKL" => {
            let unknown = unknown_keys(config, VAE_KEYS);
            if !unknown.is_empty() {
                return Err(refusal(class, Vec::new(), unknown.clone(), &format!("config key(s) this reading does not model: {} — a key that might change the math is refused, not ignored", unknown.join(", "))));
            }
            // A normalisation the decoder would have to apply to the latent beyond `z / scaling + shift`.
            for k in ["latents_mean", "latents_std"] {
                if config.get(k).is_some_and(|v| !v.is_null()) {
                    return Err(refusal(class, vec![missing("GEN_VAE_LATENT_NORM_V1", "a per-channel latent mean and std applied before the decode")], Vec::new(), &format!("`{k}` is set")));
                }
            }
            // The decoder's up blocks are resnets and a nearest-neighbour upsample: an attention up block is another network.
            if let Some(types) = config.get("up_block_types").and_then(Value::as_array)
                && types.iter().any(|t| t.as_str() != Some("UpDecoderBlock2D"))
            {
                return Err(refusal(class, vec![missing("GEN_VAE_ATTN_UP_BLOCK_V1", "an up block that is not UpDecoderBlock2D")], Vec::new(), "the decoder's up blocks"));
            }
            let mut assumed = Vec::new();
            for (k, d) in [("latent_channels", "4"), ("layers_per_block", "1"), ("norm_num_groups", "32"), ("out_channels", "3")] {
                if config.get(k).is_none() {
                    assumed.push(format!("{k} = {d}"));
                }
            }
            let cfg = VaeConfig::from_json(config).map_err(|e| refusal(class, Vec::new(), Vec::new(), &e))?;
            if cfg.block_out_channels.is_empty() {
                return Err(refusal(class, Vec::new(), Vec::new(), "no block_out_channels"));
            }
            if cfg.block_out_channels.iter().any(|c| cfg.groups == 0 || c % cfg.groups != 0) {
                return Err(refusal(class, Vec::new(), Vec::new(), "norm_num_groups does not divide every block width"));
            }
            Ok(DiffusersRead { class: class.to_string(), route: DiffusersRoute::VaeDecoder(cfg), assumed_defaults: assumed })
        }
        "FluxTransformer2DModel" => Err(refusal(
            class,
            vec![
                missing("POS_ROPE_AXES_V1", "a rotary embedding over several position axes (axes_dims_rope: the ids of text and image tokens)"),
                missing("ATTN_QK_NORM_JOINT_V1", "an RMS norm of q and k per stream inside the joint attention"),
                missing("GEN_BLOCK_SINGLE_STREAM_V1", "single-stream blocks: attention and MLP in parallel over the concatenated streams"),
                missing("EMBED_GUIDANCE_V1", "the guidance scalar's embedding (guidance_embeds)"),
            ],
            Vec::new(),
            "the MMDiT of FLUX.1 reuses the joint attention and the adaLN of SD3's but not these",
        )),
        "DiTTransformer2DModel" => Err(refusal(
            class,
            vec![
                missing("EMBED_CLASS_LABEL_V1", "a class-label embedding with an unconditional row (num_embeds_ada_norm)"),
                missing("GEN_BLOCK_ADALN_ZERO_V1", "single-stream blocks: self-attention and MLP under adaLN-Zero"),
                missing("EMBED_POSITION_SINCOS_V1", "fixed 2-D sinusoidal positions"),
                missing("GEN_OUTPUT_LEARNED_SIGMA_V1", "an output of twice the channels: noise prediction and variance"),
                missing("GEN_SAMPLER_EPS_V1", "a noise-prediction sampler (DDPM/DDIM/PNDM) with classifier-free guidance"),
            ],
            Vec::new(),
            "the class-conditional DiT is not the MMDiT's joint-stream stage",
        )),
        "UNet2DConditionModel" => {
            let mut gaps = vec![
                missing("GEN_UNET_SKIP_V1", "skip connections from the down path to the up path (stage outputs read by later stages)"),
                missing("GEN_RESNET_TIME_COND_V1", "a ResNet block that adds the projected timestep embedding between its convolutions"),
                missing("GEN_SPATIAL_TRANSFORMER_V1", "a Transformer2DModel: GroupNorm, proj_in, self- and cross-attention blocks, GEGLU, proj_out"),
                missing("ATTN_CROSS_V1", "cross-attention of the image tokens to the text rows"),
                missing("GEN_SAMPLER_EPS_V1", "a noise-prediction sampler (DDIM/PNDM/Euler-discrete) with classifier-free guidance"),
            ];
            if config.get("addition_embed_type").is_some_and(|v| !v.is_null()) {
                gaps.push(missing("EMBED_ADDITION_TEXT_TIME_V1", "SDXL's added text and time conditioning (addition_embed_type)"));
            }
            Err(refusal(class, gaps, Vec::new(), "a UNet is a graph with skip connections over convolutions and spatial transformers"))
        }
        other => Err(refusal(other, Vec::new(), Vec::new(), "a diffusers class this crate has no reading for")),
    }
}
