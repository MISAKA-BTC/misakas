//! **The float reference of the VAE decoder** (`AutoencoderKL.decode`, diffusers 0.40), `f64` over the checkpoint's own
//! tensors by their diffusers names, with every site noted for calibration ([`super::calib::Calib`]) under the name the
//! lowering ([`super::vae`]) reads.
//!
//! The decoder is `post_quant_conv` (when the config has one), `conv_in`, the mid block (`ResnetBlock2D`,
//! single-head `Attention` over the pixels, `ResnetBlock2D`), the up blocks (`layers_per_block + 1` resnets each,
//! nearest-neighbour ×2 then a 3×3 convolution between all but the last), `conv_norm_out` (GroupNorm), SiLU,
//! `conv_out`. The input is the pipeline's latent `z`, which the decode first maps `z / scaling_factor +
//! shift_factor`.

use std::collections::BTreeMap;

use super::calib::Calib;
use super::tables::silu;
use crate::weights::TensorSource;

pub const VAE_EPS: f64 = 1e-6;

/// `AutoencoderKL`'s config (`vae/config.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct VaeConfig {
    pub latent_channels: usize,
    pub out_channels: usize,
    pub block_out_channels: Vec<usize>,
    pub layers_per_block: usize,
    pub groups: usize,
    pub scaling_factor: f64,
    pub shift_factor: f64,
    pub post_quant_conv: bool,
    pub mid_attention: bool,
}

impl VaeConfig {
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let u = |k: &str, d: Option<usize>| {
            v.get(k).and_then(|x| x.as_u64()).map(|x| x as usize).or(d).ok_or_else(|| format!("vae config.json has no `{k}`"))
        };
        let f = |k: &str, d: f64| v.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
        let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d);
        let block_out_channels: Vec<usize> = v
            .get("block_out_channels")
            .and_then(|x| x.as_array())
            .ok_or("vae config.json has no `block_out_channels`")?
            .iter()
            .filter_map(|x| x.as_u64().map(|x| x as usize))
            .collect();
        if let Some(a) = v.get("act_fn").and_then(|x| x.as_str())
            && a != "silu"
        {
            return Err(format!("the VAE activation {a:?} is not SiLU"));
        }
        Ok(Self {
            latent_channels: u("latent_channels", Some(4))?,
            out_channels: u("out_channels", Some(3))?,
            block_out_channels,
            layers_per_block: u("layers_per_block", Some(1))?,
            groups: u("norm_num_groups", Some(32))?,
            scaling_factor: f("scaling_factor", 0.18215),
            shift_factor: f("shift_factor", 0.0),
            post_quant_conv: b("use_post_quant_conv", true),
            mid_attention: b("mid_block_add_attention", true),
        })
    }

    /// The decoder's up blocks: `(in_channels, out_channels, upsample)` of each, first to last.
    pub fn up_blocks(&self) -> Vec<(usize, usize, bool)> {
        let rev: Vec<usize> = self.block_out_channels.iter().rev().cloned().collect();
        let mut prev = rev[0];
        let mut out = Vec::new();
        for (i, c) in rev.iter().enumerate() {
            out.push((prev, *c, i + 1 < rev.len()));
            prev = *c;
        }
        out
    }

    /// The factor the decoder scales the latent's side by.
    pub fn upscale(&self) -> usize {
        1 << (self.block_out_channels.len() - 1)
    }
}

/// A `[C, H, W]` feature map.
#[derive(Clone, Debug)]
pub struct Fm {
    pub c: usize,
    pub h: usize,
    pub w: usize,
    pub d: Vec<f64>,
}

pub struct Vae {
    pub cfg: VaeConfig,
    w: BTreeMap<String, (Vec<usize>, Vec<f64>)>,
}

impl Vae {
    pub fn load(cfg: VaeConfig, src: &dyn TensorSource) -> Result<Self, String> {
        let mut w = BTreeMap::new();
        for n in src.names() {
            let t = src.load(&n).map_err(|e| e.to_string())?;
            w.insert(n, (t.shape, t.data.iter().map(|v| *v as f64).collect()));
        }
        Ok(Self { cfg, w })
    }

    pub fn has(&self, name: &str) -> bool {
        self.w.contains_key(name)
    }

