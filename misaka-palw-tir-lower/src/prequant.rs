//! **Pre-quantised checkpoints: the stored integers, read exactly** (GPTQ and AWQ safetensors).
//!
//! A GPTQ or AWQ linear stores its weight as small integers `q` with a scale `s` and an integer
//! zero point `z` per (group of input columns, output row):
//!
//! ```text
//! W[o, i] = s[g(i), o] · (q[i, o] − z[g(i), o])
//! ```
//!
//! [`QWeight`] holds exactly those integers and scales (the fp16 scale widened to `f64`, exact),
//! unpacked from the format's `int32` words. Its [`QWeight::dequant`] is the float weight the
//! format defines — the float reference computes with it, and a Hugging Face model with those
//! weights substituted is the fidelity reference. The lowering (`crate::lower`, `lower_linear_q`)
//! does not re-quantise anything: the program multiplies the stored integers, applies the
//! per-group scales exactly, and rounds once, at the output narrowing every projection has.
//!
//! # The formats
//!
//! | format | `qweight` | `qzeros` | `scales` | order | zero point |
//! | --- | --- | --- | --- | --- | --- |
//! | GPTQ (`gptq`, v1) | `i32 [in·b/32, out]`, packed along the input | `i32 [G, out·b/32]` | `f16 [G, out]` | `g_idx [in]` (act-order permutes it) | stored `z − 1` (mod `2^b`) |
//! | GPTQ (`gptq_v2`) | same | same | same | same | stored `z` |
//! | AWQ (GEMM) | `i32 [in, out·b/32]`, packed along the output in the order `[0,2,4,6,1,3,5,7]` | `i32 [G, out·b/32]`, same order | `f16 [G, out]` | `i / group` | stored `z` |
//!
//! Bits: GPTQ 2, 4 or 8 (3-bit words straddle `int32` boundaries and are refused); AWQ 4 (the GEMM
//! packing is defined for 4 bits only). `group_size = −1` is one group per row.
//!
//! GPTQ v1 edge: a zero point of 0 is stored as `2^b − 1`; AutoGPTQ reads it back as 0 (`(v + 1)
//! mod 2^b`, followed here), while GPTQModel's v1→v2 conversion adds `0x11111111` to the word and
//! carries into the neighbouring zero. Symmetric checkpoints (`z = 2^(b−1)`) never hit it.

use crate::error::{LowerError, Result};
use crate::quantfmt::{NeedsDescriptor, QuantFormat, QuantRegistry};
use crate::weights::Tensor;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A group-quantised linear format, as `config.json`'s `quantization_config` names it.
///
/// Every variant but [`QFormat::Gguf`] is read from the checkpoint's tensors by a descriptor
/// (`crate::quantfmt`, `misaka.palw.quant-format.v1`): `Gptq` and `Awq` are the two built-ins whose
/// configuration this module checks by hand (they carry model-family gates a descriptor cannot say),
/// `Described` is any other format a descriptor defines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum QFormat {
    /// GPTQ (AutoGPTQ / GPTQModel). `group = 0`: one group per row (`group_size = −1`).
    Gptq { bits: u8, group: usize, desc_act: bool, sym: bool, v2: bool },
    /// AWQ, GEMM packing (AutoAWQ).
    Awq { bits: u8, group: usize },
    /// GGUF (llama.cpp) block formats: the program layout of one module over every layer's tensor
    /// type (`crate::gguf`).
    Gguf { layout: QLayout },
    /// A format a descriptor defines (FP8 with block scales, compressed-tensors, or one a caller
    /// supplied), with the parameters its configuration gave.
    Described(Described),
}

/// A `tensors` descriptor with its parameters bound: what a module of this checkpoint is stored as.
#[derive(Clone, Debug)]
pub struct Described {
    pub format: Arc<QuantFormat>,
    pub params: BTreeMap<String, i64>,
}

impl PartialEq for Described {
    fn eq(&self, o: &Self) -> bool {
        self.format.digest() == o.format.digest() && self.params == o.params
    }
}
impl Eq for Described {}

impl Serialize for Described {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Described", 3)?;
        st.serialize_field("format", self.format.name())?;
        st.serialize_field("digest", &self.format.digest_hex())?;
        st.serialize_field("params", &self.params)?;
        st.end()
    }
}

impl Described {
    /// Bind `params` to a `tensors` descriptor (the descriptor's layout must evaluate under them).
    pub fn new(format: Arc<QuantFormat>, params: BTreeMap<String, i64>) -> Result<Described> {
        match (format.as_tensors(), format.as_virtual()) {
            (Some(t), _) => {
                t.layout(&params).map_err(|e| LowerError::bad(format!("quant format `{}`: {e}", format.name())))?;
            }
            (None, Some(_)) => {}
            (None, None) => return Err(LowerError::bad(format!("quant format `{}` is neither a tensors nor a virtual format", format.name()))),
        }
        Ok(Described { format, params })
    }
}

