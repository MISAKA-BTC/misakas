//! **The float reference of the MMDiT denoiser** (`SD3Transformer2DModel`, diffusers 0.40), in `f64` over the
//! checkpoint's own tensors by their diffusers names — the reference every lowerer is held to, and the run that
//! calibrates the integer program ([`super::calib::Calib`] notes each site as the lowering will read it).
//!
//! It is the model's arithmetic and nothing else: LayerNorm without affine (`ε = 1e-6`), `AdaLayerNormZero` /
//! `AdaLayerNormContinuous` (shift, scale, gate chunk orders as diffusers', scale FIRST in the continuous one),
//! joint attention over `[image tokens ‖ text tokens]` with each stream's own projections and no q/k norm,
//! GELU-tanh feed-forward, the last block `context_pre_only`. Its velocity is compared with diffusers' own in the
//! fixture test (`tests/diffusers_sd3.rs`, a generated reference) before anything is lowered against it.

use std::collections::BTreeMap;

use super::calib::Calib;
use super::embed::cropped_pos_embed;
use super::sampler::unpatchify_index;
use super::tables::{gelu_tanh, silu, timestep_embedding};
use crate::weights::TensorSource;

/// The configuration of `SD3Transformer2DModel` the lowering reads (`transformer/config.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sd3Config {
    pub in_channels: usize,
    pub out_channels: usize,
    pub patch_size: usize,
    /// The latent's side (`sample_size`, `H = W`).
    pub sample_size: usize,
    pub num_layers: usize,
    pub heads: usize,
    pub head_dim: usize,
    pub joint_dim: usize,
    pub pooled_dim: usize,
    pub caption_dim: usize,
    pub pos_max: usize,
}

impl Sd3Config {
    /// `inner_dim = heads · head_dim`, the width of the token streams.
    pub fn width(&self) -> usize {
        self.heads * self.head_dim
    }

    /// The token grid's side, `sample_size / patch_size`.
    pub fn grid(&self) -> usize {
        self.sample_size / self.patch_size
    }

    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let u =
            |k: &str| v.get(k).and_then(|x| x.as_u64()).map(|x| x as usize).ok_or_else(|| format!("config.json has no integer `{k}`"));
        if v.get("qk_norm").is_some_and(|q| !q.is_null()) {
            return Err(format!("qk_norm {} is a later profile (SD3.5); the first image profile has none", v["qk_norm"]));
        }
        if v.get("dual_attention_layers").and_then(|d| d.as_array()).is_some_and(|d| !d.is_empty()) {
            return Err("dual attention layers (SD3.5) are a later profile".to_string());
        }
        let in_channels = u("in_channels")?;
        let cfg = Self {
            in_channels,
            out_channels: v.get("out_channels").and_then(|x| x.as_u64()).map(|x| x as usize).unwrap_or(in_channels),
            patch_size: u("patch_size")?,
            sample_size: u("sample_size")?,
            num_layers: u("num_layers")?,
            heads: u("num_attention_heads")?,
            head_dim: u("attention_head_dim")?,
            joint_dim: u("joint_attention_dim")?,
            pooled_dim: u("pooled_projection_dim")?,
            caption_dim: u("caption_projection_dim")?,
            pos_max: u("pos_embed_max_size")?,
        };
        if cfg.caption_dim != cfg.width() {
            return Err(format!("caption_projection_dim {} is not the inner dimension {}", cfg.caption_dim, cfg.width()));
        }
        if cfg.sample_size % cfg.patch_size != 0 || cfg.grid() > cfg.pos_max {
            return Err("the latent grid does not fit the position table".to_string());
        }
        Ok(cfg)
    }
}

/// The denoiser's tensors, widened to `f64`, by name.
pub struct Dit {
    pub cfg: Sd3Config,
    w: BTreeMap<String, (Vec<usize>, Vec<f64>)>,
    /// The tensors the float run and the lowering have read (a checkpoint tensor nothing read is a feature this reading lacks).
    touched: std::sync::Mutex<std::collections::BTreeSet<String>>,
}

/// One denoiser call's inputs.
pub struct DitInputs<'a> {
    /// `[C, H, W]`.
    pub latent: &'a [f64],
    /// The scheduler's timestep for this step (`σ · 1000`).
    pub timestep: f64,
    /// `[L, joint_dim]` — the text stage's rows.
    pub text: &'a [f64],
    pub n_txt: usize,
    /// `[pooled_dim]`.
    pub pooled: &'a [f64],
}

