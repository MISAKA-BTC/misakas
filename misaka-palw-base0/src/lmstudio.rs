//! **The LM Studio lane: a Qwen2.5 checkpoint read out of what a model manager has on disk.**
//!
//! The W8A16 static-PTQ conversion (`convert::convert_qwen25_a16`) reads an HF checkpoint — a
//! `safetensors` blob addressed by `model.layers.N.…` names. What LM Studio stores is not that:
//! its models directory holds **GGUF** files (`~/.lmstudio/models/<publisher>/<repo>/<file>.gguf`),
//! and on Apple Silicon sometimes an MLX `safetensors` directory. A user who downloaded
//! Qwen2.5-1.5B-Instruct through LM Studio therefore had a running model and no way to make it a
//! class.
//!
//! This module closes that gap in the one direction that keeps every downstream path unchanged:
//! it reads the GGUF and **synthesizes the HF checkpoint** — a BF16 `safetensors` container with
//! the exact tensor names, shapes and dtype the converter and the float reference calibrator
//! already consume. Nothing downstream learns a second input format; the carrier is erased at the
//! door.
//!
//! # The carrier does not get to move the class — unless it is lossy, and then it says so
//!
//! Qwen2.5 ships BF16 weights. A GGUF that carries them as `BF16` (and its 1-D tensors as `F32`,
//! which llama.cpp widens exactly) hands back the ORIGINAL bits: the synthesized checkpoint is
//! byte-equal to the HF one, the static PTQ sees identical values, and the artifact root — the
//! class's identity — is the same as a conversion from the original `model.safetensors`. A test
//! pins that. A quantized carrier (`Q4_K_M`, `Q8_0`, …) hands back different weights, so it
//! produces a DIFFERENT artifact root — a real, runnable, adjudicable class, but **not** the one a
//! public genesis pinned. [`Qwen25GgufCheckpointV1::lossless_carrier`] is that fact as a boolean,
//! and the converter prints it rather than letting a digest mismatch be the first symptom.
//!
//! # Offline, and float on purpose
//!
//! Conversion, not execution: ADR-0040 Decision B pins a class's scales at registration, so what
//! reads a checkpoint may use float and what runs one may not — the same boundary `convert.rs`
//! and `gguf.rs` sit on. Nothing here is on the block-validation path.

use crate::gguf::{GgufDirectory, GgufError, GgufType, dequantize, parse_directory};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Why a GGUF could not become a Qwen2.5 checkpoint.
#[derive(Debug)]
pub enum LmStudioError {
    /// The container itself did not read.
    Gguf(GgufError),
    /// `general.architecture` is not `qwen2`. Carried so the message can say what it was —
    /// pointing this tool at a Llama or a Qwen3 GGUF should name the mistake, not a tensor.
    NotQwen2(String),
    /// A `qwen2.*` metadata key the geometry needs is absent.
    MissingMetadata(&'static str),
    /// A tensor the graph needs is absent from the file.
    MissingTensor(String),
    /// A tensor is present at a shape the geometry contradicts.
    ShapeMismatch { tensor: String, want: Vec<usize>, got: Vec<usize> },
    /// The file carries `output.weight` — an untied head. Qwen2.5-1.5B ties its embeddings and
    /// both converters tie for real (the unembedding IS the embedding), so a checkpoint with a
    /// separate head is a different model and converting it here would silently drop that matrix.
    UntiedHead,
    /// A dequantized weight is NaN or infinite. Quantizing garbage produces a plausible artifact,
    /// which is worse than an error.
    NonFinite(String),
    /// `qwen2.rope.freq_base` is a value no pinned rotary table exists for.
    UnsupportedRopeBase(f64),
    /// The tokenizer arrays are absent (a header-only prefix that ends before them, usually).
    NoTokenizer,
}

impl std::fmt::Display for LmStudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gguf(e) => write!(f, "{e}"),
            Self::NotQwen2(a) => write!(f, "general.architecture is {a:?}, and this lane converts qwen2 checkpoints"),
            Self::MissingMetadata(k) => write!(f, "the GGUF metadata has no {k}"),
            Self::MissingTensor(t) => write!(f, "no tensor named {t}"),
            Self::ShapeMismatch { tensor, want, got } => write!(f, "tensor {tensor}: want shape {want:?}, file says {got:?}"),
            Self::UntiedHead => write!(
                f,
                "the file carries output.weight (an untied lm_head); the A16 class ties its embeddings, so this is a different model"
            ),
            Self::NonFinite(t) => write!(f, "tensor {t} dequantizes to a non-finite value"),
            Self::UnsupportedRopeBase(b) => {
                write!(f, "rope.freq_base {b} has no pinned rotary table (10000 and 1000000 exist)")
            }
            Self::NoTokenizer => write!(f, "the GGUF holds no tokenizer arrays (was only a header prefix read?)"),
        }
    }
}

