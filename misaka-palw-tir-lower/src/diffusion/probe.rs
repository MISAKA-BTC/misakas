//! **Read, lower and admit a diffusers component from its checkpoint** — the check a diffusers-aware reader
//! ([`crate::hf_schema::read_diffusers`]) can only make with the weights: the first image profile's lowerers are a function of
//! `(weights, calibration, configuration)` and nothing else, so lowering a component needs its tensors and a calibration set.
//!
//! The probe calibrates on seeded random inputs (the latent, the timestep, the text rows and the pooled vector of the denoiser; a
//! latent for the decoder): the programs it builds are the real ones in shape and in every integer parameter's dtype, the scales
//! are those of random data, and admission (`tir_admit_program_v2`) depends on the shapes, not on the scales. It is non-consensus
//! tooling, like [`super::fixture`]: nothing here is on a validation path. A real conversion calibrates on its prompts
//! (`fixture::build_sd3_tiny`).
//!
//! It also answers "is every tensor of the checkpoint accounted for?": the float references note the tensors they read
//! ([`super::float::Dit::untouched`]), so a tensor of the checkpoint that neither the float run nor the lowering read is a feature
//! the reading lacks — named, never silently dropped. (A VAE decoder reads no `encoder.*` or `quant_conv.*` tensor by design.)

use serde_json::Value;

use super::calib::Calib;
use super::dit::{DitStageSpec, lower_dit_stage};
use super::float::{Dit, DitInputs, Sd3Config};
use super::sampler::SamplerTables;
use super::vae::lower_vae;
use super::vae_float::{Fm, Vae, VaeConfig};
use crate::weights::TensorSource;

/// The text rows the probe's denoiser stage reads (a padded prompt's length).
pub const PROBE_TEXT_ROWS: usize = 8;
/// The offered step counts of the probe's denoiser.
pub const PROBE_COUNTS: [u32; 2] = [2, 4];

/// One lowered stage and its admission.
#[derive(Clone, Debug)]
pub struct StageVerdict {
    pub stage: String,
    pub nodes: usize,
    pub blocks: usize,
    /// `Ok`: the position's MACs and step leaves; `Err`: the ceiling it exceeds, by name.
    pub admission: Result<(f64, u64), String>,
}

/// What a probe found.
#[derive(Clone, Debug)]
pub struct Probe {
    pub stages: Vec<StageVerdict>,
    /// Checkpoint tensors no stage reads and that the reading does not set aside by design.
    pub unread: Vec<String>,
}

impl Probe {
    /// Every stage admitted and every tensor accounted for.
    pub fn ok(&self) -> bool {
        self.unread.is_empty() && self.stages.iter().all(|s| s.admission.is_ok())
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }
    /// A float in `[-1, 1]`.
    fn unit(&mut self) -> f64 {
        (self.next() % 2_000_001) as f64 / 1_000_000.0 - 1.0
    }
}

fn admit(name: &str, p: &misaka_palw_tir::program_v2::TirProgramV2) -> StageVerdict {
    let inputs = crate::admission::default_inputs();
    let admission = match misaka_palw_tir::admit_v2::tir_admit_program_v2(p, &inputs) {
        Ok(a) => Ok((a.view.position.cost.macs as f64, a.view.position.step_leaves)),
        Err(e) => Err(e.to_string()),
    };
    StageVerdict { stage: name.to_string(), nodes: p.blocks.iter().map(|b| b.nodes.len()).sum(), blocks: p.blocks.len(), admission }
}

/// Run `f`, turning a panic of the float references (a missing tensor, a shape that does not fit) into a refusal by message.
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let m = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            Err(format!("{what}: {m}"))
        }
    }
}

/// `SD3Transformer2DModel` (`transformer/config.json` and its checkpoint) lowered as the denoise stage and admitted.
pub fn probe_sd3_transformer(config: &Value, src: &dyn TensorSource) -> Result<Probe, String> {
    let cfg = Sd3Config::from_json(config)?;
    guarded("the checkpoint does not fit the SD3 reading", || {
        let dit = Dit::load(cfg.clone(), src)?;
        let (shift, n_train) = (3.0, 1000.0);
        let tables = SamplerTables::new(&PROBE_COUNTS, shift, n_train);
        let mut cal = Calib::new();
        let mut rng = Lcg(0x5d3);
        let (c, side) = (cfg.in_channels, cfg.sample_size);
        for k in 0..3usize {
            let latent: Vec<f64> = (0..c * side * side).map(|_| rng.unit() * 2.0).collect();
            let text: Vec<f64> = (0..PROBE_TEXT_ROWS * cfg.joint_dim).map(|_| rng.unit()).collect();
            let pooled: Vec<f64> = (0..cfg.pooled_dim).map(|_| rng.unit()).collect();
            let si = k % PROBE_COUNTS.len();
            let t = tables.timesteps[si][k % tables.timesteps[si].len()];
            dit.forward(&DitInputs { latent: &latent, timestep: t, text: &text, n_txt: PROBE_TEXT_ROWS, pooled: &pooled }, &mut cal);
        }
        cal.unify(&["cond", "te_out", "pe_out"]);
        let spec = DitStageSpec {
            n_txt: PROBE_TEXT_ROWS,
            q_lat: 20,
            counts: PROBE_COUNTS.to_vec(),
            shift,
            n_train,
            text_unit: 1.0 / 4096.0,
            pooled_unit: 1.0 / 4096.0,
        };
        let stage = lower_dit_stage(&dit, &cal, &spec)?;
        Ok(Probe { stages: vec![admit("denoise", &stage.program)], unread: dit.untouched() })
    })
}

/// `AutoencoderKL` (`vae/config.json` and its checkpoint) lowered as the decoder's chain of stages and admitted. `side` is the
/// latent's side (the denoiser's `sample_size`); `None` takes 8.
pub fn probe_vae_decoder(config: &Value, src: &dyn TensorSource, side: Option<usize>) -> Result<Probe, String> {
    let cfg = VaeConfig::from_json(config)?;
    let side = side.unwrap_or(8);
    guarded("the checkpoint does not fit the VAE decoder's reading", || {
        let vae = Vae::load(cfg.clone(), src)?;
        let mut cal = Calib::new();
        let mut rng = Lcg(0xae);
        for _ in 0..2 {
            let z: Vec<f64> = (0..cfg.latent_channels * side * side).map(|_| rng.unit() * 1.5).collect();
            vae.decode(&Fm { c: cfg.latent_channels, h: side, w: side, d: z }, &mut cal);
        }
        let chain = lower_vae(&vae, &cal, side, 1.0 / (1u64 << 20) as f64)?;
        let stages = chain.stages.iter().map(|s| admit(&s.name, &s.program)).collect();
        // The decoder reads `decoder.*` and `post_quant_conv.*`: the encoder is unread by design.
        let unread = vae.untouched().into_iter().filter(|n| !(n.starts_with("encoder.") || n.starts_with("quant_conv."))).collect();
        Ok(Probe { stages, unread })
    })
}