impl Dit {
    pub fn load(cfg: Sd3Config, src: &dyn TensorSource) -> Result<Self, String> {
        let mut w = BTreeMap::new();
        for n in src.names() {
            let t = src.load(&n).map_err(|e| e.to_string())?;
            w.insert(n, (t.shape, t.data.iter().map(|v| *v as f64).collect()));
        }
        Ok(Self { cfg, w, touched: Default::default() })
    }

    pub fn has(&self, name: &str) -> bool {
        self.w.contains_key(name)
    }

    pub fn t(&self, name: &str) -> &(Vec<usize>, Vec<f64>) {
        let t = self.w.get(name).unwrap_or_else(|| panic!("the checkpoint has no tensor {name:?}"));
        if let Ok(mut s) = self.touched.lock() {
            s.insert(name.to_string());
        }
        t
    }

    /// The checkpoint's tensors nothing has read (after a float run and a lowering: the ones this reading does not account for).
    pub fn untouched(&self) -> Vec<String> {
        let s = self.touched.lock().map(|g| g.clone()).unwrap_or_default();
        self.w.keys().filter(|k| !s.contains(*k)).cloned().collect()
    }

    /// The `f32` copy of a tensor (the quantisers take `f32` rows).
    pub fn f32s(&self, name: &str) -> Vec<f32> {
        self.t(name).1.iter().map(|v| *v as f32).collect()
    }

    /// `y = x·Wᵀ + b` for `x:[rows, in]` and `W:[out, in]` named `{prefix}.weight` / `.bias`.
    fn lin(&self, x: &[f64], rows: usize, prefix: &str) -> Vec<f64> {
        let (ws, wd) = self.t(&format!("{prefix}.weight"));
        let (out, inn) = (ws[0], ws[1]);
        assert_eq!(x.len(), rows * inn, "{prefix}: input is [{rows}, {inn}]");
        let bias = if self.has(&format!("{prefix}.bias")) { Some(&self.t(&format!("{prefix}.bias")).1) } else { None };
        let mut y = vec![0f64; rows * out];
        for r in 0..rows {
            for o in 0..out {
                let mut acc = bias.map_or(0.0, |b| b[o]);
                let wr = &wd[o * inn..(o + 1) * inn];
                let xr = &x[r * inn..(r + 1) * inn];
                for i in 0..inn {
                    acc += xr[i] * wr[i];
                }
                y[r * out + o] = acc;
            }
        }
        y
    }