/// The built-in descriptor a [`QFormat::Gptq`] or [`QFormat::Awq`] decodes through, with its
/// parameters.
fn builtin_binding(fmt: &QFormat) -> Option<(Arc<QuantFormat>, BTreeMap<String, i64>)> {
    let reg = QuantRegistry::builtin();
    let gs = |g: usize| if g == 0 { -1 } else { g as i64 };
    match fmt {
        QFormat::Gptq { bits, group, sym, v2, .. } => Some((
            reg.named("GPTQ").expect("the built-in GPTQ descriptor").clone(),
            BTreeMap::from([("bits".into(), *bits as i64), ("group_size".into(), gs(*group)), ("v2".into(), *v2 as i64), ("sym".into(), *sym as i64)]),
        )),
        QFormat::Awq { bits, group } => {
            Some((reg.named("AWQ").expect("the built-in AWQ descriptor").clone(), BTreeMap::from([("bits".into(), *bits as i64), ("group_size".into(), gs(*group))])))
        }
        QFormat::Described(d) => Some((d.format.clone(), d.params.clone())),
        QFormat::Gguf { .. } => None,
    }
}

impl QFormat {
    /// The descriptor and parameters this format's tensors are decoded with (`None`: GGUF, whose
    /// descriptors are per tensor type).
    pub fn binding(&self) -> Option<(Arc<QuantFormat>, BTreeMap<String, i64>)> {
        builtin_binding(self)
    }

    /// Whether the format serves packed tensors as float tensors under another name (MXFP4 experts): the
    /// source presents them, no module is bound to the format, and the lowering sees an ordinary float
    /// checkpoint ([`crate::quantfmt::virt`]).
    pub fn is_virtual(&self) -> bool {
        matches!(self, QFormat::Described(d) if d.format.as_virtual().is_some())
    }

    /// Whether the decode yields stored integers (the exact lowering reads them) rather than floats
    /// (an FP8 weight: the ordinary W8 path).
    pub fn is_integers(&self) -> bool {
        match self {
            QFormat::Described(d) => d.format.as_tensors().is_some_and(|t| t.is_integers()),
            _ => true,
        }
    }

    /// Columns per group, `0` for one group per row.
    pub fn group(&self) -> usize {
        self.layout().group
    }
    pub fn label(&self) -> String {
        let g = |g: usize| if g == 0 { "per-row".to_string() } else { format!("g{g}") };
        match self {
            QFormat::Gptq { bits, group, desc_act, sym, v2 } => format!(
                "gptq{} b{bits} {}{}{}",
                if *v2 { "_v2" } else { "" },
                g(*group),
                if *desc_act { " act-order" } else { "" },
                if *sym { " sym" } else { " asym" }
            ),
            QFormat::Awq { bits, group } => format!("awq b{bits} {}", g(*group)),
            QFormat::Gguf { layout } => format!("gguf {}{}", g(layout.group), if layout.offset_term { " +min" } else { "" }),
            QFormat::Described(d) => {
                let p: Vec<String> = d.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
                format!("{} {}", d.format.name(), p.join(" "))
            }
        }
    }
    /// The program structure this format lowers to ([`QLayout`]): GPTQ's column order is data
    /// (`g_idx`) and GPTQModel's group-aware reordering permutes it without `desc_act`, so every
    /// GPTQ projection gathers its input through it; 8-bit asymmetric codes need the offset term.
    /// Those rules are the descriptors' (`decode.order`, `decode.offset_term`).
    pub fn layout(&self) -> QLayout {
        match self {
            QFormat::Gguf { layout } => *layout,
            // A virtual format binds no module: there is no program structure to describe.
            other if other.is_virtual() => QLayout { group: 0, order: false, offset_term: false },
            other => {
                let (f, params) = builtin_binding(other).expect("a tensors format");
                let l = f
                    .as_tensors()
                    .expect("a tensors descriptor")
                    .layout(&params)
                    .unwrap_or_else(|e| panic!("`{}` was bound to parameters its layout does not evaluate under: {e}", f.name()));
                QLayout { group: l.group, order: l.order, offset_term: l.offset_term }
            }
        }
    }
}

/// What a quantised projection's TIR structure depends on (the rest is data).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct QLayout {
    /// Columns per group; `0`: the whole row is one group.
    pub group: usize,
    /// The input is gathered through a column order first, so that every group is contiguous.
    pub order: bool,
    /// A per-group `c · Σx` term: a zero point the codes cannot absorb within `i8` (8-bit
    /// asymmetric: `q − z` spans `±255`).
    pub offset_term: bool,
}

/// A `quantization_config` this lowerer reads (GPTQ, AWQ).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct QuantConfig {
    pub fmt: QFormat,
    /// GPTQModel's `lm_head`: the head is quantised too.
    pub lm_head: bool,
    /// AWQ's `modules_to_not_convert` (a module is kept in float when its name starts or ends with
    /// one of these).
    pub skip: Vec<String>,
    /// GPTQ's `modules_in_block_to_quantize`, flattened; `None`: every linear of a block.
    pub only: Option<Vec<String>>,
    /// GGUF: the layout of each quantised module (an HF module-name template with `{L}`), from the
    /// file's tensor types; every other module is float.
    pub per_module: std::collections::BTreeMap<String, QLayout>,
}

impl QuantConfig {
    /// Whether the linear module `name` (a full module name) is stored quantised.
    pub fn converts(&self, name: &str) -> bool {
        if skip_matches(&self.skip, name) {
            return false;
        }
        match &self.only {
            None => true,
            Some(list) => list.iter().any(|m| name.ends_with(m.as_str())),
        }
    }
}

