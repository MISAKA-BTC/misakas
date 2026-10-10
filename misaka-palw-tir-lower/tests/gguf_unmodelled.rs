//! **A GGUF never lowers with a semantic it does not model** (COV-P1P2, 2026-10-08; the Mitsuba Qwen3.8-27B PTQ1_0 file).
//!
//! * A metadata namespace outside the architecture's own and provenance (`general`, `tokenizer`, `quantize`, `split`) is refused by
//!   name. The case that found it: `prism.hadamard.*`, a block-Hadamard rotation of the stored weights a format's writer declares
//!   beside them. The reader used to check only `{arch}.*` keys, so a descriptor for the weights' block type would have lowered the
//!   ROTATED weights as the model's — a silent semantic substitution.
//! * `{arch}.nextn_predict_layers` (llama.cpp counts the multi-token-prediction draft layers in `block_count`): the trailing blocks
//!   are DROPPED by name — the model's own logits never read them — and each must carry its `nextn.*` tensors.
//!
//! The file under test is the tiny `qwen35` fixture (`tests/fixtures/gguf/gguf_qwen35_q8_0`), rewritten here: metadata added, a
//! block appended.
use misaka_palw_tir_lower::LowerError;
use misaka_palw_tir_lower::gguf::{GgufFile, GgufModel};
use std::path::{Path, PathBuf};

/// One metadata value, as its type tag and its encoded bytes.
#[derive(Clone)]
struct Kv {
    key: String,
    ty: u32,
    raw: Vec<u8>,
}

struct Tensor {
    name: String,
    dims: Vec<u64>,
    ty: u32,
    offset: u64,
}

struct Gguf {
    version: u32,
    kvs: Vec<Kv>,
    tensors: Vec<Tensor>,
    data: Vec<u8>,
}

const ALIGN: usize = 32;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gguf/gguf_qwen35_q8_0/model.gguf")
}

struct Rd<'a>(&'a [u8], usize);

impl Rd<'_> {
    fn take(&mut self, n: usize) -> &[u8] {
        let s = &self.0[self.1..self.1 + n];
        self.1 += n;
        s
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take(8).try_into().unwrap())
    }
    fn string(&mut self) -> String {
        let n = self.u64() as usize;
        String::from_utf8(self.take(n).to_vec()).unwrap()
    }
    /// Skip one value of type `ty`, returning its bytes.
    fn value(&mut self, ty: u32) -> Vec<u8> {
        let start = self.1;
        match ty {
            0 | 1 | 7 => {
                self.take(1);
            }
            2 | 3 => {
                self.take(2);
            }
            4..=6 => {
                self.take(4);
            }
            10..=12 => {
                self.take(8);
            }
            8 => {
                let n = self.u64() as usize;
                self.take(n);
            }
            9 => {
                let et = self.u32();
                let n = self.u64();
                for _ in 0..n {
                    self.value(et);
                }
            }
            t => panic!("type {t}"),
        }
        self.0[start..self.1].to_vec()
    }
}

fn parse(bytes: &[u8]) -> Gguf {
    let mut r = Rd(bytes, 0);
    assert_eq!(r.take(4), b"GGUF");
    let version = r.u32();
    let (nt, nkv) = (r.u64(), r.u64());
    let kvs = (0..nkv)
        .map(|_| {
            let key = r.string();
            let ty = r.u32();
            let raw = r.value(ty);
            Kv { key, ty, raw }
        })
        .collect();
    let tensors = (0..nt)
        .map(|_| {
            let name = r.string();
            let nd = r.u32();
            let dims = (0..nd).map(|_| r.u64()).collect();
            let ty = r.u32();
            let offset = r.u64();
            Tensor { name, dims, ty, offset }
        })
        .collect();
    let start = r.1.div_ceil(ALIGN) * ALIGN;
    Gguf { version, kvs, tensors, data: bytes[start..].to_vec() }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend((s.len() as u64).to_le_bytes());
    out.extend(s.as_bytes());
}