    /// The forward pass: the velocity `[C, H, W]`; every site noted into `cal`.
    pub fn forward(&self, inp: &DitInputs<'_>, cal: &mut Calib) -> Vec<f64> {
        let cfg = &self.cfg;
        let (d, g, p, c) = (cfg.width(), cfg.grid(), cfg.patch_size, cfg.in_channels);
        let n = g * g;
        let l = inp.n_txt;
        let side = cfg.sample_size;

        // ---- the image tokens: strided conv + the cropped position table ----
        cal.note("lat_in", inp.latent);
        let (_, cw) = self.t("pos_embed.proj.weight");
        let cb = &self.t("pos_embed.proj.bias").1;
        let pos32: Vec<f32> = self.t("pos_embed.pos_embed").1.iter().map(|v| *v as f32).collect();
        let pos = cropped_pos_embed(&pos32, cfg.pos_max, d, g, g);
        let mut h_img = vec![0f64; n * d];
        for t in 0..n {
            let (oh, ow) = (t / g, t % g);
            for o in 0..d {
                let mut acc = cb[o] + pos[t * d + o] as f64;
                for ch in 0..c {
                    for kh in 0..p {
                        for kw in 0..p {
                            acc += inp.latent[(ch * side + oh * p + kh) * side + ow * p + kw] * cw[((o * c + ch) * p + kh) * p + kw];
                        }
                    }
                }
                h_img[t * d + o] = acc;
            }
        }
        cal.note("stream_img", &h_img);

        // ---- the conditioning vector ----
        let sin = timestep_embedding(inp.timestep, 256, true, 0.0, 10_000.0);
        let te_h = self.lin(&sin, 1, "time_text_embed.timestep_embedder.linear_1");
        cal.note("te_h", &te_h);
        let te_a: Vec<f64> = te_h.iter().map(|v| silu(*v)).collect();
        cal.note("te_a", &te_a);
        let te = self.lin(&te_a, 1, "time_text_embed.timestep_embedder.linear_2");
        cal.note("te_out", &te);
        cal.note("pe_in", inp.pooled);
        let pe_h = self.lin(inp.pooled, 1, "time_text_embed.text_embedder.linear_1");
        cal.note("pe_h", &pe_h);
        let pe_a: Vec<f64> = pe_h.iter().map(|v| silu(*v)).collect();
        cal.note("pe_a", &pe_a);
        let pe = self.lin(&pe_a, 1, "time_text_embed.text_embedder.linear_2");
        cal.note("pe_out", &pe);
        let cond: Vec<f64> = te.iter().zip(&pe).map(|(a, b)| a + b).collect();
        cal.note("cond", &cond);
        let cond_a: Vec<f64> = cond.iter().map(|v| silu(*v)).collect();
        cal.note("cond_a", &cond_a);

        // ---- the text tokens ----
        cal.note("txt_in", inp.text);
        let mut h_txt = self.lin(inp.text, l, "context_embedder");
        cal.note("stream_txt", &h_txt);

        for i in 0..cfg.num_layers {
            let pre_only = i + 1 == cfg.num_layers;
            self.block(i, pre_only, &mut h_img, &mut h_txt, &cond_a, l, cal);
        }

        // ---- the output: AdaLayerNormContinuous, proj_out, unpatchify ----
        let m = self.lin(&cond_a, 1, "norm_out.linear");
        cal.note("no_mod", &m);
        cal.note("no_in", &h_img);
        let (scale, shift) = (&m[..d], &m[d..2 * d]);
        let x = modulate(&h_img, n, d, shift, scale);
        cal.note("no_x", &x);
        let rows = self.lin(&x, n, "proj_out");
        cal.note("vel", &rows);
        let idx = unpatchify_index(cfg.out_channels, g, g, p);
        idx.iter().map(|i| rows[*i as usize]).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn block(&self, i: usize, pre_only: bool, h_img: &mut Vec<f64>, h_txt: &mut Vec<f64>, cond_a: &[f64], l: usize, cal: &mut Calib) {
        let cfg = &self.cfg;
        let (d, n) = (cfg.width(), cfg.grid() * cfg.grid());
        let pf = format!("transformer_blocks.{i}");
        let b = format!("b{i}");
        cal.note(&format!("{b}.h_in_img"), h_img);
        cal.note(&format!("{b}.h_in_txt"), h_txt);

        // The modulations (AdaLayerNormZero: shift_msa, scale_msa, gate_msa, shift_mlp, scale_mlp, gate_mlp).
        let m_img = self.lin(cond_a, 1, &format!("{pf}.norm1.linear"));
        cal.note(&format!("{b}.mod_img"), &m_img);
        let m_txt = self.lin(cond_a, 1, &format!("{pf}.norm1_context.linear"));
        cal.note(&format!("{b}.mod_txt"), &m_txt);
        // The lowering splits each stream's modulation into the attention half's chunks and the MLP half's.
        cal.note(&format!("{b}.mod_img.a"), &m_img[..3 * d]);
        cal.note(&format!("{b}.mod_img.m"), &m_img[3 * d..]);
        if pre_only {
            cal.note(&format!("{b}.mod_txt.a"), &m_txt);
        } else {
            cal.note(&format!("{b}.mod_txt.a"), &m_txt[..3 * d]);
            cal.note(&format!("{b}.mod_txt.m"), &m_txt[3 * d..]);
        }
        let ch = |m: &[f64], k: usize| m[k * d..(k + 1) * d].to_vec();

        cal.note(&format!("{b}.ln1_in_img"), h_img);
        let x1_img = modulate(h_img, n, d, &ch(&m_img, 0), &ch(&m_img, 1));
        cal.note(&format!("{b}.x1_img"), &x1_img);
        cal.note(&format!("{b}.ln1_in_txt"), h_txt);
        let x1_txt = if pre_only {
            // AdaLayerNormContinuous: scale FIRST, then shift.
            modulate(h_txt, l, d, &ch(&m_txt, 1), &ch(&m_txt, 0))
        } else {
            modulate(h_txt, l, d, &ch(&m_txt, 0), &ch(&m_txt, 1))
        };
        cal.note(&format!("{b}.x1_txt"), &x1_txt);

        // Joint attention, image tokens first.
        let proj = |x: &[f64], rows: usize, name: &str| self.lin(x, rows, &format!("{pf}.attn.{name}"));
        let q = [proj(&x1_img, n, "to_q"), proj(&x1_txt, l, "add_q_proj")].concat();
        let k = [proj(&x1_img, n, "to_k"), proj(&x1_txt, l, "add_k_proj")].concat();
        let v = [proj(&x1_img, n, "to_v"), proj(&x1_txt, l, "add_v_proj")].concat();
        cal.note(&format!("{b}.q"), &q);
        cal.note(&format!("{b}.k"), &k);
        cal.note(&format!("{b}.v"), &v);
        let t = n + l;
        let (heads, dh) = (cfg.heads, cfg.head_dim);
        let mut ctx = vec![0f64; t * d];
        for hd in 0..heads {
            for a in 0..t {
                let sc: Vec<f64> = (0..t)
                    .map(|j| (0..dh).map(|e| q[a * d + hd * dh + e] * k[j * d + hd * dh + e]).sum::<f64>() / (dh as f64).sqrt())
                    .collect();
                let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
                let ex: Vec<f64> = sc.iter().map(|s| (s - mx).exp()).collect();
                let z: f64 = ex.iter().sum();
                for e in 0..dh {
                    ctx[a * d + hd * dh + e] = (0..t).map(|j| ex[j] / z * v[j * d + hd * dh + e]).sum();
                }
            }
        }
        cal.note(&format!("{b}.ctx"), &ctx);
        let ao = self.lin(&ctx[..n * d], n, &format!("{pf}.attn.to_out.0"));
        cal.note(&format!("{b}.ao"), &ao);
        let aot = (!pre_only).then(|| self.lin(&ctx[n * d..], l, &format!("{pf}.attn.to_add_out")));
        if let Some(a) = &aot {
            cal.note(&format!("{b}.aot"), a);
        }

        // Image stream: gated attention residual, then the modulated MLP.
        let gate = ch(&m_img, 2);
        for r in 0..n {
            for o in 0..d {
                h_img[r * d + o] += gate[o] * ao[r * d + o];
            }
        }
        cal.note("stream_img", h_img);
        cal.note(&format!("{b}.h_mid_img"), h_img);
        cal.note(&format!("{b}.ln2_in_img"), h_img);
        let x2 = modulate(h_img, n, d, &ch(&m_img, 3), &ch(&m_img, 4));
        cal.note(&format!("{b}.x2_img"), &x2);
        let ff = self.ff(&pf, "ff", &x2, n, &format!("{b}.img"), cal);
        let gate = ch(&m_img, 5);
        for r in 0..n {
            for o in 0..d {
                h_img[r * d + o] += gate[o] * ff[r * d + o];
            }
        }
        cal.note("stream_img", h_img);
        cal.note(&format!("{b}.h_out_img"), h_img);

        // Text stream (the last block drops it).
        if let Some(aot) = aot {
            let gate = ch(&m_txt, 2);
            for r in 0..l {
                for o in 0..d {
                    h_txt[r * d + o] += gate[o] * aot[r * d + o];
                }
            }
            cal.note("stream_txt", h_txt);
            cal.note(&format!("{b}.h_mid_txt"), h_txt);
            cal.note(&format!("{b}.ln2_in_txt"), h_txt);
            let x2 = modulate(h_txt, l, d, &ch(&m_txt, 3), &ch(&m_txt, 4));
            cal.note(&format!("{b}.x2_txt"), &x2);
            let ff = self.ff(&pf, "ff_context", &x2, l, &format!("{b}.txt"), cal);
            let gate = ch(&m_txt, 5);
            for r in 0..l {
                for o in 0..d {
                    h_txt[r * d + o] += gate[o] * ff[r * d + o];
                }
            }
            cal.note("stream_txt", h_txt);
            cal.note(&format!("{b}.h_out_txt"), h_txt);
        }
    }

    /// `FeedForward(gelu-approximate)`: `net.0.proj` (to `4d`), GELU-tanh, `net.2`.
    fn ff(&self, pf: &str, which: &str, x: &[f64], rows: usize, site: &str, cal: &mut Calib) -> Vec<f64> {
        let h = self.lin(x, rows, &format!("{pf}.{which}.net.0.proj"));
        cal.note(&format!("{site}.ff1"), &h);
        let a: Vec<f64> = h.iter().map(|v| gelu_tanh(*v)).collect();
        cal.note(&format!("{site}.ffa"), &a);
        let o = self.lin(&a, rows, &format!("{pf}.{which}.net.2"));
        cal.note(&format!("{site}.ff2"), &o);
        o
    }
}

/// `LN(x) · (1 + scale) + shift` row by row (`ε = 1e-6`, no affine).
pub fn modulate(x: &[f64], rows: usize, d: usize, shift: &[f64], scale: &[f64]) -> Vec<f64> {
    let mut y = vec![0f64; rows * d];
    for r in 0..rows {
        let row = &x[r * d..(r + 1) * d];
        let mean = row.iter().sum::<f64>() / d as f64;
        let var = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / d as f64;
        let inv = 1.0 / (var + 1e-6).sqrt();
        for o in 0..d {
            y[r * d + o] = (row[o] - mean) * inv * (1.0 + scale[o]) + shift[o];
        }
    }
    y
}

#[cfg(test)]
pub(crate) mod testkit {
    //! A seeded random `SD3Transformer2DModel` (the fixture's reduced shape, any size) as an in-memory source — so the
    //! lowerers and the assembly are tested without a checkpoint on disk.