/// Whether a module name is among the modules a configuration keeps in float. An entry is
/// `exact:<name>`, `contains:<text>` or `re:<pattern>` (a descriptor's `skip`, see
/// `quantfmt::desc::ConfigDesc`), or — AWQ's own — a bare prefix or suffix of the name.
pub fn skip_matches(skip: &[String], name: &str) -> bool {
    skip.iter().any(|k| {
        if let Some(p) = k.strip_prefix("re:") {
            regex_prefix_match(p, name)
        } else if let Some(n) = k.strip_prefix("exact:") {
            name == n
        } else if let Some(n) = k.strip_prefix("contains:") {
            name.contains(n)
        } else {
            name.starts_with(k.as_str()) || name.ends_with(k.as_str())
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Re {
    Any,
    Lit(char),
}

/// The regular-expression subset a configuration's `ignore` list is written in: literals, `.`,
/// `.*`, escapes of a punctuation character, `^` at the start and `$` at the end. Anything else is
/// refused by name rather than guessed at.
fn regex_parse(p: &str) -> Result<(Vec<(Re, bool)>, bool)> {
    let p = p.strip_prefix('^').unwrap_or(p);
    let (p, end) = match p.strip_suffix('$') {
        Some(q) if !q.ends_with('\\') => (q, true),
        _ => (p, false),
    };
    let mut out: Vec<(Re, bool)> = Vec::new();
    let mut it = p.chars().peekable();
    while let Some(c) = it.next() {
        let tok = match c {
            '\\' => match it.next() {
                Some(e) if e.is_ascii_punctuation() => Re::Lit(e),
                other => return Err(nl(format!("`ignore` pattern `{p}`: the escape \\{} is outside the subset read (literals, `.`, `.*`, `^`, `$`)", other.map(String::from).unwrap_or_default()))),
            },
            '.' => Re::Any,
            '*' | '+' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '^' | '$' => {
                return Err(nl(format!("`ignore` pattern `{p}`: `{c}` is outside the subset read (literals, `.`, `.*`, `^`, `$`)")));
            }
            c => Re::Lit(c),
        };
        let star = it.peek() == Some(&'*');
        if star {
            if tok != Re::Any {
                return Err(nl(format!("`ignore` pattern `{p}`: `*` after a literal is outside the subset read (only `.*`)")));
            }
            it.next();
        }
        out.push((tok, star));
    }
    Ok((out, end))
}

/// Check that a `re:` entry is in the subset (the refusal is raised when the configuration is read).
pub fn check_skip_pattern(entry: &str) -> Result<()> {
    match entry.strip_prefix("re:") {
        Some(p) => regex_parse(p).map(|_| ()),
        None => Ok(()),
    }
}

/// `re.match(pattern, name)` (anchored at the start; at the end only with `$`) over the subset.
fn regex_prefix_match(p: &str, name: &str) -> bool {
    let Ok((toks, end)) = regex_parse(p) else { return false };
    let s: Vec<char> = name.chars().collect();
    fn go(t: &[(Re, bool)], s: &[char], end: bool) -> bool {
        match t.split_first() {
            None => !end || s.is_empty(),
            Some(((tok, star), rest)) => {
                let one = |c: char| match tok {
                    Re::Any => true,
                    Re::Lit(l) => *l == c,
                };
                if *star {
                    (0..=s.len()).take_while(|k| s[..*k].iter().all(|c| one(*c))).any(|k| go(rest, &s[k..], end))
                } else {
                    s.first().is_some_and(|c| one(*c)) && go(rest, &s[1..], end)
                }
            }
        }
    }
    go(&toks, &s, end)
}

/// `quantization_config` keys that never change the stored weights' meaning: tooling, calibration
/// and kernel choices.
const GPTQ_INERT: &[&str] = &[
    "quant_method",
    "batch_size",
    "block_name_to_quantize",
    "cache_block_outputs",
    "damp_percent",
    "damp_auto_increment",
    "dataset",
    "exllama_config",
    "max_input_length",
    "model_seqlen",
    "module_name_preceding_first_block",
    "pad_token_id",
    "tokenizer",
    "true_sequential",
    "use_cuda_fp16",
    "use_exllama",
    "disable_exllama",
    "static_groups",
    "model_name_or_path",
    "model_file_base_name",
    "meta",
    "backend",
    "act_group_aware",
    "mse",
    "v2",
    "v2_alpha",
    "v2_memory_device",
    "hyb_act",
    "offload_to_disk",
    "offload_to_disk_path",
    "pack_impl",
    "fail_safe",
    "adapter",
    "rotation",
    "hessian_chunk_size",
    "gptaq",
    "gptaq_alpha",
    "gptaq_memory_device",
    "mock_quantization",
    "buffered_fwd",
    "use_marlin",
];

const AWQ_INERT: &[&str] = &[
    "quant_method",
    "backend",
    "do_fuse",
    "fuse_max_seq_len",
    "exllama_config",
    "modules_to_fuse",
    // `AwqConfig` subclasses `GPTQConfig` in transformers ≥ 5: its saved dict carries the GPTQ
    // calibration fields, which say nothing about the stored AWQ tensors.
    "damp_percent",
    "true_sequential",
    "dataset",
    "tokenizer",
    "batch_size",
    "pad_token_id",
    "model_seqlen",
    "block_name_to_quantize",
    "module_name_preceding_first_block",
    "cache_block_outputs",
    "max_input_length",
    "modules_in_block_to_quantize",
    "meta",
    "act_group_aware",
    "sym",
];

fn nl(s: String) -> LowerError {
    LowerError::not_lowerable(s)
}

fn get_u(q: &serde_json::Map<String, Value>, k: &str) -> Result<Option<i64>> {
    match q.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_i64().map(Some).ok_or_else(|| LowerError::bad(format!("quantization_config.{k} is not an integer"))),
    }
}

fn get_b(q: &serde_json::Map<String, Value>, k: &str) -> Result<Option<bool>> {
    match q.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| LowerError::bad(format!("quantization_config.{k} is not a bool"))),
    }
}

fn get_s(q: &serde_json::Map<String, Value>, k: &str) -> Result<Option<String>> {
    match q.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_ascii_lowercase()))
            .ok_or_else(|| LowerError::bad(format!("quantization_config.{k} is not a string"))),
    }
}

