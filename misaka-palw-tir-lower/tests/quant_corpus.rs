//! **The quant registry against an independent corpus.** `tools/gen_quant_formats.py --corpus FILE N`
//! writes N random valid blocks of every built-in type with the values gguf-py's numpy dequantisers
//! (and, for Q1_0 and Q2_0, a numpy transcription of the C) give them; this decodes each with the
//! descriptor interpreter and requires every `f32` to agree bit for bit. Not committed (13 MB at
//! N = 300): set `QUANT_CORPUS=<file>` to run it; the descriptors' own embedded vectors run always
//! (`quantfmt::tests`).

use misaka_palw_tir_lower::quantfmt::QuantRegistry;
use std::collections::BTreeMap;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn every_builtin_format_decodes_the_independent_corpus_bit_for_bit() {
    let Some(path) = std::env::var_os("QUANT_CORPUS") else {
        eprintln!("SKIPPED: set QUANT_CORPUS to a file written by tools/gen_quant_formats.py --corpus");
        return;
    };
    let text = std::fs::read_to_string(path).expect("corpus");
    let reg = QuantRegistry::builtin();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let name = v["format"].as_str().unwrap();
        let f = reg.named(name).unwrap_or_else(|| panic!("no descriptor for {name}"));
        let b = f.as_blocks().unwrap();
        let (raw, want) = (unhex(v["block_hex"].as_str().unwrap()), unhex(v["values_f32_hex"].as_str().unwrap()));
        let blocks = raw.len() / b.bytes;
        let got = b.decode_floats(&raw, 1, blocks * b.elems).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got.len() * 4, want.len(), "{name}");
        for (i, (g, w)) in got.iter().zip(want.chunks_exact(4)).enumerate() {
            let w = f32::from_le_bytes([w[0], w[1], w[2], w[3]]);
            // Bit for bit, a zero of either sign being one value.
            assert!(g.to_bits() == w.to_bits() || (*g == 0.0 && w == 0.0), "{name}: element {i} of block {}: {g:e} vs {w:e}", *counts.get(name).unwrap_or(&0));
        }
        *counts.entry(name.to_string()).or_default() += 1;
    }
    eprintln!("{} formats, {} blocks: {counts:?}", counts.len(), counts.values().sum::<usize>());
    assert!(counts.len() >= 28);
}
