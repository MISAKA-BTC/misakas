//! **Limitation L4, what this tree can do offline** (RFC-0002 Part II §II.9): the quantisation libraries (AutoGPTQ, AutoAWQ,
//! compressed-tensors, bitsandbytes) are not installed, so no checkpoint's *library-dequantised* output can be produced here. What the
//! in-repo fixtures (`tests/fixtures/hf-quant`, 33 checkpoints quantised in numpy/torch per each format's specification, with
//! `transformers`' logits over the dequantised weights beside them) give is a **frozen decode**: for every fixture, the digest of
//! every parameter the converter binds from it — the dequantised float tensor and, where the format stores integers, the stored codes,
//! scales, zero points and group index of each projection (and each expert) — pinned in `tests/golden/quant_decode_v1.json`.
//!
//! The pins do two jobs. A change to a descriptor, an unpacker or the f16/bf16 conversion that moves one byte of a decoded weight in
//! any of 33 fixtures fails here by fixture and tensor name (the end-to-end `quantized.rs` only sees it through logits tolerances). And
//! the day a real checkpoint from a real quantiser, with the library's dequantised tensors, is added to the corpus, its vectors are
//! compared to the same decoder these pins hold fixed. `UPDATE_PINS=1` rewrites the file (review the diff).

use blake2b_simd::Params;
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::lower::LowerOpts;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn digest(parts: &[&[u8]]) -> String {
    let mut h = Params::new().hash_length(16).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    h.finalize().to_hex().to_string()
}

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_bits().to_le_bytes()).collect()
}

/// `{ "<param>@<layer>": "<digest of the float tensor>", "<param>@<layer>#q<k>": "<digest of the k-th stored-integer weight>" }`
fn decode_of(name: &str) -> Result<BTreeMap<String, String>, String> {
    let dir = root().join("tests/fixtures/hf-quant").join(name);
    let (prep, ck) = fidelity::open_model(&dir, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let (store, _) = ParamStore::from_source(&prep.hl, &prep.binding, ck.as_ref()).map_err(|e| e.to_string())?;
    let mut out = BTreeMap::new();
    for (pi, decl) in prep.hl.params.iter().enumerate() {
        let layers: Vec<Option<usize>> = if decl.per_layer { (0..64).map(Some).collect() } else { vec![None] };
        let mut last: Option<String> = None;
        for layer in layers {
            let key = |suffix: &str| format!("{}@{}{suffix}", decl.name, layer.map_or("-".into(), |l| l.to_string()));
            let Ok(t) = store.get(pi as u32, layer) else { continue };
            let shape: Vec<u8> = t.shape.iter().flat_map(|d| (*d as u64).to_le_bytes()).collect();
            let d = digest(&[&shape, &f32s(&t.data)]);
            // A layer that reads the global tensor (`get` falls back) is one entry, not 64.
            if layer.is_some() && last.as_deref() == Some(d.as_str()) && !store_has_layered(&store, pi as u32, layer) {
                continue;
            }
            last = Some(d.clone());
            out.insert(key(""), d);
            if let Some(qs) = store.get_q(pi as u32, layer) {
                for (k, q) in qs.iter().enumerate() {
                    let shape: Vec<u8> = [q.out, q.inp, q.group, q.bits as usize, q.signed as usize].iter().flat_map(|d| (*d as u64).to_le_bytes()).collect();
                    let codes: Vec<u8> = q.q.iter().flat_map(|x| x.to_le_bytes()).collect();
                    let scale: Vec<u8> = q.scale.iter().flat_map(|x| x.to_bits().to_le_bytes()).collect();
                    let zero: Vec<u8> = q.zero.iter().flat_map(|x| x.to_le_bytes()).collect();
                    let min: Vec<u8> = q.min.iter().flatten().flat_map(|x| x.to_bits().to_le_bytes()).collect();
                    let gidx: Vec<u8> = q.gidx.iter().flat_map(|x| x.to_le_bytes()).collect();
                    out.insert(key(&format!("#q{k}")), digest(&[&shape, &codes, &scale, &zero, &min, &gidx]));
                }
            }
        }
    }
    Ok(out)
}

fn store_has_layered(store: &ParamStore, p: u32, layer: Option<usize>) -> bool {
    // A layered entry differs from the global one only when the two digests differ; `get` for `None` reads the global.
    match (store.get(p, None), store.get(p, layer)) {
        (Ok(g), Ok(l)) => !std::ptr::eq(g, l),
        _ => true,
    }
}

#[test]
fn every_quantised_fixture_decodes_to_its_pinned_weights() {
    let pins_path = root().join("tests/golden/quant_decode_v1.json");
    let mut names: Vec<String> = std::fs::read_dir(root().join("tests/fixtures/hf-quant"))
        .expect("fixtures")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect();
    names.sort();
    assert!(names.len() >= 33, "{} quantised fixtures", names.len());
    let mut now: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for n in &names {
        now.insert(n.clone(), decode_of(n).unwrap_or_else(|e| panic!("{n}: {e}")));
    }
    if std::env::var_os("UPDATE_PINS").is_some() {
        std::fs::write(&pins_path, serde_json::to_string_pretty(&now).expect("json") + "\n").expect("write the pins");
        return;
    }
    let pinned: BTreeMap<String, BTreeMap<String, String>> =
        serde_json::from_str(&std::fs::read_to_string(&pins_path).expect("tests/golden/quant_decode_v1.json (UPDATE_PINS=1 writes it)"))
            .expect("pins");
    for (fixture, tensors) in &now {
        let want = pinned.get(fixture).unwrap_or_else(|| panic!("{fixture}: no pins (UPDATE_PINS=1)"));
        for (k, d) in tensors {
            assert_eq!(want.get(k), Some(d), "{fixture}: `{k}` no longer decodes to its pinned bytes");
        }
        assert_eq!(want.len(), tensors.len(), "{fixture}: a tensor was added or dropped");
    }
    assert_eq!(pinned.len(), now.len(), "a fixture was added or dropped");
    // The vectors cover every format family the RFC names for L4.
    for family in ["gptq_", "awq_", "ct_", "bnb_", "fp8_", "mxfp4_"] {
        assert!(now.keys().any(|k| k.starts_with(family)), "no {family} fixture");
        assert!(now.iter().filter(|(k, _)| k.starts_with(family)).all(|(_, v)| !v.is_empty()), "{family}: a fixture with no decoded tensor");
    }
}
