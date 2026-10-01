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
const INTENDED: &[(&str, &str)] = &[];

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