    pub fn t(&self, name: &str) -> &(Vec<usize>, Vec<f64>) {
        self.w.get(name).unwrap_or_else(|| panic!("the VAE checkpoint has no tensor {name:?}"))
    }

    pub fn f32s(&self, name: &str) -> Vec<f32> {
        self.t(name).1.iter().map(|v| *v as f32).collect()
    }

    pub fn conv(&self, x: &Fm, prefix: &str, pad: usize) -> Fm {
        let (ws, wd) = self.t(&format!("{prefix}.weight"));
        let b = &self.t(&format!("{prefix}.bias")).1;
        conv2d(x, wd, ws[0], ws[2], pad, b)
    }

    pub fn gn(&self, x: &Fm, prefix: &str) -> Fm {
        group_norm(x, self.cfg.groups, &self.t(&format!("{prefix}.weight")).1, &self.t(&format!("{prefix}.bias")).1, VAE_EPS)
    }

    /// A `ResnetBlock2D`: `norm1, SiLU, conv1, norm2, SiLU, conv2`, the shortcut (a 1×1 convolution when the channels
    /// change) added. Notes `{s}.n1/.a1/.c1/.n2/.a2/.c2/.o`.
    pub fn resnet(&self, x: &Fm, prefix: &str, s: &str, cal: &mut Calib) -> Fm {
        let n1 = self.gn(x, &format!("{prefix}.norm1"));
        cal.note(&format!("{s}.n1"), &n1.d);
        let a1 = map(&n1, silu);
        cal.note(&format!("{s}.a1"), &a1.d);
        let c1 = self.conv(&a1, &format!("{prefix}.conv1"), 1);
        cal.note(&format!("{s}.c1"), &c1.d);
        let n2 = self.gn(&c1, &format!("{prefix}.norm2"));
        cal.note(&format!("{s}.n2"), &n2.d);
        let a2 = map(&n2, silu);
        cal.note(&format!("{s}.a2"), &a2.d);
        let c2 = self.conv(&a2, &format!("{prefix}.conv2"), 1);
        cal.note(&format!("{s}.c2"), &c2.d);
        let sc = if self.has(&format!("{prefix}.conv_shortcut.weight")) {
            let r = self.conv(x, &format!("{prefix}.conv_shortcut"), 0);
            cal.note(&format!("{s}.sc"), &r.d);
            r
        } else {
            x.clone()
        };
        let o = Fm { c: c2.c, h: c2.h, w: c2.w, d: sc.d.iter().zip(&c2.d).map(|(a, b)| a + b).collect() };
        cal.note(&format!("{s}.o"), &o.d);
        o
    }

    /// The mid block's single-head attention over the pixels with its residual. Notes `{s}.n/.q/.k/.v/.ctx/.p/.o`.
    pub fn attention(&self, x: &Fm, prefix: &str, s: &str, cal: &mut Calib) -> Fm {
        let n = self.gn(x, &format!("{prefix}.group_norm"));
        cal.note(&format!("{s}.n"), &n.d);
        let (c, t) = (x.c, x.h * x.w);
        // [C, HW] -> [HW, C].
        let rows: Vec<f64> = (0..t).flat_map(|p| (0..c).map(move |ch| (p, ch))).map(|(p, ch)| n.d[ch * t + p]).collect();
        let lin = |inp: &[f64], name: &str| -> Vec<f64> {
            let (ws, wd) = self.t(&format!("{prefix}.{name}.weight"));
            let b = &self.t(&format!("{prefix}.{name}.bias")).1;
            (0..t)
                .flat_map(|p| (0..ws[0]).map(move |o| (p, o)))
                .map(|(p, o)| b[o] + (0..ws[1]).map(|i| inp[p * ws[1] + i] * wd[o * ws[1] + i]).sum::<f64>())
                .collect()
        };
        let (q, k, v) = (lin(&rows, "to_q"), lin(&rows, "to_k"), lin(&rows, "to_v"));
        cal.note(&format!("{s}.q"), &q);
        cal.note(&format!("{s}.k"), &k);
        cal.note(&format!("{s}.v"), &v);
        let mut ctx = vec![0f64; t * c];
        for a in 0..t {
            let sc: Vec<f64> = (0..t).map(|j| (0..c).map(|e| q[a * c + e] * k[j * c + e]).sum::<f64>() / (c as f64).sqrt()).collect();
            let mx = sc.iter().cloned().fold(f64::MIN, f64::max);
            let ex: Vec<f64> = sc.iter().map(|z| (z - mx).exp()).collect();
            let z: f64 = ex.iter().sum();
            for e in 0..c {
                ctx[a * c + e] = (0..t).map(|j| ex[j] / z * v[j * c + e]).sum();
            }
        }
        cal.note(&format!("{s}.ctx"), &ctx);
        let p = lin(&ctx, "to_out.0");
        cal.note(&format!("{s}.p"), &p);
        let mut o = x.clone();
        for pix in 0..t {
            for ch in 0..c {
                o.d[ch * t + pix] += p[pix * c + ch];
            }
        }
        cal.note(&format!("{s}.o"), &o.d);
        o
    }