    use super::super::testkit::Lcg;
    use super::*;
    use crate::weights::{MapSource, Tensor};

    pub fn tiny_config() -> Sd3Config {
        Sd3Config {
            in_channels: 4,
            out_channels: 4,
            patch_size: 2,
            sample_size: 8,
            num_layers: 2,
            heads: 4,
            head_dim: 16,
            joint_dim: 32,
            pooled_dim: 32,
            caption_dim: 64,
            pos_max: 8,
        }
    }

    /// Weights of the diffusers names with `N(0, scale)`-ish entries (`unit() · scale`) and small biases.
    pub fn random_source(cfg: &Sd3Config, seed: u64) -> MapSource {
        let mut rng = Lcg(seed);
        let mut m = MapSource::default();
        let mut put = |name: String, shape: &[usize], scale: f64| {
            let n: usize = shape.iter().product();
            let data = (0..n).map(|_| (rng.unit() * scale) as f32).collect();
            m.0.insert(name, Tensor::new(shape.to_vec(), data));
        };
        let (d, p, c) = (cfg.width(), cfg.patch_size, cfg.in_channels);
        let lin = |put: &mut dyn FnMut(String, &[usize], f64), name: &str, out: usize, inn: usize, s: f64| {
            put(format!("{name}.weight"), &[out, inn], s);
            put(format!("{name}.bias"), &[out], 0.05);
        };
        put("pos_embed.proj.weight".into(), &[d, c, p, p], 0.3);
        put("pos_embed.proj.bias".into(), &[d], 0.05);
        put("pos_embed.pos_embed".into(), &[1, cfg.pos_max * cfg.pos_max, d], 0.2);
        lin(&mut put, "time_text_embed.timestep_embedder.linear_1", d, 256, 0.1);
        lin(&mut put, "time_text_embed.timestep_embedder.linear_2", d, d, 0.15);
        lin(&mut put, "time_text_embed.text_embedder.linear_1", d, cfg.pooled_dim, 0.2);
        lin(&mut put, "time_text_embed.text_embedder.linear_2", d, d, 0.15);
        lin(&mut put, "context_embedder", d, cfg.joint_dim, 0.2);
        for i in 0..cfg.num_layers {
            let pre_only = i + 1 == cfg.num_layers;
            let pf = format!("transformer_blocks.{i}");
            lin(&mut put, &format!("{pf}.norm1.linear"), 6 * d, d, 0.08);
            lin(&mut put, &format!("{pf}.norm1_context.linear"), if pre_only { 2 * d } else { 6 * d }, d, 0.08);
            for n in ["to_q", "to_k", "to_v", "add_q_proj", "add_k_proj", "add_v_proj", "to_out.0"] {
                lin(&mut put, &format!("{pf}.attn.{n}"), d, d, 0.18);
            }
            if !pre_only {
                lin(&mut put, &format!("{pf}.attn.to_add_out"), d, d, 0.18);
            }
            lin(&mut put, &format!("{pf}.ff.net.0.proj"), 4 * d, d, 0.15);
            lin(&mut put, &format!("{pf}.ff.net.2"), d, 4 * d, 0.1);
            if !pre_only {
                lin(&mut put, &format!("{pf}.ff_context.net.0.proj"), 4 * d, d, 0.15);
                lin(&mut put, &format!("{pf}.ff_context.net.2"), d, 4 * d, 0.1);
            }
        }
        lin(&mut put, "norm_out.linear", 2 * d, d, 0.08);
        lin(&mut put, "proj_out", p * p * cfg.out_channels, d, 0.15);
        m
    }