fn group_of(g: Option<i64>) -> Result<usize> {
    match g {
        None => Ok(128),
        Some(-1) => Ok(0),
        Some(g) if g > 0 => Ok(g as usize),
        Some(g) => Err(LowerError::bad(format!("quantization_config.group_size {g}"))),
    }
}

/// Parse `quantization_config` against the built-in descriptors.
pub fn parse_quant_config(q: &Value, arch: &str, model_type: &str) -> Result<QuantConfig> {
    parse_quant_config_with(q, arch, model_type, QuantRegistry::builtin())
}

/// A `quantization_config` whose `quant_method` a descriptor in `reg` reads: the descriptor says
/// which keys it knows, which conditions must hold and how its parameters are found. A method no
/// descriptor reads is a refusal that says what to supply ([`NeedsDescriptor`]).
fn described_config(q: &Value, method: &str, arch: &str, reg: &QuantRegistry) -> Result<QuantConfig> {
    let format = q.get("format").and_then(Value::as_str);
    let Some(f) = reg.config(method, format) else {
        let name = match format {
            Some(fm) => format!("{method}/{fm}"),
            None => method.to_string(),
        };
        let n = NeedsDescriptor { scheme: "config".into(), id: None, name: Some(name), tensors: Vec::new() };
        return Err(nl(format!("{arch}: pre-quantized checkpoint: {}", n.message(&reg.config_methods()))));
    };
    let r = f.read_config(q).map_err(|e| match e {
        LowerError::NotLowerable(m) => nl(format!("{arch}: {m}")),
        other => other,
    })?;
    for e in &r.skip {
        check_skip_pattern(e)?;
    }
    Ok(QuantConfig { fmt: QFormat::Described(Described::new(f.clone(), r.params)?), lm_head: r.lm_head, skip: r.skip, only: None, per_module: Default::default() })
}