impl Gguf {
    fn bytes(&self) -> Vec<u8> {
        let mut out = b"GGUF".to_vec();
        out.extend(self.version.to_le_bytes());
        out.extend((self.tensors.len() as u64).to_le_bytes());
        out.extend((self.kvs.len() as u64).to_le_bytes());
        for kv in &self.kvs {
            put_str(&mut out, &kv.key);
            out.extend(kv.ty.to_le_bytes());
            out.extend(&kv.raw);
        }
        for t in &self.tensors {
            put_str(&mut out, &t.name);
            out.extend((t.dims.len() as u32).to_le_bytes());
            for d in &t.dims {
                out.extend(d.to_le_bytes());
            }
            out.extend(t.ty.to_le_bytes());
            out.extend(t.offset.to_le_bytes());
        }
        out.resize(out.len().div_ceil(ALIGN) * ALIGN, 0);
        out.extend(&self.data);
        out
    }

    fn set(&mut self, key: &str, ty: u32, raw: Vec<u8>) {
        match self.kvs.iter_mut().find(|k| k.key == key) {
            Some(k) => {
                k.ty = ty;
                k.raw = raw;
            }
            None => self.kvs.push(Kv { key: key.into(), ty, raw }),
        }
    }

    /// An F32 tensor of `dims` appended to the data section (aligned).
    fn push_f32(&mut self, name: &str, dims: Vec<u64>) {
        self.data.resize(self.data.len().div_ceil(ALIGN) * ALIGN, 0);
        let n: u64 = dims.iter().product();
        let offset = self.data.len() as u64;
        self.data.extend((0..n).flat_map(|i| (i as f32 * 1e-3).to_le_bytes()));
        self.tensors.push(Tensor { name: name.into(), dims, ty: 0, offset });
    }

    fn model(&self) -> Result<GgufModel, LowerError> {
        let bytes = self.bytes();
        let file = GgufFile::parse(
            &bytes[..],
            Some(bytes.len() as u64),
            Path::new("rewritten.gguf"),
            misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin(),
        )?;
        GgufModel::from_file(file)
    }
}

fn s(v: &str) -> (u32, Vec<u8>) {
    let mut b = Vec::new();
    put_str(&mut b, v);
    (8, b)
}

fn u(v: u32) -> (u32, Vec<u8>) {
    (4, v.to_le_bytes().to_vec())
}

fn arr_u32(v: &[u32]) -> (u32, Vec<u8>) {
    let mut b = 4u32.to_le_bytes().to_vec();
    b.extend((v.len() as u64).to_le_bytes());
    b.extend(v.iter().flat_map(|x| x.to_le_bytes()));
    (9, b)
}

fn arr_str(v: &[&str]) -> (u32, Vec<u8>) {
    let mut b = 8u32.to_le_bytes().to_vec();
    b.extend((v.len() as u64).to_le_bytes());
    for x in v {
        put_str(&mut b, x);
    }
    (9, b)
}

fn base() -> Gguf {
    parse(&std::fs::read(fixture()).unwrap())
}

#[test]
fn the_fixture_maps_as_it_is() {
    let m = base().model().expect("the unmodified fixture maps");
    assert_eq!(m.config["num_hidden_layers"], 4);
    assert!(m.dropped().is_empty());
}