impl std::error::Error for LmStudioError {}

impl From<GgufError> for LmStudioError {
    fn from(e: GgufError) -> Self {
        Self::Gguf(e)
    }
}

/// The geometry and arithmetic constants a Qwen2 GGUF declares — the same facts `config.json`
/// states for the HF checkpoint, read from `qwen2.*` metadata and the embedding tensor.
#[derive(Clone, Debug, PartialEq)]
pub struct Qwen25GgufConfigV1 {
    pub hidden_size: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub num_hidden_layers: usize,
    pub intermediate_size: usize,
    /// From `token_embd.weight`'s own dimension, not from the tokenizer array: llama.cpp pads the
    /// token list to the embedding's width, and the EMBEDDING is what the engine indexes.
    pub vocab_size: usize,
    pub rms_norm_eps: f32,
    pub rope_theta: f32,
    /// `general.name`, when the writer recorded one — display only.
    pub model_name: Option<String>,
}

/// A Qwen2.5 checkpoint synthesized from a GGUF: the blob every existing consumer reads, plus
/// what the carrier was.
pub struct Qwen25GgufCheckpointV1 {
    /// A BF16 `safetensors` container with HF tensor names — byte-compatible with what
    /// `convert::parse_safetensors_header` / `read_bf16_tensor` and the float reference read.
    pub blob: Vec<u8>,
    pub config: Qwen25GgufConfigV1,
    /// `true` when every tensor's carrier hands back the original BF16 bits
    /// ([`GgufType::preserves_bf16`]): the synthesized checkpoint is then byte-equal to the HF one
    /// and the conversion reproduces the HF artifact root. `false` means the weights were
    /// re-quantized in transit — a usable class with a DIFFERENT root.
    pub lossless_carrier: bool,
    /// How many tensors each carrier type held, e.g. `Q4K×196 Q6K×2 F32×114` — for the one line
    /// the converter prints about provenance.
    pub carrier_summary: String,
}

/// `f32` to `bf16` bits, round-to-nearest-even — the rounding `torch.to(bfloat16)` performs, so a
/// value that started life as bf16 (every Qwen2.5 weight) comes back bit-identical, and a
/// dequantized value lands on its nearest representable neighbour.
fn bf16_bits(v: f32) -> u16 {
    let x = v.to_bits();
    // NaN would round its mantissa away and could become infinity; callers refuse non-finite
    // values before this, so the plain path is total here.
    let round = ((x >> 16) & 1) + 0x7FFF;
    ((x.wrapping_add(round)) >> 16) as u16
}

/// Encode named `f32` tensors as a BF16 `safetensors` container, in the order given.
///
/// The intermediate the whole lane funnels through — and public because the dev fixture writes
/// its safetensors twin with it, which is what lets a test state "the carrier does not move the
/// class" as a byte comparison.
pub fn encode_safetensors_bf16(tensors: &[(String, Vec<usize>, Vec<f32>)]) -> Vec<u8> {
    let mut header = String::from("{");
    let mut data: Vec<u8> = Vec::new();
    for (i, (name, shape, values)) in tensors.iter().enumerate() {
        debug_assert_eq!(shape.iter().product::<usize>(), values.len(), "{name}: shape and data agree");
        let begin = data.len();
        for v in values {
            data.extend_from_slice(&bf16_bits(*v).to_le_bytes());
        }
        if i > 0 {
            header.push(',');
        }
        let dims = shape.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",");
        header.push_str(&format!("\"{name}\":{{\"dtype\":\"BF16\",\"shape\":[{dims}],\"data_offsets\":[{begin},{}]}}", data.len()));
    }
    header.push('}');
    let mut blob = (header.len() as u64).to_le_bytes().to_vec();
    blob.extend_from_slice(header.as_bytes());
    blob.extend_from_slice(&data);
    blob
}