/// Parse `quantization_config` (GPTQ and AWQ by hand; any other method through the descriptor in
/// `reg` that reads it, else refused by name). Every key is either read, known inert, or a refusal.
pub fn parse_quant_config_with(q: &Value, arch: &str, model_type: &str, reg: &QuantRegistry) -> Result<QuantConfig> {
    let qv = q;
    let q = q.as_object().ok_or_else(|| LowerError::bad("quantization_config is not an object"))?;
    let method = q.get("quant_method").and_then(Value::as_str).unwrap_or("unknown").to_ascii_lowercase();
    let known: &[&str] = match method.as_str() {
        "gptq" => &[
            "bits",
            "group_size",
            "desc_act",
            "sym",
            "checkpoint_format",
            "format",
            "lm_head",
            "dynamic",
            "modules_in_block_to_quantize",
            "pack_dtype",
            "is_marlin_format",
        ],
        "awq" => &["bits", "group_size", "zero_point", "version", "format", "modules_to_not_convert", "desc_act"],
        other => return described_config(qv, other, arch, reg),
    };
    let inert = if method == "gptq" { GPTQ_INERT } else { AWQ_INERT };
    let unknown: Vec<&String> = q.keys().filter(|k| !known.contains(&k.as_str()) && !inert.contains(&k.as_str())).collect();
    if !unknown.is_empty() {
        return Err(nl(format!("{arch}: quantization_config ({method}) has keys this lowerer does not know: {unknown:?}")));
    }
    let bits = get_u(q, "bits")?.unwrap_or(4);
    let group = group_of(get_u(q, "group_size")?)?;
    if method == "gptq" {
        if ![2, 4, 8].contains(&bits) {
            return Err(nl(format!(
                "{arch}: GPTQ {bits}-bit (2, 4 and 8 bits are read; 3-bit words straddle int32 boundaries)"
            )));
        }
        let fmt = get_s(q, "checkpoint_format")?.or(get_s(q, "format")?).unwrap_or_else(|| "gptq".into());
        let v2 = match fmt.as_str() {
            "gptq" => false,
            "gptq_v2" => true,
            other => return Err(nl(format!("{arch}: GPTQ checkpoint_format `{other}` (only gptq and gptq_v2 are read)"))),
        };
        if get_b(q, "is_marlin_format")? == Some(true) {
            return Err(nl(format!("{arch}: a Marlin-repacked GPTQ checkpoint")));
        }
        if let Some(p) = get_s(q, "pack_dtype")?
            && p != "int32"
        {
            return Err(nl(format!("{arch}: GPTQ pack_dtype `{p}` (only int32 words are read)")));
        }
        match q.get("dynamic") {
            None | Some(Value::Null) => {}
            Some(Value::Object(m)) if m.is_empty() => {}
            Some(_) => return Err(nl(format!("{arch}: GPTQ `dynamic` per-module overrides"))),
        }
        let only = match q.get("modules_in_block_to_quantize") {
            None | Some(Value::Null) => None,
            Some(Value::Array(outer)) => {
                let mut v = Vec::new();
                for g in outer {
                    let inner = g.as_array().ok_or_else(|| LowerError::bad("modules_in_block_to_quantize is not a list of lists"))?;
                    for m in inner {
                        v.push(m.as_str().ok_or_else(|| LowerError::bad("modules_in_block_to_quantize entry"))?.to_string());
                    }
                }
                Some(v)
            }
            Some(_) => return Err(LowerError::bad("modules_in_block_to_quantize is not a list")),
        };
        let desc_act = get_b(q, "desc_act")?.unwrap_or(false);
        let sym = get_b(q, "sym")?.unwrap_or(true);
        let lm_head = get_b(q, "lm_head")?.unwrap_or(false);
        Ok(QuantConfig {
            fmt: QFormat::Gptq { bits: bits as u8, group, desc_act, sym, v2 },
            lm_head,
            skip: Vec::new(),
            only,
            per_module: Default::default(),
        })
    } else {
        if bits != 4 {
            return Err(nl(format!("{arch}: AWQ {bits}-bit (the GEMM packing is defined for 4 bits)")));
        }
        if get_b(q, "zero_point")? == Some(false) {
            return Err(nl(format!("{arch}: AWQ without zero points")));
        }
        if get_b(q, "desc_act")? == Some(true) {
            return Err(nl(format!("{arch}: AWQ with desc_act")));
        }
        let ver = get_s(q, "version")?.or(get_s(q, "format")?).unwrap_or_else(|| "gemm".into());
        if ver != "gemm" {
            return Err(nl(format!("{arch}: AWQ `{ver}` packing (GEMM is read; GEMV and the Marlin/LLM-AWQ repacks are not)")));
        }
        // AutoAWQ scales the FFN activation per channel for these (`ScaledActivation`).
        const SCALED_ACT: &[&str] = &["starcoder2", "RefinedWebModel", "falcon", "mpt", "gptj", "gpt_neox", "gpt_bigcode", "bloom"];
        if SCALED_ACT.contains(&model_type) {
            return Err(nl(format!("{arch}: AWQ's per-channel activation scales (`ScaledActivation`) for {model_type}")));
        }
        let skip = match q.get("modules_to_not_convert") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .map(|m| m.as_str().map(str::to_string).ok_or_else(|| LowerError::bad("modules_to_not_convert entry")))
                .collect::<Result<_>>()?,
            Some(_) => return Err(LowerError::bad("modules_to_not_convert is not a list")),
        };
        Ok(QuantConfig { fmt: QFormat::Awq { bits: 4, group }, lm_head: false, skip, only: None, per_module: Default::default() })
    }
}

/// A group-quantised `[out, in]` weight, as stored: `W[o, i] = scale[o, g] · (q[o, i] −
/// zero[o, g]) − min[o, g]`, `g = gidx[i]` (`min`: GGUF's float offsets — K-quant minimums,
/// `Q4_1`/`Q5_1`'s `m`; absent for GPTQ and AWQ).
#[derive(Clone, Debug, PartialEq)]
pub struct QWeight {
    pub out: usize,
    pub inp: usize,
    /// Columns per group (every group has exactly this many; the whole row for per-row formats).
    pub group: usize,
    /// The stored integers, `[out, in]`, original column order.
    pub q: Vec<i16>,
    /// `[out, G]`: each (row, group)'s scale — the stored fp16, exactly.
    pub scale: Vec<f64>,
    /// `[out, G]`: each (row, group)'s integer zero point.
    pub zero: Vec<i16>,
    /// `[out, G]`: a float offset subtracted after the scale (exact: an fp16 times a small integer).
    pub min: Option<Vec<f64>>,
    /// `[in]`: the group of each input column.
    pub gidx: Vec<u32>,
    pub bits: u8,
    /// The codes are signed (`Q8_0`), else `[0, 2^bits)`.
    pub signed: bool,
    /// The format (for reports).
    pub label: String,
}