#[test]
fn a_weight_space_rotation_is_never_ignored() {
    // A declaration the reader cannot apply as the producer would (here: explicit signs with no sign values) is refused by name —
    // never lowered as if the stored weights were the model's. (A complete one is applied: `tests/weight_rotation.rs`.)
    let mut g = base();
    for (k, (ty, raw)) in [
        ("prism.hadamard.version", u(1)),
        ("prism.hadamard.block_size", u(32)),
        ("prism.hadamard.transform", s("normalized-sylvester-walsh-hadamard")),
        ("prism.hadamard.axis", s("input-last-dimension")),
        ("prism.hadamard.sign_mode", s("explicit")),
        ("prism.hadamard.weight_names", arr_str(&["blk.0.attn_qkv.weight"])),
        ("prism.hadamard.sign_widths", arr_u32(&[64, 128])),
        ("prism.hadamard.inverse_weight_names", arr_str(&["token_embd.weight"])),
        ("prism.hadamard.gdn_v_grouped", (7, vec![1])),
    ] {
        g.set(k, ty, raw);
    }
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("an incomplete rotation is never mapped as the model") };
    for needle in ["prism.hadamard", "sign width", "WEIGHT_ROTATION_HADAMARD_V1"] {
        assert!(msg.contains(needle), "`{needle}` not in: {msg}");
    }
    // Another key of the producer's namespace, without the declaration it belongs to, is refused as unmodelled.
    let mut g = base();
    let (ty, raw) = s("x");
    g.set("prism.something_else", ty, raw);
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("refused") };
    assert!(msg.contains("prism.something_else"), "{msg}");
    // A key of a later version of the declaration is refused, not skipped.
    let mut g = base();
    for (k, (ty, raw)) in [("prism.hadamard.version", u(2))] {
        g.set(k, ty, raw);
    }
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("refused") };
    assert!(msg.contains("version"), "{msg}");
}

#[test]
fn any_unmodelled_metadata_namespace_is_refused_and_provenance_is_not() {
    let mut g = base();
    let (ty, raw) = u(3);
    g.set("acme.weight_transform", ty, raw);
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("refused") };
    assert!(msg.contains("`acme.weight_transform`") && msg.contains("`acme.*`"), "{msg}");
    // Provenance namespaces are read as provenance.
    let mut g = base();
    let (ty, raw) = s("imatrix.dat");
    g.set("quantize.imatrix.file", ty, raw);
    g.model().expect("quantize.* is provenance");
}

/// **The inert provenance registry** (`GGUF_INERT_PROVENANCE_V1`): a quantiser's bookkeeping namespace, as the census found it in
/// 123 saved headers (`mradermacher.*`, every key and value type of the observed files), is ignored and recorded; the model is the
/// file's model without it. An unlisted key of a registered namespace, a table value in one, and an unregistered namespace are
/// refused by name.
#[test]
fn an_inert_provenance_namespace_is_ignored_by_name_and_nothing_else_is() {
    let plain = base().model().expect("the fixture maps");
    let mut g = base();
    for (k, v) in [
        ("mradermacher.quantize_version", "2"),
        ("mradermacher.quantized_by", "mradermacher"),
        ("mradermacher.quantized_at", "2025-02-23T04:13:46+01:00"),
        ("mradermacher.quantized_on", "nico1"),
        ("mradermacher.convert_type", "hf"),
    ] {
        let (ty, raw) = s(v);
        g.set(k, ty, raw);
    }
    let m = g.model().expect("a quantiser's bookkeeping is inert");
    assert_eq!(m.config, plain.config, "the same model as the file without its bookkeeping");
    assert_eq!(m.inert_keys().len(), 5, "{:?}", m.inert_keys());
    assert!(m.inert_keys().iter().all(|k| k.starts_with("mradermacher.")));
    assert_eq!(m.unmapped(), plain.unmapped());
    // The second registered namespace, as observed (free text).
    let mut g = base();
    for (k, v) in [
        ("duynt.quantized.by", "duyntnet"),
        ("duynt.greetings", "hello"),
        ("duynt.random.quote", "q"),
        ("duynt.quantization.date", "d"),
    ] {
        let (ty, raw) = s(v);
        g.set(k, ty, raw);
    }
    assert_eq!(g.model().expect("inert").inert_keys().len(), 4);
    // An unlisted key of a registered namespace is refused, naming the key and the registry.
    let mut g = base();
    let (ty, raw) = s("hf");
    g.set("mradermacher.convert_type", ty, raw);
    let (ty, raw) = u(1);
    g.set("mradermacher.weight_permutation", ty, raw);
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("an unlisted key is refused") };
    assert!(msg.contains("`mradermacher.weight_permutation`") && msg.contains("GGUF_INERT_PROVENANCE_V1"), "{msg}");
    // A listed key whose value is a table is refused.
    let mut g = base();
    let (ty, raw) = arr_str(&["a", "b"]);
    g.set("mradermacher.quantized_by", ty, raw);
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("a table is never inert") };
    assert!(msg.contains("array"), "{msg}");
    // A namespace that declares a reshape of the stored tensors stays refused, inert keys beside it or not.
    let mut g = base();
    let (ty, raw) = s("hf");
    g.set("mradermacher.convert_type", ty, raw);
    let (ty, raw) = arr_str(&["64", "128"]);
    g.set("comfy.gguf.orig_shape.token_embd.weight", ty, raw);
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("refused") };
    assert!(msg.contains("`comfy.*`"), "{msg}");
}