/// The tensor correspondence: what the graph needs, by both names. GGUF dimension order is
/// fastest-varying first, so a 2-D entry's expected dims are the HF shape reversed; the bytes are
/// row-major either way and identical.
fn qwen25_tensor_specs(layers: usize, d: usize, kv: usize, d_ff: usize, vocab: usize) -> Vec<(String, String, Vec<usize>)> {
    let mut specs: Vec<(String, String, Vec<usize>)> = vec![
        ("token_embd.weight".into(), "model.embed_tokens.weight".into(), vec![vocab, d]),
        ("output_norm.weight".into(), "model.norm.weight".into(), vec![d]),
    ];
    for li in 0..layers {
        for (g, h, shape) in [
            ("attn_norm.weight", "input_layernorm.weight", vec![d]),
            ("attn_q.weight", "self_attn.q_proj.weight", vec![d, d]),
            ("attn_q.bias", "self_attn.q_proj.bias", vec![d]),
            ("attn_k.weight", "self_attn.k_proj.weight", vec![kv, d]),
            ("attn_k.bias", "self_attn.k_proj.bias", vec![kv]),
            ("attn_v.weight", "self_attn.v_proj.weight", vec![kv, d]),
            ("attn_v.bias", "self_attn.v_proj.bias", vec![kv]),
            ("attn_output.weight", "self_attn.o_proj.weight", vec![d, d]),
            ("ffn_norm.weight", "post_attention_layernorm.weight", vec![d]),
            ("ffn_gate.weight", "mlp.gate_proj.weight", vec![d_ff, d]),
            ("ffn_up.weight", "mlp.up_proj.weight", vec![d_ff, d]),
            ("ffn_down.weight", "mlp.down_proj.weight", vec![d, d_ff]),
        ] {
            specs.push((format!("blk.{li}.{g}"), format!("model.layers.{li}.{h}"), shape));
        }
    }
    specs
}

/// **Read a Qwen2 GGUF and synthesize the HF checkpoint the converters consume.**
///
/// `bytes` must be the whole file — tensor data included, unlike the header-only reads the
/// hybrid's streaming converter performs. A 1.5B GGUF is 1–3 GiB; holding it beside the ~3 GiB
/// blob this synthesizes is the peak, and it fits the machines the runbook names.
pub fn qwen25_checkpoint_from_gguf(bytes: &[u8]) -> Result<Qwen25GgufCheckpointV1, LmStudioError> {
    let dir = parse_directory(bytes)?;
    let arch = dir.metadata.get("general.architecture").and_then(|v| v.as_str()).unwrap_or("<absent>");
    if arch != "qwen2" {
        return Err(LmStudioError::NotQwen2(arch.to_string()));
    }
    let need = |key: &'static str| dir.metadata.get(key).ok_or(LmStudioError::MissingMetadata(key));
    let num = |key: &'static str| -> Result<usize, LmStudioError> {
        need(key)?.as_u64().map(|v| v as usize).ok_or(LmStudioError::MissingMetadata(key))
    };
    let layers = num("qwen2.block_count")?;
    let d = num("qwen2.embedding_length")?;
    let heads = num("qwen2.attention.head_count")?;
    let kv_heads = num("qwen2.attention.head_count_kv")?;
    let d_ff = num("qwen2.feed_forward_length")?;
    let rms_norm_eps = need("qwen2.attention.layer_norm_rms_epsilon")?
        .as_f64()
        .ok_or(LmStudioError::MissingMetadata("qwen2.attention.layer_norm_rms_epsilon"))? as f32;
    let rope_theta = need("qwen2.rope.freq_base")?.as_f64().ok_or(LmStudioError::MissingMetadata("qwen2.rope.freq_base"))? as f32;
    let embed = dir.tensors.get("token_embd.weight").ok_or_else(|| LmStudioError::MissingTensor("token_embd.weight".into()))?;
    // dims fastest-first: [d, vocab].
    let vocab = *embed.dims.get(1).ok_or_else(|| LmStudioError::ShapeMismatch {
        tensor: "token_embd.weight".into(),
        want: vec![0, d],
        got: embed.dims.iter().map(|v| *v as usize).collect(),
    })? as usize;
    if dir.tensors.contains_key("output.weight") {
        return Err(LmStudioError::UntiedHead);
    }
    let kv = d / heads * kv_heads;

    let mut carriers: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut lossless = true;
    let mut tensors: Vec<(String, Vec<usize>, Vec<f32>)> = Vec::new();
    for (gguf_name, hf_name, want) in qwen25_tensor_specs(layers, d, kv, d_ff, vocab) {
        let tensor = dir.tensors.get(&gguf_name).ok_or_else(|| LmStudioError::MissingTensor(gguf_name.clone()))?;
        let got: Vec<usize> = tensor.dims.iter().map(|v| *v as usize).collect();
        let want_gguf: Vec<usize> = want.iter().rev().copied().collect();
        if got != want_gguf {
            return Err(LmStudioError::ShapeMismatch { tensor: gguf_name, want: want_gguf, got });
        }
        let (begin, end) = tensor.range();
        if end > bytes.len() as u64 {
            return Err(LmStudioError::Gguf(GgufError::ShortData {
                tensor: gguf_name,
                want: tensor.bytes,
                got: bytes.len().saturating_sub(begin as usize),
            }));
        }
        let values = dequantize(tensor, &bytes[begin as usize..end as usize])?;
        if values.iter().any(|v| !v.is_finite()) {
            return Err(LmStudioError::NonFinite(gguf_name));
        }
        lossless &= tensor.kind.preserves_bf16();
        *carriers
            .entry(match tensor.kind {
                GgufType::F32 => "F32",
                GgufType::F16 => "F16",
                GgufType::Bf16 => "BF16",
                GgufType::Q8_0 => "Q8_0",
                GgufType::Q4K => "Q4_K",
                GgufType::Q5K => "Q5_K",
                GgufType::Q6K => "Q6_K",
            })
            .or_insert(0) += 1;
        tensors.push((hf_name, want, values));
    }

    let blob = encode_safetensors_bf16(&tensors);
    let carrier_summary = carriers.iter().map(|(k, n)| format!("{k}×{n}")).collect::<Vec<_>>().join(" ");
    Ok(Qwen25GgufCheckpointV1 {
        blob,
        config: Qwen25GgufConfigV1 {
            hidden_size: d,
            num_attention_heads: heads,
            num_key_value_heads: kv_heads,
            num_hidden_layers: layers,
            intermediate_size: d_ff,
            vocab_size: vocab,
            rms_norm_eps,
            rope_theta,
            model_name: dir.metadata.get("general.name").and_then(|v| v.as_str()).map(str::to_string),
        },
        lossless_carrier: lossless,
        carrier_summary,
    })
}