impl QWeight {
    pub fn groups(&self) -> usize {
        self.scale.len() / self.out.max(1)
    }
    /// `W[o, i]`, exactly (an fp16 scale times an integer of ≤ 9 bits is exact in `f64` and `f32`).
    pub fn value(&self, o: usize, i: usize) -> f64 {
        let g = self.gidx[i] as usize;
        let k = o * self.groups() + g;
        let m = self.min.as_ref().map_or(0.0, |m| m[k]);
        self.scale[k] * (self.q[o * self.inp + i] as f64 - self.zero[k] as f64) - m
    }
    /// The float weight the format defines, `[out, in]`.
    pub fn dequant(&self) -> Tensor {
        let mut data = Vec::with_capacity(self.out * self.inp);
        for o in 0..self.out {
            for i in 0..self.inp {
                data.push(self.value(o, i) as f32);
            }
        }
        Tensor::new(vec![self.out, self.inp], data)
    }
    /// The rows `rows` (a fused projection's slice).
    pub fn take_rows(&self, rows: &[usize]) -> Result<QWeight> {
        let g = self.groups();
        if let Some(r) = rows.iter().find(|r| **r >= self.out) {
            return Err(LowerError::weights(format!("quantised weight: row {r} of {}", self.out)));
        }
        let mut q = Vec::with_capacity(rows.len() * self.inp);
        let (mut scale, mut zero) = (Vec::with_capacity(rows.len() * g), Vec::with_capacity(rows.len() * g));
        let mut min = self.min.as_ref().map(|_| Vec::with_capacity(rows.len() * g));
        for &r in rows {
            q.extend_from_slice(&self.q[r * self.inp..(r + 1) * self.inp]);
            scale.extend_from_slice(&self.scale[r * g..(r + 1) * g]);
            zero.extend_from_slice(&self.zero[r * g..(r + 1) * g]);
            if let (Some(dst), Some(src)) = (min.as_mut(), self.min.as_ref()) {
                dst.extend_from_slice(&src[r * g..(r + 1) * g]);
            }
        }
        Ok(QWeight { out: rows.len(), q, scale, zero, min, gidx: self.gidx.clone(), label: self.label.clone(), ..*self })
    }
    /// The columns in the order `cols` (column `j` of the result is column `cols[j]`): the codes
    /// and each column's group move together, so the weight is unchanged column for column.
    pub fn permute_cols(&self, cols: &[usize]) -> Result<QWeight> {
        if cols.len() != self.inp || cols.iter().any(|c| *c >= self.inp) {
            return Err(LowerError::weights(format!("quantised weight: a column order of {} for {} columns", cols.len(), self.inp)));
        }
        let mut q = Vec::with_capacity(self.q.len());
        for o in 0..self.out {
            let row = &self.q[o * self.inp..(o + 1) * self.inp];
            q.extend(cols.iter().map(|c| row[*c]));
        }
        let gidx = cols.iter().map(|c| self.gidx[*c]).collect();
        Ok(QWeight { q, gidx, scale: self.scale.clone(), zero: self.zero.clone(), min: self.min.clone(), label: self.label.clone(), ..*self })
    }
    /// The column order in which every group is contiguous: columns sorted by group, stable. Every
    /// group must have exactly `group` columns.
    pub fn order(&self) -> Result<Vec<u32>> {
        let g = self.groups();
        let mut ord: Vec<u32> = (0..self.inp as u32).collect();
        ord.sort_by_key(|i| (self.gidx[*i as usize], *i));
        let mut count = vec![0usize; g];
        for x in &self.gidx {
            count[*x as usize] += 1;
        }
        if let Some((k, c)) = count.iter().enumerate().find(|(_, c)| **c != self.group) {
            return Err(LowerError::not_lowerable(format!(
                "quantised weight ({}): group {k} has {c} columns, not {} (groups must be equal)",
                self.label, self.group
            )));
        }
        Ok(ord)
    }
}

fn shape2(what: &str, s: &[usize]) -> Result<(usize, usize)> {
    match s {
        [a, b] => Ok((*a, *b)),
        _ => Err(LowerError::weights(format!("{what}: shape {s:?} is not 2-D"))),
    }
}

/// `(word >> (bits·k)) & (2^bits − 1)`.
fn field(word: i32, bits: u8, k: usize) -> i16 {
    (((word as u32) >> (bits as u32 * k as u32)) & ((1u32 << bits) - 1)) as i16
}

/// Unpack a GPTQ linear: `qweight [in·b/32, out]`, `qzeros [G, out·b/32]`, `scales [G, out]`,
/// `g_idx [in]` (absent: `i / group`).
pub fn unpack_gptq(
    fmt: &QFormat,
    qweight: (&[usize], &[i32]),
    qzeros: (&[usize], &[i32]),
    scales: &Tensor,
    g_idx: Option<(&[usize], &[i32])>,
) -> Result<QWeight> {
    let QFormat::Gptq { bits, group, v2, .. } = *fmt else {
        return Err(LowerError::eval("internal: unpack_gptq of a non-GPTQ format"));
    };
    let pack = 32 / bits as usize;
    let (rows, out) = shape2("qweight", qweight.0)?;
    let inp = rows * pack;
    let (ng, sc) = shape2("scales", &scales.shape)?;
    if sc != out {
        return Err(LowerError::weights(format!("GPTQ scales {:?} for {out} outputs", scales.shape)));
    }
    let gs = if group == 0 { inp } else { group };
    if ng != inp.div_ceil(gs) {
        return Err(LowerError::weights(format!("GPTQ: {ng} scale groups for {inp} inputs at group size {gs}")));
    }
    if qzeros.0 != [ng, out.div_ceil(pack)] || out % pack != 0 {
        return Err(LowerError::weights(format!("GPTQ qzeros {:?} for {ng} groups × {out} outputs at {bits} bits", qzeros.0)));
    }
    let gidx: Vec<u32> = match g_idx {
        Some((s, v)) => {
            if s != [inp] {
                return Err(LowerError::weights(format!("GPTQ g_idx {s:?} for {inp} inputs")));
            }
            v.iter()
                .map(|g| {
                    if *g < 0 || *g as usize >= ng {
                        Err(LowerError::weights(format!("GPTQ g_idx value {g} outside [0, {ng})")))
                    } else {
                        Ok(*g as u32)
                    }
                })
                .collect::<Result<_>>()?
        }
        None => (0..inp).map(|i| (i / gs) as u32).collect(),
    };
    let mut q = vec![0i16; out * inp];
    for i in 0..inp {
        let (r, k) = (i / pack, i % pack);
        for o in 0..out {
            q[o * inp + i] = field(qweight.1[r * out + o], bits, k);
        }
    }
    let mask = (1i16 << bits) - 1;
    let zw = out / pack;
    let mut zero = vec![0i16; out * ng];
    let mut scale = vec![0f64; out * ng];
    for g in 0..ng {
        for o in 0..out {
            let raw = field(qzeros.1[g * zw + o / pack], bits, o % pack);
            zero[o * ng + g] = if v2 { raw } else { (raw + 1) & mask };
            scale[o * ng + g] = scales.data[g * out + o] as f64;
        }
    }
    Ok(QWeight { out, inp, group: gs, q, scale, zero, min: None, gidx, bits, signed: false, label: fmt.label() })
}