/// The registry never lists a namespace that declares something about the weights, and its identity is fixed by its rows.
#[test]
fn the_inert_registry_and_the_never_inert_list_are_disjoint() {
    use misaka_palw_tir_lower::gguf::{GGUF_INERT_NAMESPACES_V1, GGUF_NEVER_INERT_NAMESPACES_V1, gguf_inert_registry_digest_v1};
    for r in GGUF_INERT_NAMESPACES_V1 {
        assert!(
            !GGUF_NEVER_INERT_NAMESPACES_V1.iter().any(|(ns, _)| *ns == r.namespace),
            "`{}` declares something about the weights and is listed inert",
            r.namespace
        );
        assert!(!["general", "tokenizer", "quantize", "split", "prism"].contains(&r.namespace));
        assert!(!r.keys.is_empty() && !r.why.is_empty());
    }
    assert!(GGUF_NEVER_INERT_NAMESPACES_V1.iter().any(|(ns, _)| *ns == "prism"));
    assert_eq!(gguf_inert_registry_digest_v1().len(), 64);
}

#[test]
fn multi_token_prediction_layers_are_dropped_by_name() {
    let mut g = base();
    let (ty, raw) = u(5);
    g.set("qwen35.block_count", ty, raw);
    let (ty, raw) = u(1);
    g.set("qwen35.nextn_predict_layers", ty, raw);
    // Block 4 is the draft layer: the main model's layer shapes plus its `nextn.*` projections and norms.
    for (name, dims) in [
        ("blk.4.attn_norm.weight", vec![64]),
        ("blk.4.post_attention_norm.weight", vec![64]),
        ("blk.4.nextn.eh_proj.weight", vec![128, 64]),
        ("blk.4.nextn.enorm.weight", vec![64]),
        ("blk.4.nextn.hnorm.weight", vec![64]),
        ("blk.4.nextn.shared_head_norm.weight", vec![64]),
    ] {
        g.push_f32(name, dims);
    }
    let m = g.model().expect("the draft layer is dropped, the model maps");
    let plain = base().model().unwrap();
    assert_eq!(m.config, plain.config, "the same model as the file without its draft layer");
    assert_eq!(m.dropped().len(), 6);
    assert!(m.dropped().iter().all(|t| t.starts_with("blk.4.")));
    assert!(m.unmapped().iter().all(|t| !t.starts_with("blk.4.")), "dropped by design, not unread: {:?}", m.unmapped());
    // A trailing block that carries no `nextn.*` tensor is not a draft layer: refused.
    let mut g = base();
    let (ty, raw) = u(5);
    g.set("qwen35.block_count", ty, raw);
    let (ty, raw) = u(1);
    g.set("qwen35.nextn_predict_layers", ty, raw);
    g.push_f32("blk.4.attn_norm.weight", vec![64]);
    let Err(LowerError::NotLowerable(msg)) = g.model() else { panic!("refused") };
    assert!(msg.contains("no `nextn.*` tensor"), "{msg}");
}