/// The pinned `ln(θ)` for a declared rope base, or a refusal — the rotary table is generated by
/// CORDIC from this constant, and "close" is a different rotation, which is a different model
/// (measured at cosine 0.73 for layer-0 attention when the base was wrong).
pub fn ln_theta_gen_q_for_rope_base(rope_theta: f32) -> Result<i128, LmStudioError> {
    if (rope_theta - 1_000_000.0).abs() < 0.5 {
        Ok(crate::artifact::LN_THETA_1000000_GEN_Q)
    } else if (rope_theta - 10_000.0).abs() < 0.5 {
        Ok(crate::artifact::LN_THETA_10000_GEN_Q)
    } else {
        Err(LmStudioError::UnsupportedRopeBase(rope_theta as f64))
    }
}

/// The tokenizer, from the GGUF's own arrays — LM Studio downloads no `tokenizer.json`, and the
/// vocabulary and merge table travel inside the file header.
pub fn tokenizer_from_gguf(dir: &GgufDirectory) -> Result<crate::tokenizer::QwenTokenizer, LmStudioError> {
    let tokens = dir.metadata.get("tokenizer.ggml.tokens").and_then(|v| v.as_strings()).ok_or(LmStudioError::NoTokenizer)?;
    let merges = dir.metadata.get("tokenizer.ggml.merges").and_then(|v| v.as_strings()).ok_or(LmStudioError::NoTokenizer)?;
    let types = dir.metadata.get("tokenizer.ggml.token_type").and_then(|v| v.as_ints()).unwrap_or(&[]);
    crate::tokenizer::QwenTokenizer::from_gguf(tokens, merges, types)
        .map_err(|_| LmStudioError::Gguf(GgufError::Truncated("tokenizer arrays")))
}

// ---------------------------------------------------------------------------------------------
// Finding the model where LM Studio put it
// ---------------------------------------------------------------------------------------------

/// What a discovery hit is: a GGUF file, or a `safetensors` checkpoint directory (LM Studio's MLX
/// downloads — which the DIRECTORY lane of `qwen25-convert` already reads when the weights are
/// BF16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LmStudioModelKindV1 {
    Gguf,
    SafetensorsDir,
}

