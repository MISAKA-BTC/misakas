//! **The built-in adapters' identities are pinned** (lane F, RFC-0002 Part II).
//!
//! A runtime pack names the adapter that read its model by `{id, hash}` (the hash of the effective adapter:
//! `misaka.palw.model-adapter.v1`'s identity). A hash that moves without notice breaks every pack that
//! pinned the old one: `palw-class pack verify` would report the adapter changed under it. So the hash of
//! every adapter the pack shipped with is recorded in `tests/golden/adapter_pins_v1.json`, and this test
//! fails when one changes. A change on purpose is listed in `INTENDED` with the commit that explains it
//! (the old packs then name the old text, which they carry or can fetch); a NEW adapter is recorded with
//! `PALW_PINS_UPDATE=1`, which refuses to touch an existing row that is not in `INTENDED`.

use misaka_palw_tir_lower::adapter::builtin;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `id → why`: adapters whose hash changed on purpose.
const INTENDED: &[(&str, &str)] = &[
    // The merge of `tir/generic` into the RFC-0002 line (rfc2/rest): the adapters moved with the frontend they are read by.
    (
        "qwen4-exp",
        "lane G (EMBED_NGRAM_PLE_V1, `generic-frontend-v1.md` §9.4): the n-gram table is one axis-0 `[rows, dim]` param per hash head (and chunk) — the adapter's weights expressions follow",
    ),
    (
        "mllama",
        "lane R2-A (FR-21, ATTN_CROSS_V1): the cross layers' tensors are read when the states are declared (their roles are named, and no longer ignored by prefix: the binder ignores them itself when the layers are skipped)",
    ),
    (
        "refusals",
        "ae21eeeec: Nemotron-H, Falcon-H1 and LFM2 lower as data now, so their refusals are deleted from the shared refusal table",
    ),
    // The census lane's key fixes of 2026-10-04 moved these after the last regeneration (455364f0d) without recording them here; the
    // test failed at `febc07f24` (the model-onboarding base) before any change of lane A. Recorded, with their commits.
    ("llama", "6b082bef7: Llama reads its positional-limit aliases as inert and accepts rope_interleaved / sliding_window only absent, false or null (census CONFIG_KEY_UNREAD)"),
    ("gpt2", "6b082bef7: GPT-2 accepts n_special only when 0 (census CONFIG_KEY_UNREAD); model-onboarding: its lm_head tensor name is the `lm_head_name` variable (the sequence-classification mixin renames it `score`)"),
    ("mixtral", "23f8a9feb: Mixtral reads attention_bias (absent or false only)"),
    ("phi3", "23f8a9feb: Phi-3 reads attention_bias (absent or false only); model-onboarding: its lm_head tensor name is the `lm_head_name` variable (the sequence-classification mixin renames it `score`)"),
    ("opt", "model-onboarding: OPT's lm_head tensor name is the `lm_head_name` variable (the sequence-classification mixin renames it `score`)"),
    ("mixin-vlm", "42faa5b55: a chat model's wrapper keys that shadow its decoder's are inert (`root_shadows_decoder`), the decoder's image token ids are inert"),
    ("vlm", "42faa5b55: as mixin-vlm (the wrapper adapter extends it)"),
    ("vlm-gemma", "42faa5b55 / 1472a300c: as mixin-vlm; the Gemma wrapper's own keys"),
    ("vlm-gemma2", "42faa5b55: as mixin-vlm"),
    ("vlm-gemma3", "42faa5b55: as mixin-vlm"),
    ("vlm-llama", "42faa5b55: as mixin-vlm"),
    ("vlm-llama4", "42faa5b55: as mixin-vlm"),
    ("vlm-mistral", "42faa5b55: as mixin-vlm"),
    ("vlm-qwen2", "42faa5b55: as mixin-vlm"),
    ("vlm-qwen2-vl", "42faa5b55: flat Qwen2/2.5-VL configurations dispatch to their decoder (`decoder_optional`)"),
    ("vlm-qwen3-5", "42faa5b55 / cc72d2424: as mixin-vlm; the Qwen3.5 vision tower"),
    ("vlm-qwen3-5-moe", "42faa5b55 / cc72d2424: as mixin-vlm; the Qwen3.5 vision tower"),
];

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/adapter_pins_v1.json")
}

fn load() -> BTreeMap<String, String> {
    match std::fs::read(golden_path()) {
        Ok(b) => serde_json::from_slice(&b).expect("adapter pins json"),
        Err(_) => BTreeMap::new(),
    }
}

#[test]
fn every_pinned_adapter_still_has_its_hash() {
    let now: BTreeMap<String, String> = builtin::pack_manifest().into_iter().collect();
    let mut pinned = load();
    if std::env::var_os("PALW_PINS_UPDATE").is_some() {
        for (id, h) in &now {
            match pinned.get(id) {
                Some(old) if old != h && !INTENDED.iter().any(|(n, _)| n == id) => {
                    panic!("{id}: the pinned hash would change ({old} → {h}); list it in INTENDED with the commit that explains it")
                }
                _ => {
                    pinned.insert(id.clone(), h.clone());
                }
            }
        }
        std::fs::create_dir_all(golden_path().parent().expect("dir")).expect("mkdir");
        std::fs::write(golden_path(), serde_json::to_string_pretty(&pinned).expect("json") + "\n").expect("write pins");
        eprintln!("pinned {} adapters in {}", pinned.len(), golden_path().display());
        return;
    }
    assert!(!pinned.is_empty(), "no pins at {}", golden_path().display());
    let mut moved = Vec::new();
    for (id, want) in &pinned {
        match now.get(id) {
            Some(h) if h == want => {}
            Some(h) if INTENDED.iter().any(|(n, _)| n == id) => eprintln!("{id}: changed on purpose ({h})"),
            Some(h) => moved.push(format!("{id}: pinned {} but is now {}", &want[..16], &h[..16])),
            None => moved.push(format!("{id}: pinned, and no longer in the built-in pack")),
        }
    }
    let unpinned: Vec<&String> = now.keys().filter(|k| !pinned.contains_key(*k)).collect();
    if !unpinned.is_empty() {
        eprintln!("{} adapter(s) not pinned yet (PALW_PINS_UPDATE=1 records them): {unpinned:?}", unpinned.len());
    }
    assert!(moved.is_empty(), "a pinned adapter's identity changed — every pack that names it would stop verifying:\n{}", moved.join("\n"));
}

/// The pack's hash is over the sorted `(id, hash)` pairs: the pins above make it a function of the files.
#[test]
fn the_pack_hash_is_the_hash_of_the_pinned_manifest() {
    let a = builtin::pack_hash();
    assert_eq!(a.len(), 128);
    assert_eq!(a, builtin::pack_hash(), "deterministic");
    let m = builtin::pack_manifest();
    assert!(m.windows(2).all(|w| w[0].0 < w[1].0), "sorted by id");
}