    /// The whole decode: the latent `z` (`[C, H, W]`, the pipeline's, before `/scaling + shift`) to the decoded image
    /// `[out, H·up, W·up]` in about `[-1, 1]`.
    pub fn decode(&self, z: &Fm, cal: &mut Calib) -> Fm {
        let cfg = &self.cfg;
        let mut x = Fm { c: z.c, h: z.h, w: z.w, d: z.d.iter().map(|v| v / cfg.scaling_factor + cfg.shift_factor).collect() };
        cal.note("vae.z", &x.d);
        if cfg.post_quant_conv {
            x = self.conv(&x, "post_quant_conv", 0);
            cal.note("vae.pq", &x.d);
        }
        x = self.conv(&x, "decoder.conv_in", 1);
        cal.note("vae.ci", &x.d);
        x = self.resnet(&x, "decoder.mid_block.resnets.0", "vae.mid.r0", cal);
        if cfg.mid_attention {
            x = self.attention(&x, "decoder.mid_block.attentions.0", "vae.mid.at", cal);
        }
        x = self.resnet(&x, "decoder.mid_block.resnets.1", "vae.mid.r1", cal);
        for (i, (_, _, up)) in cfg.up_blocks().iter().enumerate() {
            for j in 0..cfg.layers_per_block + 1 {
                x = self.resnet(&x, &format!("decoder.up_blocks.{i}.resnets.{j}"), &format!("vae.up{i}.r{j}"), cal);
            }
            if *up {
                x = upsample_nearest(&x);
                cal.note(&format!("vae.up{i}.us"), &x.d);
                x = self.conv(&x, &format!("decoder.up_blocks.{i}.upsamplers.0.conv"), 1);
                cal.note(&format!("vae.up{i}.uc"), &x.d);
            }
        }
        let n = self.gn(&x, "decoder.conv_norm_out");
        cal.note("vae.no", &n.d);
        let a = map(&n, silu);
        cal.note("vae.ao", &a.d);
        let img = self.conv(&a, "decoder.conv_out", 1);
        cal.note("vae.img", &img.d);
        img
    }
}

pub fn map(x: &Fm, f: impl Fn(f64) -> f64) -> Fm {
    Fm { c: x.c, h: x.h, w: x.w, d: x.d.iter().map(|v| f(*v)).collect() }
}

/// `Conv2d` with stride 1, a square kernel `k`, symmetric zero padding; `w` is `[cout, cin, k, k]`.
pub fn conv2d(x: &Fm, w: &[f64], cout: usize, k: usize, pad: usize, bias: &[f64]) -> Fm {
    let (cin, h, wd) = (x.c, x.h, x.w);
    let (ho, wo) = (h + 2 * pad - k + 1, wd + 2 * pad - k + 1);
    let mut d = vec![0f64; cout * ho * wo];
    for o in 0..cout {
        for oh in 0..ho {
            for ow in 0..wo {
                let mut acc = bias[o];
                for c in 0..cin {
                    for kh in 0..k {
                        for kw in 0..k {
                            let (ih, iw) = ((oh + kh) as isize - pad as isize, (ow + kw) as isize - pad as isize);
                            if ih >= 0 && iw >= 0 && (ih as usize) < h && (iw as usize) < wd {
                                acc += x.d[(c * h + ih as usize) * wd + iw as usize] * w[((o * cin + c) * k + kh) * k + kw];
                            }
                        }
                    }
                }
                d[(o * ho + oh) * wo + ow] = acc;
            }
        }
    }
    Fm { c: cout, h: ho, w: wo, d }
}