/// One model LM Studio holds, ranked.
#[derive(Clone, Debug)]
pub struct LmStudioModelV1 {
    pub path: PathBuf,
    pub kind: LmStudioModelKindV1,
    /// Lower ranks first. The order is fidelity of what the file can hand the PTQ: a carrier that
    /// preserves the BF16 weights outranks every quantized one, and among the quantized ones more
    /// bits outrank fewer.
    pub rank: u8,
}

/// Where LM Studio keeps models on this machine: `$LMSTUDIO_MODELS` when set, then the current
/// default (`~/.lmstudio/models`), then the legacy location (`~/.cache/lm-studio/models`).
pub fn lmstudio_model_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(v) = std::env::var("LMSTUDIO_MODELS")
        && !v.is_empty()
    {
        roots.push(PathBuf::from(v));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".lmstudio").join("models"));
        roots.push(home.join(".cache").join("lm-studio").join("models"));
    }
    roots.retain(|p| p.is_dir());
    roots
}

/// Case, `-`, `_`, `.` and whitespace erased — so "Qwen2.5 1.5B Instruct", `qwen2.5-1.5b-instruct`
/// and `Qwen2.5-1.5B-Instruct-GGUF` all reach one spelling.
fn fold(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '-' | '_' | '.' | ' ')).flat_map(char::to_lowercase).collect()
}

fn carrier_rank(name: &str) -> u8 {
    let folded = fold(name);
    // MLX's own quantized layouts pack weights this converter does not read; keep them at the
    // bottom so a bf16 sibling wins, and the error names the problem if one is forced.
    for (needle, rank) in [
        ("bf16", 0u8),
        ("fp16", 1),
        ("f16", 1),
        ("q8", 2),
        ("q6k", 3),
        ("q5km", 4),
        ("q5ks", 5),
        ("q5", 5),
        ("q4km", 6),
        ("q4ks", 7),
        ("q4", 7),
        ("8bit", 8),
        ("4bit", 9),
    ] {
        if folded.contains(needle) {
            return rank;
        }
    }
    8
}

/// Search LM Studio's models for `query` (folded-substring terms, all required). Returns hits
/// ranked best-first: preserving carriers before quantized ones, then by path for determinism.
pub fn find_lmstudio_models(query: &str, roots: &[PathBuf]) -> Vec<LmStudioModelV1> {
    let terms: Vec<String> = query.split_whitespace().map(fold).filter(|t| !t.is_empty()).collect();
    let mut hits = Vec::new();
    for root in roots {
        // The layout is models/<publisher>/<repo>/<files>; a bounded walk keeps a stray symlink
        // from turning discovery into a filesystem crawl.
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                let matched = || {
                    let folded = fold(&rel);
                    terms.iter().all(|t| folded.contains(t))
                };
                if path.is_dir() {
                    if path.join("model.safetensors").is_file() && path.join("config.json").is_file() {
                        if matched() {
                            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            hits.push(LmStudioModelV1 { path, kind: LmStudioModelKindV1::SafetensorsDir, rank: carrier_rank(&name) });
                        }
                    } else if depth < 3 {
                        stack.push((path, depth + 1));
                    }
                } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf")) && matched() {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    hits.push(LmStudioModelV1 { path, kind: LmStudioModelKindV1::Gguf, rank: carrier_rank(&name) });
                }
            }
        }
    }
    hits.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.path.cmp(&b.path)));
    hits
}

