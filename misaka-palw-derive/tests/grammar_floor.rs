//! **The floor of each shipped grammar: the SMALLEST canonical answer that still makes a
//! non-degenerate artifact.**
//!
//! `bounds_headroom.rs` measured the other end — that no SA-2 ceiling refuses a big answer. This
//! file measures the end that decides whether the public demonstration is possible at all: the
//! registered classes admit 8 or 16 context positions (`palw_qwen36_profile::QWEN36_*` n_ctx 9 /
//! footprint 8, `palw_qwen25_profile::QWEN25_1_5B_A16` n_ctx 16), and a class cannot emit a DSL
//! it has no positions to spell. A grammar whose shortest legal sentence is a hundred tokens is
//! not adjudicable at those widths no matter what the court does.
//!
//! # Method, stated so every number below has one
//!
//! For each kind a hand-written seed answer is canonicalized by the SHIPPED grammar
//! (`derive_named` -> `Derivation::canonical_dsl`), and then MECHANICALLY SHRUNK: every
//! single-byte deletion is tried, and a deletion is accepted when the result
//!
//!   1. still derives through the shipped transformer,
//!   2. is still its own canonical form (so the measurement is of canonical bytes, which is what
//!      `dsl_hash` covers and what a verifier re-derives), and
//!   3. still yields a NON-DEGENERATE artifact, checked on the artifact itself:
//!      * `music`  — the Standard MIDI File is walked and must carry a note-on with velocity != 0;
//!      * `cad`    — the mesh's exact `six_times_volume` (i128, the shipped predicate) is != 0;
//!      * `scene`  — the built mesh's exact 6x signed volume (i128 over the shipped `Mesh`) is != 0.
//!
//! The loop runs to a fixed point, so the result is a string from which NO single byte can be
//! removed. Because every one of these grammars uses `exact_keys` (no defaults, no optional
//! fields) and `canon_json` (sorted keys, no whitespace, integers only), a 1-deletion-minimal
//! canonical string is also the grammar's global minimum: the key set is fixed, the punctuation
//! is fixed, and the only remaining freedom is the width of each literal, which single-byte
//! deletion explores exactly.
//!
//! Token counts are the same canonical bytes through the SHIPPED Qwen byte-level BPE
//! (`misaka_palw_base0::tokenizer::QwenTokenizer::encode_without_specials`) — the tokenizer the
//! free-prompt workers use — loaded from a checkpoint's `tokenizer.json` named by an environment
//! variable, because the checkpoints are not in the repository. With none set the byte floors are
//! still pinned and the token columns are skipped rather than guessed.
//!
//! # What this measured on 2026-09-03, in this tree
//!
//! ```text
//!   kind    smallest corpus file    canonical floor            floor in tokens
//!                (raw / canonical)                        (both Qwen BPEs agree)
//!   music     277 B / 184 B (73 tok)     169 bytes                60
//!   cad       178 B /  87 B (38 tok)      84 bytes                38
//!   scene     740 B / 325 B (135 tok)    281 bytes               104
//! ```
//!
//! The "277 bytes" and "178 bytes" that circulate are the RAW corpus files, which are
//! pretty-printed; canonicalization removes about a third, and the grammar's own floor is lower
//! again. Neither number is what decides anything — the token count is.
//!
//! # The budget those tokens are spent against
//!
//! A class's context is shared between the prompt and the answer. `fp_worker::run_one_job_v1`
//! refuses a job unless `prefill + decode_token_limit <= max_context_tokens <= n_ctx`, and
//! `PalwStepFaultV1::JobExceedsClassContext` makes an oversized commitment a convictable fault, so
//! the decode budget is `n_ctx - prefill` and nothing widens it. The Qwen chat template costs 8
//! tokens before a single byte of the user's request (measured: `qwen_chat_prompt(None,
//! [("user", "")])` encodes to 8 ids), so:
//!
//! ```text
//!   n_ctx   9 (QWEN36 registered)  prefill >= 9   decode budget 0
//!   n_ctx  16 (QWEN25 A16 registered)             decode budget 7 with a one-token request, 4 with a four-word one
//!   n_ctx  30 (widest the court carrier admits)   decode budget 21 / 18
//!   n_ctx 512 (hypothetical, ADR-0080)            decode budget ~500, about 1,200 bytes at the
//!                                                 2.4 bytes/token these grammars measure
//! ```
//!
//! Against 21 decode tokens — the most generous width anyone has proposed short of ADR-0080 — the
//! smallest of the three floors is 38 tokens. The floors are pinned below so that a grammar edit
//! which changes them has to come here and say so.

use kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN;
use kaspa_hashes::Hash64;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use misaka_palw_derive::kinds::{cad, scene};
use misaka_palw_derive::{ClaimBinding, derive_named};

fn binding() -> ClaimBinding {
    ClaimBinding {
        network_domain: Hash64::default(),
        claim_id: Hash64::default(),
        output_root: Hash64::default(),
        executor_pubkey: vec![0u8; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
    }
}

// ---------------------------------------------------------------------------------------------
// Non-degeneracy, checked on the artifact and not on the answer.
// ---------------------------------------------------------------------------------------------

/// A minimal Standard MIDI File walk: chunks, then per `MTrk` the delta/status stream the writer
/// documents (no running status, meta events length-prefixed). Returns true when some note-on
/// carries a non-zero velocity — the one thing that makes a MIDI file audible.
fn midi_has_audible_note(bytes: &[u8]) -> bool {
    let mut at = 0usize;
    while at + 8 <= bytes.len() {
        let tag = &bytes[at..at + 4];
        let len = u32::from_be_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body_at = at + 8;
        if body_at + len > bytes.len() {
            return false;
        }
        if tag == b"MTrk" {
            let body = &bytes[body_at..body_at + len];
            let mut i = 0usize;
            while i < body.len() {
                // delta time (variable-length quantity)
                while i < body.len() && body[i] & 0x80 != 0 {
                    i += 1;
                }
                i += 1;
                if i >= body.len() {
                    break;
                }
                let status = body[i];
                i += 1;
                match status {
                    0xFF => {
                        if i >= body.len() {
                            break;
                        }
                        i += 1; // meta type
                        let mut n = 0usize;
                        while i < body.len() && body[i] & 0x80 != 0 {
                            n = (n << 7) | (body[i] & 0x7F) as usize;
                            i += 1;
                        }
                        if i >= body.len() {
                            break;
                        }
                        n = (n << 7) | (body[i] & 0x7F) as usize;
                        i += 1 + n;
                    }
                    0x80..=0x8F | 0xA0..=0xBF | 0xE0..=0xEF => i += 2,
                    0xC0..=0xDF => i += 1,
                    0x90..=0x9F => {
                        if i + 1 < body.len() && body[i + 1] != 0 {
                            return true;
                        }
                        i += 2;
                    }
                    _ => break,
                }
            }
        }
        at = body_at + len;
    }
    false
}

/// The `cad` artifact is non-degenerate when the shipped exact volume predicate says the solid
/// encloses something. The mesh is rebuilt with the shipped pipeline rather than re-parsed out of
/// the STL, so the check is on the transformer's own geometry in exact integers.
fn cad_has_volume(canonical_dsl: &[u8]) -> bool {
    let Ok(model) = cad::canonical_model(canonical_dsl) else { return false };
    let Ok(raw) = cad::mesh(&model) else { return false };
    let Ok(tris) = cad::canonical_mesh(raw) else { return false };
    cad::six_times_volume(&tris) != 0
}

/// The `scene` artifact is non-degenerate when some built mesh encloses a non-zero volume. Six
/// times the signed volume of a closed triangle soup is `sum (v0 . (v1 x v2))`, taken in `i128`
/// over the mesh's own fixed-point positions, so no rounding enters the check.
fn scene_has_volume(canonical_dsl: &[u8]) -> bool {
    let Ok(dsl) = scene::canonical_scene(canonical_dsl) else { return false };
    let Ok(built) = scene::build_scene(&dsl) else { return false };
    built.meshes.iter().any(|m| {
        let p = &m.mesh.positions;
        let mut six = 0i128;
        for tri in m.mesh.indices.as_chunks::<3>().0 {
            let (a, b, c) = (p[tri[0] as usize], p[tri[1] as usize], p[tri[2] as usize]);
            let (a, b, c) = (a.map(i128::from), b.map(i128::from), c.map(i128::from));
            let cross = [b[1] * c[2] - b[2] * c[1], b[2] * c[0] - b[0] * c[2], b[0] * c[1] - b[1] * c[0]];
            six += a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
        }
        six != 0
    })
}

// ---------------------------------------------------------------------------------------------
// The shrinker.
// ---------------------------------------------------------------------------------------------

/// Does `candidate` derive to a canonical, non-degenerate artifact under `transformer`?
fn admissible(transformer: &str, candidate: &[u8], nondegenerate: fn(&[u8], &[u8]) -> bool) -> bool {
    let Ok(d) = derive_named(transformer, &binding(), candidate) else { return false };
    d.canonical_dsl == candidate && nondegenerate(&d.canonical_dsl, &d.artifact.bytes)
}

/// Shrink to a fixed point under single-byte deletion. The returned string derives, is canonical,
/// is non-degenerate, and no one byte can be removed from it while keeping all three.
fn shrink(transformer: &str, seed: &str, nondegenerate: fn(&[u8], &[u8]) -> bool) -> Vec<u8> {
    let d = derive_named(transformer, &binding(), seed.as_bytes())
        .unwrap_or_else(|e| panic!("{transformer}: the seed answer does not derive: {e}"));
    let mut best = d.canonical_dsl;
    assert!(
        admissible(transformer, &best, nondegenerate),
        "{transformer}: the seed's canonical form is not admissible; the seed is wrong, not the grammar"
    );
    loop {
        let mut improved = None;
        for cut in 0..best.len() {
            let mut trial = best.clone();
            trial.remove(cut);
            if admissible(transformer, &trial, nondegenerate) {
                improved = Some(trial);
                break;
            }
        }
        match improved {
            Some(next) => best = next,
            None => return best,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tokenization with the shipped tokenizer, when a checkpoint is on this host.
// ---------------------------------------------------------------------------------------------

/// `(label, tokenizer)` for every `tokenizer.json` named in the environment.
fn tokenizers() -> Vec<(String, QwenTokenizer)> {
    [("dense/qwen2.5", "MISAKA_FLOOR_TOKENIZER_DENSE"), ("moe/qwen3.6", "MISAKA_FLOOR_TOKENIZER_MOE")]
        .iter()
        .filter_map(|(label, var)| {
            let path = std::env::var(var).ok()?;
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{var}={path}: {e}"));
            let t = QwenTokenizer::from_json(&bytes).unwrap_or_else(|e| panic!("{var}={path}: {e}"));
            Some((format!("{label} ({} tokens)", t.len()), t))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The floors.
// ---------------------------------------------------------------------------------------------

/// One kind's measurement: the transformer, a seed answer, the artifact-side non-degeneracy check,
/// and the two numbers this file exists to pin.
struct Case {
    transformer: &'static str,
    seed: &'static str,
    nondegenerate: fn(&[u8], &[u8]) -> bool,
    /// The exact canonical bytes of the shortest legal non-degenerate answer, pinned in full so a
    /// reader can see the sentence a class would have to emit rather than only its length.
    floor: &'static str,
    /// Its length under the Qwen byte-level BPE. Both checkpoints measured (Qwen2.5-1.5B's
    /// 151,665-entry vocabulary and Qwen3.6's 248,077-entry one) give the same number for these
    /// ASCII strings; the tokenizer test below re-derives it whenever a checkpoint is on the host.
    floor_tokens: usize,
}

/// Seeds are hand-written at what the schema demands and nothing more; the shrinker takes them
/// the rest of the way, so a seed that is merely *close* to minimal is enough. In the event none
/// of them was reducible by even one byte: these grammars all use `exact_keys` (no defaults, no
/// optional fields) over `canon_json` (sorted keys, no whitespace), so the shortest legal sentence
/// is fully determined by the schema, and the shrinker's job here is to PROVE that rather than to
/// discover it.
const CASES: [Case; 3] = [
    Case {
        transformer: "music/smf/v1",
        seed: r#"{"ppq":96,"tempo_us_per_quarter":1,"time_signature":[1,1],
            "tracks":[{"channel":0,"name":"","notes":[{"duration":1,"onset":0,"pitch":0,"velocity":1}],"program":0}],"v":1}"#,
        nondegenerate: |_dsl, artifact| midi_has_audible_note(artifact),
        floor: r#"{"ppq":96,"tempo_us_per_quarter":1,"time_signature":[1,1],"tracks":[{"channel":0,"name":"","notes":[{"duration":1,"onset":0,"pitch":0,"velocity":1}],"program":0}],"v":1}"#,
        floor_tokens: 60,
    },
    Case {
        transformer: "cad/stl/v1",
        seed: r#"{"frac_bits":0,"sketches":{},"solid":{"max":[1,1,1],"min":[0,0,0],"op":"box"},"v":1}"#,
        nondegenerate: |dsl, _artifact| cad_has_volume(dsl),
        floor: r#"{"frac_bits":0,"sketches":{},"solid":{"max":[1,1,1],"min":[0,0,0],"op":"box"},"v":1}"#,
        floor_tokens: 38,
    },
    Case {
        transformer: "scene/glb/v1",
        seed: r#"{"frac_bits":0,
            "materials":[{"base_color":[0,0,0,0],"double_sided":false,"metallic":0,"name":"m","roughness":0}],
            "nodes":[{"children":[],"material":"m","name":"","rotation":[0,0,0,2],"scale":[1,1,1],
                      "shape":{"max":[1,1,1],"min":[0,0,0],"shape":"box"},"translation":[0,0,0]}],"v":1}"#,
        nondegenerate: |dsl, _artifact| scene_has_volume(dsl),
        floor: r#"{"frac_bits":0,"materials":[{"base_color":[0,0,0,0],"double_sided":false,"metallic":0,"name":"m","roughness":0}],"nodes":[{"children":[],"material":"m","name":"","rotation":[0,0,0,2],"scale":[1,1,1],"shape":{"max":[1,1,1],"min":[0,0,0],"shape":"box"},"translation":[0,0,0]}],"v":1}"#,
        floor_tokens: 104,
    },
];

/// **The measured floor of every grammar a public demonstration would use**, pinned so that a
/// grammar edit which changes what a model must emit cannot land silently.
///
/// The assertion is on the exact canonical bytes rather than only their length, because the point
/// of the number is that somebody can read the sentence a class would have to produce.
#[test]
fn the_shortest_legal_non_degenerate_answer_of_each_grammar_is_pinned() {
    for case in &CASES {
        let floor = shrink(case.transformer, case.seed, case.nondegenerate);
        let text = String::from_utf8(floor).expect("a canonical DSL is UTF-8");
        println!("{}: {} bytes / {} tokens\n    {text}", case.transformer, text.len(), case.floor_tokens);
        assert_eq!(
            text, case.floor,
            "\n{}'s shortest legal non-degenerate answer moved.\n  measured: {text}\n  pinned:   {}\n\
             If a grammar changed on purpose, update the pin AND this file's header table — this number is what \
             decides whether a class with 8 or 16 context positions can emit this kind at all.",
            case.transformer, case.floor
        );
    }
}

/// **The pinned token counts are what the shipped tokenizer actually produces.**
///
/// Skipped, loudly, when no checkpoint is on the host — the checkpoints are not in the repository,
/// and a test that silently passed without one would leave the deciding number unverified.
#[test]
fn the_pinned_token_counts_are_the_shipped_tokenizers_own() {
    let tokenizers = tokenizers();
    if tokenizers.is_empty() {
        println!(
            "SKIPPED: set MISAKA_FLOOR_TOKENIZER_DENSE and/or MISAKA_FLOOR_TOKENIZER_MOE to a checkpoint's \
             tokenizer.json to re-verify the token floors. The byte floors are pinned regardless."
        );
        return;
    }
    for case in &CASES {
        for (label, t) in &tokenizers {
            let ids = t.encode_without_specials(case.floor).expect("a canonical ASCII DSL encodes");
            assert_eq!(
                ids.len(),
                case.floor_tokens,
                "{}'s floor is {} tokens under {label}, not the pinned {}. The token count is the number that \
                 decides whether a class can emit this kind; re-pin it and revisit the header.",
                case.transformer,
                ids.len(),
                case.floor_tokens
            );
            println!("{} under {label}: {} tokens", case.transformer, ids.len());
        }
    }
}

/// **No context width anyone has proposed short of ADR-0080 can spell any of these floors.**
///
/// The comparison is against the DECODE budget and not against `n_ctx`, because the prompt is
/// spent from the same context (`fp_worker::run_one_job_v1`: `prefill + decode_token_limit <=
/// max_context_tokens <= n_ctx`). The budget used here is the most generous one that exists: the
/// widest row the court carrier admits, minus the chat template's own 8 tokens, minus one token
/// for a request that is a single character. A real request costs more and the budget is smaller.
///
/// Stated as an assertion rather than a comment so that the day a grammar gets small enough — or a
/// class wide enough — to make the demonstration possible, this test is what says so.
#[test]
fn no_context_width_short_of_adr_0080_can_spell_any_grammars_floor() {
    /// The widest context any class registered on testnet-11 declares
    /// (`palw_qwen25_profile::QWEN25_1_5B_A16`). The MoE class is narrower: `n_ctx` 9.
    const WIDEST_REGISTERED_N_CTX: usize = 16;
    /// The widest dense row the 81,920-byte court carrier admits, measured elsewhere in this batch.
    const WIDEST_CARRIER_ADMISSIBLE_N_CTX: usize = 30;
    /// `qwen_chat_prompt(None, &[("user", "")])`, encoded: the template's own cost before a byte of
    /// the request. Measured, not assumed.
    const CHAT_TEMPLATE_TOKENS: usize = 8;
    /// A request cannot be empty (`prompt_ids_for_input_v1` refuses zero bytes), so one token is
    /// the floor of a request.
    const SHORTEST_POSSIBLE_REQUEST_TOKENS: usize = 1;

    let budget = WIDEST_CARRIER_ADMISSIBLE_N_CTX - CHAT_TEMPLATE_TOKENS - SHORTEST_POSSIBLE_REQUEST_TOKENS;
    for case in &CASES {
        assert!(
            case.floor_tokens > budget,
            "{}'s {}-token floor now fits in the {budget}-token decode budget of the \
             {WIDEST_CARRIER_ADMISSIBLE_N_CTX}-position dense row the court carrier admits (registered classes are \
             narrower still, at {WIDEST_REGISTERED_N_CTX} and 9). That is a CHANGE IN THE ANSWER: the demonstration \
             this crate exists for may now be reachable without widening a class, and the launch owner has to be told.",
            case.transformer,
            case.floor_tokens
        );
    }
    println!(
        "decode budget at the widest admissible width: {budget} tokens; floors are {}",
        CASES.iter().map(|c| format!("{} {}", c.transformer, c.floor_tokens)).collect::<Vec<_>>().join(", ")
    );
}