    /// A seeded calibration input: the latent, a timestep, the text rows and the pooled vector.
    pub fn inputs(cfg: &Sd3Config, seed: u64, n_txt: usize) -> (Vec<f64>, f64, Vec<f64>, Vec<f64>) {
        let mut rng = Lcg(seed);
        let latent = (0..cfg.in_channels * cfg.sample_size * cfg.sample_size).map(|_| rng.unit() * 2.0).collect();
        let text = (0..n_txt * cfg.joint_dim).map(|_| rng.unit()).collect();
        let pooled = (0..cfg.pooled_dim).map(|_| rng.unit()).collect();
        (latent, 400.0 + rng.unit() * 300.0, text, pooled)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn the_config_reads_the_diffusers_keys_and_refuses_later_profiles() {
        let v = serde_json::json!({
            "sample_size": 8, "patch_size": 2, "in_channels": 4, "num_layers": 2, "attention_head_dim": 16,
            "num_attention_heads": 4, "joint_attention_dim": 32, "caption_projection_dim": 64,
            "pooled_projection_dim": 32, "out_channels": 4, "pos_embed_max_size": 8, "qk_norm": null
        });
        assert_eq!(Sd3Config::from_json(&v).unwrap(), tiny_config());
        let mut bad = v.clone();
        bad["qk_norm"] = serde_json::json!("rms_norm");
        assert!(Sd3Config::from_json(&bad).unwrap_err().contains("later profile"));
        let mut bad = v.clone();
        bad["dual_attention_layers"] = serde_json::json!([0]);
        assert!(Sd3Config::from_json(&bad).is_err());
        let mut bad = v;
        bad["caption_projection_dim"] = serde_json::json!(32);
        assert!(Sd3Config::from_json(&bad).is_err());
    }

    #[test]
    fn the_float_denoiser_runs_and_notes_every_site_the_lowering_reads() {
        let cfg = tiny_config();
        let dit = Dit::load(cfg.clone(), &random_source(&cfg, 1)).unwrap();
        let (latent, t, text, pooled) = inputs(&cfg, 2, 5);
        let mut cal = Calib::new();
        let v = dit.forward(&DitInputs { latent: &latent, timestep: t, text: &text, n_txt: 5, pooled: &pooled }, &mut cal);
        assert_eq!(v.len(), latent.len());
        assert!(v.iter().all(|x| x.is_finite()) && v.iter().any(|x| x.abs() > 1e-6), "a live velocity");
        for s in ["lat_in", "stream_img", "stream_txt", "cond", "cond_a", "te_out", "pe_out", "no_mod", "no_x", "vel"] {
            assert!(cal.amax.contains_key(s), "site {s}");
        }
        for i in 0..cfg.num_layers {
            for s in [
                "mod_img",
                "mod_txt",
                "x1_img",
                "x1_txt",
                "q",
                "k",
                "v",
                "ctx",
                "ao",
                "ln2_in_img",
                "x2_img",
                "img.ff1",
                "img.ffa",
                "img.ff2",
            ] {
                assert!(cal.amax.contains_key(&format!("b{i}.{s}")), "b{i}.{s}");
            }
        }
        // The text stream's second half exists in block 0 only (block 1 is context_pre_only).
        assert!(cal.amax.contains_key("b0.aot") && cal.amax.contains_key("b0.txt.ff2"));
        assert!(!cal.amax.contains_key("b1.aot") && !cal.amax.contains_key("b1.txt.ff2"));
    }

    #[test]
    fn modulation_is_layer_norm_times_one_plus_scale_plus_shift() {
        let (rows, d) = (2usize, 6usize);
        let x: Vec<f64> = (0..rows * d).map(|i| (i as f64 * 0.7).sin() * 3.0 + 1.0).collect();
        let (shift, scale) = (vec![0.5; d], vec![-0.25; d]);
        let y = modulate(&x, rows, d, &shift, &scale);
        for r in 0..rows {
            // Un-modulate: (y − shift) / (1 + scale) is zero-mean unit-variance.
            let z: Vec<f64> = (0..d).map(|o| (y[r * d + o] - 0.5) / 0.75).collect();
            let mean = z.iter().sum::<f64>() / d as f64;
            let var = z.iter().map(|v| v * v).sum::<f64>() / d as f64;
            assert!(mean.abs() < 1e-9 && (var - 1.0).abs() < 1e-3);
        }
    }
}