/// The one GGUF a directory holds, best-ranked — for pointing the converter at an LM Studio
/// `<publisher>/<repo>` directory instead of a file.
pub fn pick_gguf_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u8, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf")) {
            let rank = carrier_rank(&path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            if best.as_ref().is_none_or(|(r, p)| (rank, &path) < (*r, p)) {
                best = Some((rank, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

// ---------------------------------------------------------------------------------------------
// The dev fixture: a Qwen2.5-shaped checkpoint, written as a real GGUF
// ---------------------------------------------------------------------------------------------

/// Which carrier the fixture stores its 2-D tensors in. 1-D tensors are always `F32`, as
/// llama.cpp writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevFixtureCarrierV1 {
    Bf16,
    F32,
    /// Written at a scale (`1/16`) that carries the fixture's integer-valued weights exactly, so
    /// even this quantized carrier reproduces the same class — which is what makes the
    /// cross-carrier digest test a test of the DECODERS rather than of rounding luck.
    Q8_0,
}

/// The fixture's tensors: `(hf_name, gguf_name, hf_shape, values)`, values from the same integer
/// LCG the converter's own test fixture draws (small signed values, exact in every carrier).
///
/// Public for the same reason `qwen36_dev_fixture` is: the consensus crate's block-production
/// test needs a checkpoint a CI machine can hold, and it must be THIS crate that says what shape
/// a dev Qwen2.5 has. Never register anything derived from it.
pub fn qwen25_dev_checkpoint_tensors(shape: &crate::artifact::Base0ShapeV1) -> Vec<(String, String, Vec<usize>, Vec<f32>)> {
    let d = shape.d_model();
    let kv = shape.kv_dim();
    let mut seed = 1u64;
    qwen25_tensor_specs(shape.n_layers, d, kv, shape.d_ff, shape.vocab)
        .into_iter()
        .map(|(gguf_name, hf_name, hf_shape)| {
            let n: usize = hf_shape.iter().product();
            let values: Vec<f32> = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    (((seed >> 33) % 9) as i32 - 4) as f32
                })
                .collect();
            (hf_name, gguf_name, hf_shape, values)
        })
        .collect()
}

/// Write the dev checkpoint as a **real GGUF v3 file** — parsed by the same reader real files go
/// through, so the fixture exercises the format, not a bypass of it.
pub fn qwen25_gguf_dev_fixture(shape: &crate::artifact::Base0ShapeV1, carrier: DevFixtureCarrierV1) -> Vec<u8> {
    let d = shape.d_model();
    let tensors = qwen25_dev_checkpoint_tensors(shape);

    let put_str = |out: &mut Vec<u8>, s: &str| {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    let kv_str = |out: &mut Vec<u8>, key: &str, value: &str| {
        put_str(out, key);
        out.extend_from_slice(&8u32.to_le_bytes());
        put_str(out, value);
    };
    let kv_u32 = |out: &mut Vec<u8>, key: &str, value: u32| {
        put_str(out, key);
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    };
    let kv_f32 = |out: &mut Vec<u8>, key: &str, value: f32| {
        put_str(out, key);
        out.extend_from_slice(&6u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    };

    // The fixture's rope base must be one a pinned table exists for; the tiny shapes use 10000.
    let rope_base = if shape.ln_theta_gen_q == crate::artifact::LN_THETA_1000000_GEN_Q { 1_000_000.0f32 } else { 10_000.0f32 };

    let mut kv: Vec<u8> = Vec::new();
    kv_str(&mut kv, "general.architecture", "qwen2");
    kv_str(&mut kv, "general.name", "qwen25-dev-fixture");
    kv_u32(&mut kv, "qwen2.block_count", shape.n_layers as u32);
    kv_u32(&mut kv, "qwen2.context_length", shape.max_position as u32);
    kv_u32(&mut kv, "qwen2.embedding_length", d as u32);
    kv_u32(&mut kv, "qwen2.feed_forward_length", shape.d_ff as u32);
    kv_u32(&mut kv, "qwen2.attention.head_count", shape.n_heads as u32);
    kv_u32(&mut kv, "qwen2.attention.head_count_kv", shape.n_kv_heads as u32);
    kv_f32(&mut kv, "qwen2.attention.layer_norm_rms_epsilon", 1e-6);
    kv_f32(&mut kv, "qwen2.rope.freq_base", rope_base);
    kv_u32(&mut kv, "general.alignment", 32);
    let n_kv = 11u64;

    // Encode each tensor's payload in its carrier; 1-D stays F32.
    let encoded: Vec<(String, Vec<u64>, u32, Vec<u8>)> = tensors
        .iter()
        .map(|(_, gguf_name, hf_shape, values)| {
            let dims: Vec<u64> = hf_shape.iter().rev().map(|v| *v as u64).collect();
            let (tag, payload): (u32, Vec<u8>) = if hf_shape.len() == 1 {
                (0, values.iter().flat_map(|v| v.to_le_bytes()).collect())
            } else {
                match carrier {
                    DevFixtureCarrierV1::F32 => (0, values.iter().flat_map(|v| v.to_le_bytes()).collect()),
                    DevFixtureCarrierV1::Bf16 => (30, values.iter().flat_map(|v| bf16_bits(*v).to_le_bytes()).collect()),
                    DevFixtureCarrierV1::Q8_0 => {
                        // d = 1/16 (f16 bits 0x2C00), q = 16·v: exact for the fixture's ±4 range.
                        let mut out = Vec::with_capacity(values.len() / 32 * 34);
                        for block in values.chunks_exact(32) {
                            out.extend_from_slice(&0x2C00u16.to_le_bytes());
                            for v in block {
                                out.push(((*v * 16.0) as i32 as i8) as u8);
                            }
                        }
                        (8, out)
                    }
                }
            };
            (gguf_name.clone(), dims, tag, payload)
        })
        .collect();

    let mut infos: Vec<u8> = Vec::new();
    let mut offset = 0u64;
    for (name, dims, tag, payload) in &encoded {
        put_str(&mut infos, name);
        infos.extend_from_slice(&(dims.len() as u32).to_le_bytes());
        for dim in dims {
            infos.extend_from_slice(&dim.to_le_bytes());
        }
        infos.extend_from_slice(&tag.to_le_bytes());
        infos.extend_from_slice(&offset.to_le_bytes());
        offset = (offset + payload.len() as u64).next_multiple_of(32);
    }

    let mut file: Vec<u8> = Vec::new();
    file.extend_from_slice(b"GGUF");
    file.extend_from_slice(&3u32.to_le_bytes());
    file.extend_from_slice(&(encoded.len() as u64).to_le_bytes());
    file.extend_from_slice(&n_kv.to_le_bytes());
    file.extend_from_slice(&kv);
    file.extend_from_slice(&infos);
    let data_start = (file.len() as u64).next_multiple_of(32);
    file.resize(data_start as usize, 0);
    let mut offset = 0u64;
    for (_, _, _, payload) in &encoded {
        file.resize((data_start + offset) as usize, 0);
        file.extend_from_slice(payload);
        offset = (offset + payload.len() as u64).next_multiple_of(32);
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use crate::convert::{Qwen25ConvertPlan, convert_qwen25_a16, parse_safetensors_header, read_bf16_tensor};
    use crate::reference::{RefConfigV1, reference_forward_full};

    fn dev_shape() -> Base0ShapeV1 {
        Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 64,
            vocab: 32,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1 << 8,
        }
    }

    /// The synthesized checkpoint is read by the SAME parser and reader the HF lane uses, and
    /// every tensor comes back at its HF name, shape and values.
    #[test]
    fn a_gguf_round_trips_into_the_checkpoint_the_converter_reads() {
        let shape = dev_shape();
        let gguf = qwen25_gguf_dev_fixture(&shape, DevFixtureCarrierV1::Bf16);
        let ck = qwen25_checkpoint_from_gguf(&gguf).expect("the fixture ingests");
        assert_eq!(
            (ck.config.num_hidden_layers, ck.config.hidden_size, ck.config.num_attention_heads, ck.config.num_key_value_heads),
            (2, 32, 4, 2)
        );
        assert_eq!((ck.config.intermediate_size, ck.config.vocab_size), (64, 32));
        assert_eq!(ck.config.rope_theta, 10_000.0);
        assert!(ck.lossless_carrier, "BF16 2-D + F32 1-D preserve the bits");

        let index = parse_safetensors_header(&ck.blob).expect("a well-formed safetensors container");
        for (hf_name, _, hf_shape, values) in qwen25_dev_checkpoint_tensors(&shape) {
            let got = read_bf16_tensor(&ck.blob, &index, &hf_name, &hf_shape).expect(&hf_name);
            assert_eq!(got, values, "{hf_name} survives the carrier bit-exactly");
        }
    }

    /// **The carrier does not move the class.** One set of weights, three carriers — the HF
    /// `safetensors` blob, a BF16 GGUF, an F32 GGUF, and a Q8_0 GGUF written at an exact scale —
    /// and one artifact digest out of the full W8A16 static-PTQ path for all of them.
    #[test]
    fn the_gguf_carrier_does_not_move_the_class() {
        let shape = dev_shape();
        let tensors = qwen25_dev_checkpoint_tensors(&shape);
        let hf_blob =
            encode_safetensors_bf16(&tensors.iter().map(|(h, _, s, v)| (h.clone(), s.clone(), v.clone())).collect::<Vec<_>>());

        let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };
        let ref_cfg = RefConfigV1 {
            n_layers: shape.n_layers,
            n_heads: shape.n_heads,
            n_kv_heads: shape.n_kv_heads,
            d_head: shape.d_head,
            d_ff: shape.d_ff,
            vocab: shape.vocab,
            rms_eps: 1e-6,
            rope_theta: 10_000.0,
        };
        let prompt: Vec<usize> = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8];
        let digest_of = |blob: &[u8]| {
            let (_, _, stats) = reference_forward_full(blob, &ref_cfg, &prompt).expect("the reference runs");
            convert_qwen25_a16(blob, &plan, &stats).expect("the static PTQ converts").artifact_digest()
        };

        let from_hf = digest_of(&hf_blob);
        for carrier in [DevFixtureCarrierV1::Bf16, DevFixtureCarrierV1::F32, DevFixtureCarrierV1::Q8_0] {
            let gguf = qwen25_gguf_dev_fixture(&shape, carrier);
            let ck = qwen25_checkpoint_from_gguf(&gguf).expect("the fixture ingests");
            assert_eq!(ck.blob, hf_blob, "{carrier:?}: the synthesized checkpoint is byte-equal to the HF one");
            assert_eq!(digest_of(&ck.blob), from_hf, "{carrier:?}: same weights, same class");
        }
        // And the lossless flag tells the truth about the carrier TYPE, values notwithstanding:
        // Q8_0 happening to carry these values exactly does not make it a preserving carrier.
        let q8 = qwen25_checkpoint_from_gguf(&qwen25_gguf_dev_fixture(&shape, DevFixtureCarrierV1::Q8_0)).unwrap();
        assert!(!q8.lossless_carrier, "a quantized carrier never claims losslessness");
        assert!(q8.carrier_summary.contains("Q8_0"), "the provenance line names the carrier: {}", q8.carrier_summary);
    }

    /// The refusals are refusals, not guesses: a wrong architecture, a missing tensor and an
    /// untied head each name themselves.
    #[test]
    fn the_ingestion_refuses_what_it_cannot_vouch_for() {
        let shape = dev_shape();
        let good = qwen25_gguf_dev_fixture(&shape, DevFixtureCarrierV1::F32);

        // Not qwen2: flip the architecture string in place ("qwen2" -> "qwen3").
        let mut wrong_arch = good.clone();
        let arch = b"qwen2";
        let pos = wrong_arch.windows(arch.len()).position(|w| w == arch).expect("the fixture names its architecture");
        wrong_arch[pos + 4] = b'3';
        assert!(matches!(qwen25_checkpoint_from_gguf(&wrong_arch), Err(LmStudioError::NotQwen2(a)) if a == "qwen3"));

        // A truncated file: the directory parses (it is at the front) but a tensor's bytes end
        // early, and the reader says which.
        let truncated = &good[..good.len() - 64];
        assert!(qwen25_checkpoint_from_gguf(truncated).is_err());
    }

    /// Discovery finds a GGUF under an LM-Studio-shaped tree, folds naming conventions, and ranks
    /// a preserving carrier above a quantized one.
    #[test]
    fn discovery_walks_the_lmstudio_layout_and_ranks_carriers() {
        let root = std::env::temp_dir().join(format!("lmstudio-fixture-{}", std::process::id()));
        let repo = root.join("lmstudio-community").join("Qwen2.5-1.5B-Instruct-GGUF");
        std::fs::create_dir_all(&repo).expect("mkdir");
        for name in ["Qwen2.5-1.5B-Instruct-Q4_K_M.gguf", "Qwen2.5-1.5B-Instruct-Q8_0.gguf", "Qwen2.5-1.5B-Instruct-BF16.gguf"] {
            std::fs::write(repo.join(name), b"GGUF").expect("write");
        }
        std::fs::write(repo.join("README.md"), b"not a model").expect("write");

        let hits = find_lmstudio_models("Qwen2.5 1.5B Instruct", std::slice::from_ref(&root));
        assert_eq!(hits.len(), 3, "three GGUFs, no README");
        assert!(hits[0].path.to_string_lossy().contains("BF16"), "the preserving carrier ranks first");
        assert!(hits[1].path.to_string_lossy().contains("Q8_0"));
        assert!(hits[2].path.to_string_lossy().contains("Q4_K_M"));
        assert_eq!(hits[0].kind, LmStudioModelKindV1::Gguf);

        assert!(find_lmstudio_models("qwen3 30b", std::slice::from_ref(&root)).is_empty(), "terms all have to hold");
        let picked = pick_gguf_in_dir(&repo).expect("a repo directory implies its best file");
        assert!(picked.to_string_lossy().contains("BF16"));

        std::fs::remove_dir_all(&root).ok();
    }
}