/// AWQ's GEMM packing: slot `k` of a word holds output `8·c + AWQ_ORDER[k]`.
pub const AWQ_ORDER: [usize; 8] = [0, 2, 4, 6, 1, 3, 5, 7];

/// Unpack an AWQ (GEMM) linear: `qweight [in, out/8]`, `qzeros [G, out/8]`, `scales [G, out]`.
pub fn unpack_awq(fmt: &QFormat, qweight: (&[usize], &[i32]), qzeros: (&[usize], &[i32]), scales: &Tensor) -> Result<QWeight> {
    let QFormat::Awq { bits, group } = *fmt else {
        return Err(LowerError::eval("internal: unpack_awq of a non-AWQ format"));
    };
    if bits != 4 {
        return Err(LowerError::not_lowerable(format!("AWQ {bits}-bit")));
    }
    let (inp, words) = shape2("qweight", qweight.0)?;
    let out = words * 8;
    let gs = if group == 0 { inp } else { group };
    if inp % gs != 0 {
        return Err(LowerError::weights(format!("AWQ: {inp} inputs are not a multiple of the group size {gs}")));
    }
    let ng = inp / gs;
    if scales.shape != [ng, out] || qzeros.0 != [ng, words] {
        return Err(LowerError::weights(format!(
            "AWQ scales {:?} / qzeros {:?} for {ng} groups × {out} outputs",
            scales.shape, qzeros.0
        )));
    }
    let mut slot = [0usize; 8];
    for (k, o) in AWQ_ORDER.iter().enumerate() {
        slot[*o] = k;
    }
    let mut q = vec![0i16; out * inp];
    for i in 0..inp {
        for o in 0..out {
            q[o * inp + i] = field(qweight.1[i * words + o / 8], 4, slot[o % 8]);
        }
    }
    let mut zero = vec![0i16; out * ng];
    let mut scale = vec![0f64; out * ng];
    for g in 0..ng {
        for o in 0..out {
            zero[o * ng + g] = field(qzeros.1[g * words + o / 8], 4, slot[o % 8]);
            scale[o * ng + g] = scales.data[g * out + o] as f64;
        }
    }
    let gidx = (0..inp).map(|i| (i / gs) as u32).collect();
    Ok(QWeight { out, inp, group: gs, q, scale, zero, min: None, gidx, bits, signed: false, label: fmt.label() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack `q [in, out]` (row-major over `in`) the GPTQ way.
    fn pack_gptq_q(q: &[Vec<u32>], bits: u8) -> Vec<i32> {
        let (inp, out) = (q.len(), q[0].len());
        let pack = 32 / bits as usize;
        let mut w = vec![0u32; inp / pack * out];
        for i in 0..inp {
            for o in 0..out {
                w[(i / pack) * out + o] |= q[i][o] << (bits as usize * (i % pack));
            }
        }
        w.into_iter().map(|x| x as i32).collect()
    }

    #[test]
    fn gptq_unpacks_codes_zeros_and_act_order_groups() {
        // 8 inputs, 8 outputs, 4 bits, groups of 4 with an act-order g_idx.
        let bits = 4u8;
        let (inp, out) = (8usize, 8usize);
        let q: Vec<Vec<u32>> = (0..inp).map(|i| (0..out).map(|o| ((i * 3 + o * 5) % 16) as u32).collect()).collect();
        let qw = pack_gptq_q(&q, bits);
        // Zero points per (group, out): z = 1 + (g + o) % 15; v1 stores z − 1.
        let z = |g: usize, o: usize| (1 + (g + o) % 15) as u32;
        let mut qz = vec![0u32; 2];
        for g in 0..2 {
            for o in 0..out {
                qz[g] |= ((z(g, o) - 1) & 15) << (4 * o);
            }
        }
        let qz: Vec<i32> = qz.into_iter().map(|x| x as i32).collect();
        let scales = Tensor::new(vec![2, out], (0..2 * out).map(|k| 0.5 + k as f32 * 0.125).collect());
        let g_idx: Vec<i32> = vec![1, 0, 0, 1, 1, 0, 1, 0];
        let fmt = QFormat::Gptq { bits, group: 4, desc_act: true, sym: false, v2: false };
        let w = unpack_gptq(&fmt, (&[1, out], &qw), (&[2, 1], &qz), &scales, Some((&[inp], &g_idx))).unwrap();
        for i in 0..inp {
            for o in 0..out {
                let g = g_idx[i] as usize;
                let want = scales.data[g * out + o] as f64 * (q[i][o] as f64 - z(g, o) as f64);
                assert_eq!(w.value(o, i), want, "({o}, {i})");
            }
        }
        let ord = w.order().unwrap();
        assert_eq!(ord, vec![1, 2, 5, 7, 0, 3, 4, 6]);
        // v2 reads the zero as stored.
        let fmt2 = QFormat::Gptq { bits, group: 4, desc_act: true, sym: false, v2: true };
        let w2 = unpack_gptq(&fmt2, (&[1, out], &qw), (&[2, 1], &qz), &scales, Some((&[inp], &g_idx))).unwrap();
        assert_eq!(w2.zero[0] + 1, w.zero[0]);
        // A row slice keeps each row's groups.
        let t = w.take_rows(&[3, 5]).unwrap();
        assert_eq!(t.value(1, 6), w.value(5, 6));
    }

    #[test]
    fn gptq_eight_bit_and_per_row_groups() {
        let (inp, out) = (8usize, 4usize);
        let q: Vec<Vec<u32>> = (0..inp).map(|i| (0..out).map(|o| ((i * 37 + o * 91) % 256) as u32).collect()).collect();
        let qw = pack_gptq_q(&q, 8);
        let qz = vec![(127u32 | (127 << 8) | (127 << 16) | (127 << 24)) as i32];
        let scales = Tensor::new(vec![1, out], vec![0.25, 0.5, 1.0, 2.0]);
        let fmt = QFormat::Gptq { bits: 8, group: 0, desc_act: false, sym: true, v2: false };
        let w = unpack_gptq(&fmt, (&[2, out], &qw), (&[1, 1], &qz), &scales, None).unwrap();
        assert_eq!(w.group, 8);
        assert_eq!(w.value(2, 5), 1.0 * (q[5][2] as f64 - 128.0));
    }

    #[test]
    fn awq_unpacks_the_interleaved_order() {
        let (inp, out, gs) = (4usize, 8usize, 2usize);
        let q: Vec<Vec<u32>> = (0..inp).map(|i| (0..out).map(|o| ((i * 7 + o * 3) % 16) as u32).collect()).collect();
        let mut qw = vec![0u32; inp];
        for i in 0..inp {
            for (k, o) in AWQ_ORDER.iter().enumerate() {
                qw[i] |= q[i][*o] << (4 * k);
            }
        }
        let z: Vec<Vec<u32>> = (0..2).map(|g| (0..out).map(|o| ((g * 5 + o) % 16) as u32).collect()).collect();
        let mut qz = vec![0u32; 2];
        for g in 0..2 {
            for (k, o) in AWQ_ORDER.iter().enumerate() {
                qz[g] |= z[g][*o] << (4 * k);
            }
        }
        let qw: Vec<i32> = qw.into_iter().map(|x| x as i32).collect();
        let qz: Vec<i32> = qz.into_iter().map(|x| x as i32).collect();
        let scales = Tensor::new(vec![2, out], (0..2 * out).map(|k| 0.25 * (k + 1) as f32).collect());
        let fmt = QFormat::Awq { bits: 4, group: gs };
        let w = unpack_awq(&fmt, (&[inp, 1], &qw), (&[2, 1], &qz), &scales).unwrap();
        for i in 0..inp {
            for o in 0..out {
                let g = i / gs;
                assert_eq!(w.value(o, i), scales.data[g * out + o] as f64 * (q[i][o] as f64 - z[g][o] as f64));
            }
        }
    }

    #[test]
    fn configs_are_read_strictly() {
        let gptq = serde_json::json!({"bits": 4, "group_size": 128, "desc_act": true, "sym": true, "quant_method": "gptq",
            "damp_percent": 0.01, "true_sequential": true, "use_exllama": true, "exllama_config": {"version": 1}});
        let c = parse_quant_config(&gptq, "LlamaForCausalLM", "llama").unwrap();
        assert_eq!(c.fmt, QFormat::Gptq { bits: 4, group: 128, desc_act: true, sym: true, v2: false });
        let awq = serde_json::json!({"bits": 4, "group_size": 64, "zero_point": true, "version": "gemm", "quant_method": "awq",
            "modules_to_not_convert": null});
        assert_eq!(parse_quant_config(&awq, "Qwen2ForCausalLM", "qwen2").unwrap().fmt, QFormat::Awq { bits: 4, group: 64 });
        for bad in [
            serde_json::json!({"quant_method": "bitsandbytes"}),
            serde_json::json!({"quant_method": "gptq", "bits": 3}),
            serde_json::json!({"quant_method": "gptq", "bits": 4, "checkpoint_format": "marlin"}),
            serde_json::json!({"quant_method": "gptq", "bits": 4, "mystery": 1}),
            serde_json::json!({"quant_method": "awq", "bits": 4, "version": "gemv"}),
        ] {
            assert!(matches!(parse_quant_config(&bad, "LlamaForCausalLM", "llama"), Err(LowerError::NotLowerable(_))), "{bad}");
        }
        assert!(parse_quant_config(&awq, "GPTNeoXForCausalLM", "gpt_neox").is_err());
    }
}