/// `GroupNorm(groups, C, eps)` with affine `gamma`, `beta`: statistics over each group's channels and all pixels.
pub fn group_norm(x: &Fm, groups: usize, gamma: &[f64], beta: &[f64], eps: f64) -> Fm {
    let (c, hw) = (x.c, x.h * x.w);
    let cg = c / groups;
    let mut d = vec![0f64; x.d.len()];
    for g in 0..groups {
        let span = &x.d[g * cg * hw..(g + 1) * cg * hw];
        let n = span.len() as f64;
        let mean = span.iter().sum::<f64>() / n;
        let var = span.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
        let inv = 1.0 / (var + eps).sqrt();
        for ch in g * cg..(g + 1) * cg {
            for p in 0..hw {
                d[ch * hw + p] = (x.d[ch * hw + p] - mean) * inv * gamma[ch] + beta[ch];
            }
        }
    }
    Fm { c, h: x.h, w: x.w, d }
}

/// Nearest-neighbour ×2.
pub fn upsample_nearest(x: &Fm) -> Fm {
    let (h2, w2) = (x.h * 2, x.w * 2);
    let mut d = vec![0f64; x.c * h2 * w2];
    for c in 0..x.c {
        for h in 0..h2 {
            for w in 0..w2 {
                d[(c * h2 + h) * w2 + w] = x.d[(c * x.h + h / 2) * x.w + w / 2];
            }
        }
    }
    Fm { c: x.c, h: h2, w: w2, d }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::super::testkit::Lcg;
    use super::*;
    use crate::weights::{MapSource, Tensor};

    /// The fixture's reduced VAE: `block_out_channels = (8, 16)`, one layer per block, 4 latent channels, 4 groups.
    pub fn tiny_config() -> VaeConfig {
        VaeConfig {
            latent_channels: 4,
            out_channels: 3,
            block_out_channels: vec![8, 16],
            layers_per_block: 1,
            groups: 4,
            scaling_factor: 1.5305,
            shift_factor: 0.0609,
            post_quant_conv: false,
            mid_attention: true,
        }
    }

    pub fn random_source(cfg: &VaeConfig, seed: u64) -> MapSource {
        let mut rng = Lcg(seed);
        let mut m = MapSource::default();
        let mut put = |name: String, shape: &[usize], scale: f64, around: f64| {
            let n: usize = shape.iter().product();
            m.0.insert(name, Tensor::new(shape.to_vec(), (0..n).map(|_| (around + rng.unit() * scale) as f32).collect()));
        };
        let conv = |put: &mut dyn FnMut(String, &[usize], f64, f64), name: &str, cout: usize, cin: usize, k: usize| {
            put(format!("{name}.weight"), &[cout, cin, k, k], 0.6 / ((cin * k * k) as f64).sqrt(), 0.0);
            put(format!("{name}.bias"), &[cout], 0.05, 0.0);
        };
        let gn = |put: &mut dyn FnMut(String, &[usize], f64, f64), name: &str, c: usize| {
            put(format!("{name}.weight"), &[c], 0.1, 1.0);
            put(format!("{name}.bias"), &[c], 0.05, 0.0);
        };
        let resnet = |put: &mut dyn FnMut(String, &[usize], f64, f64), name: &str, cin: usize, cout: usize| {
            gn(put, &format!("{name}.norm1"), cin);
            conv(put, &format!("{name}.conv1"), cout, cin, 3);
            gn(put, &format!("{name}.norm2"), cout);
            conv(put, &format!("{name}.conv2"), cout, cout, 3);
            if cin != cout {
                conv(put, &format!("{name}.conv_shortcut"), cout, cin, 1);
            }
        };
        let top = *cfg.block_out_channels.last().unwrap();
        if cfg.post_quant_conv {
            conv(&mut put, "post_quant_conv", cfg.latent_channels, cfg.latent_channels, 1);
        }
        conv(&mut put, "decoder.conv_in", top, cfg.latent_channels, 3);
        resnet(&mut put, "decoder.mid_block.resnets.0", top, top);
        if cfg.mid_attention {
            gn(&mut put, "decoder.mid_block.attentions.0.group_norm", top);
            for n in ["to_q", "to_k", "to_v", "to_out.0"] {
                put(format!("decoder.mid_block.attentions.0.{n}.weight"), &[top, top], 0.6 / (top as f64).sqrt(), 0.0);
                put(format!("decoder.mid_block.attentions.0.{n}.bias"), &[top], 0.05, 0.0);
            }
        }
        resnet(&mut put, "decoder.mid_block.resnets.1", top, top);
        for (i, (cin, cout, up)) in cfg.up_blocks().into_iter().enumerate() {
            for j in 0..cfg.layers_per_block + 1 {
                resnet(&mut put, &format!("decoder.up_blocks.{i}.resnets.{j}"), if j == 0 { cin } else { cout }, cout);
            }
            if up {
                conv(&mut put, &format!("decoder.up_blocks.{i}.upsamplers.0.conv"), cout, cout, 3);
            }
        }
        gn(&mut put, "decoder.conv_norm_out", cfg.block_out_channels[0]);
        conv(&mut put, "decoder.conv_out", cfg.out_channels, cfg.block_out_channels[0], 3);
        m
    }

    pub fn latent(cfg: &VaeConfig, side: usize, seed: u64) -> Fm {
        let mut rng = Lcg(seed);
        Fm { c: cfg.latent_channels, h: side, w: side, d: (0..cfg.latent_channels * side * side).map(|_| rng.unit() * 2.0).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn the_config_reads_diffusers_keys_and_orders_the_up_blocks() {
        let v = serde_json::json!({
            "latent_channels": 4, "out_channels": 3, "block_out_channels": [8, 16], "layers_per_block": 1,
            "norm_num_groups": 4, "scaling_factor": 1.5305, "shift_factor": 0.0609,
            "use_post_quant_conv": false, "act_fn": "silu"
        });
        assert_eq!(VaeConfig::from_json(&v).unwrap(), tiny_config());
        let cfg = tiny_config();
        // The decoder runs the widest channels first and upsamples after every block but the last.
        assert_eq!(cfg.up_blocks(), vec![(16, 16, true), (16, 8, false)]);
        assert_eq!(cfg.upscale(), 2);
        let mut bad = v;
        bad["act_fn"] = serde_json::json!("gelu");
        assert!(VaeConfig::from_json(&bad).is_err());
    }

    #[test]
    fn the_float_decoder_has_the_right_shapes_and_notes_its_sites() {
        let cfg = tiny_config();
        let vae = Vae::load(cfg.clone(), &random_source(&cfg, 3)).unwrap();
        let mut cal = Calib::new();
        let img = vae.decode(&latent(&cfg, 8, 4), &mut cal);
        assert_eq!((img.c, img.h, img.w), (3, 16, 16));
        assert!(img.d.iter().all(|v| v.is_finite()) && img.d.iter().any(|v| v.abs() > 1e-3));
        for s in [
            "vae.z",
            "vae.ci",
            "vae.mid.r0.o",
            "vae.mid.at.ctx",
            "vae.mid.r1.o",
            "vae.up0.r0.o",
            "vae.up0.r1.o",
            "vae.up0.us",
            "vae.up0.uc",
            "vae.up1.r0.sc",
            "vae.up1.r1.o",
            "vae.no",
            "vae.ao",
            "vae.img",
        ] {
            assert!(cal.amax.contains_key(s), "site {s}");
        }
    }

    #[test]
    fn group_norm_and_the_nearest_upsample_are_the_textbook_ones() {
        let x = Fm { c: 4, h: 2, w: 2, d: (0..16).map(|i| (i as f64 * 0.9).sin() * 2.0 + 0.3).collect() };
        let y = group_norm(&x, 2, &[1.0; 4], &[0.0; 4], 1e-6);
        for g in 0..2 {
            let span = &y.d[g * 8..(g + 1) * 8];
            let mean = span.iter().sum::<f64>() / 8.0;
            let var = span.iter().map(|v| v * v).sum::<f64>() / 8.0;
            assert!(mean.abs() < 1e-9 && (var - 1.0).abs() < 1e-4);
        }
        let u = upsample_nearest(&Fm { c: 1, h: 2, w: 2, d: vec![1.0, 2.0, 3.0, 4.0] });
        assert_eq!(u.d, vec![1.0, 1.0, 2.0, 2.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 3.0, 3.0, 4.0, 4.0]);
    }
}
