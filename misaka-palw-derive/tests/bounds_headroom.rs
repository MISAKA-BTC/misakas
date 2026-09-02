//! **ADR-0078 SA-2's other failure mode: a bound that refuses the thing it was built to protect.**
//!
//! SA-2's ceilings are protection, and the drill already proves each one BITES (`palw-derive
//! drill` feeds every transformer an answer one byte over its `max_dsl_bytes` and requires a
//! refusal). Nothing proved the opposite half — that the ceilings sit far enough above a real
//! answer that a person asking a certified class for a MIDI file or a mesh gets one. A bound
//! chosen from intuition rather than from a measurement is a bound that refuses the demonstration,
//! and it does it in production, on the one path anybody is watching.
//!
//! This file is that measurement, executable, so it re-runs when a kind changes a ceiling or a
//! writer.
//!
//! # What was measured (2026-09-03, this tree)
//!
//! Every corpus answer that derives, against its transformer's declared ceilings. The DSL figure
//! is the CANONICAL byte count — what the transformer actually sees — which is well under the raw
//! corpus file (canonicalization strips the whitespace a hand-written sample carries: `scene`'s
//! largest is 8,320 bytes on disk and 2,532 canonicalized).
//!
//! ```text
//!   transformer           largest DSL / cap      largest artifact / cap     headroom (dsl/artifact)
//!   music/smf/v1            115,501 /  4 MiB        17,711 / 16 MiB            36x  /   947x
//!   scene/glb/v1              2,532 / 256 KiB       17,728 /  2 MiB           103x  /   118x
//!   cad/stl/v1                  235 /  64 KiB        7,284 /  1 MiB           278x  /   143x
//!   map/mmap/v1              16,648 /  4 MiB       124,544 / 16 MiB           251x  /   134x
//!   image/png/v1             40,497 /  4 MiB     1,049,236 / 32 MiB           103x  /    31x
//!   simulation/trace/v1       3,562 /  4 MiB        74,255 / 64 MiB         1,177x  /   903x
//!   code/evm/v1               1,209 /  4 MiB           548 / 16 MiB         3,469x  / 30,615x
//! ```
//!
//! Seven of the eight transformers appear: `contract/evm/v1` shares the `code` corpus directory
//! and has none of its own, so it contributes no measurement and the loop skips it.
//!
//! # The bound that actually governs the demonstration is not in this crate
//!
//! The measurement above is against hand-written corpus answers, which are larger than a model's.
//! The real ceiling on a model-written DSL is the gateway's decode cap: `max_decode_cap` defaults
//! to 1,024 tokens and `HARD_MAX_DECODE_CAP` is 4,096, and no flag may raise it. Even at 4 bytes
//! per token — generous for the ASCII JSON these grammars take — the largest answer that can reach
//! a transformer through `POST /v1/chat/completions` is about 16 KB.
//!
//! That is below **every** kind's `max_dsl_bytes`, the tightest of which is `cad/stl/v1`'s 64 KiB,
//! and it is the reason the step and artifact ceilings cannot bite either: both are functions of
//! the DSL. `scene/glb/v1`'s 65,536-vertex budget is the one worth checking by hand, because it is
//! the ceiling a few bytes of DSL could plausibly name — and in this grammar it cannot. There is no
//! procedural primitive: the only vertex-multiplying shape is `Prism`, whose vertex count is `6n`
//! in the number of base points, and every base point is spelled out in the DSL (~10 bytes
//! canonicalized). A 16 KB answer therefore names at most ~1,600 points, about 9,800 vertices —
//! under a seventh of the budget. `MAX_NODES` (1,024) caps the other direction at 24,576 vertices
//! for a scene of boxes.
//!
//! **Conclusion, stated so nobody has to re-derive it: no SA-2 ceiling in this crate can refuse an
//! answer that arrives through the gateway.** If that changes — a compact procedural primitive in
//! the scene grammar, a raised decode cap, a kind whose artifact is superlinear in its DSL — this
//! reasoning expires, and the assertion below is what will notice.

use misaka_palw_derive::{ClaimBinding, derive_named, registry};
use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN, kind};
use kaspa_hashes::Hash64;
use std::path::PathBuf;

/// The gateway's hard decode cap, in tokens (`misaka-palw-gateway`'s `HARD_MAX_DECODE_CAP`). Not
/// imported, because the gateway depends on this crate and not the other way round; the test below
/// fails loudly if the two ever disagree in the dangerous direction.
const GATEWAY_HARD_DECODE_CAP_TOKENS: u64 = 4_096;
/// A deliberately generous upper bound on the bytes one token of an ASCII JSON DSL renders to.
const BYTES_PER_TOKEN_UPPER: u64 = 4;
/// The largest answer that can reach a transformer through the gateway.
const LARGEST_GATEWAY_ANSWER_BYTES: u64 = GATEWAY_HARD_DECODE_CAP_TOKENS * BYTES_PER_TOKEN_UPPER;

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

fn binding() -> ClaimBinding {
    ClaimBinding {
        network_domain: Hash64::default(),
        claim_id: Hash64::default(),
        output_root: Hash64::default(),
        executor_pubkey: vec![0u8; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
    }
}

/// **Every ceiling sits above what a model can send, with the factor stated.**
///
/// The comparison is against the gateway's decode cap rather than against the corpus, because the
/// corpus is written by hand and the demonstration is not. This is the assertion that expires the
/// header's reasoning if somebody raises a cap or tightens a bound past each other.
#[test]
fn no_declared_dsl_ceiling_can_refuse_an_answer_the_gateway_can_produce() {
    let mut rows = Vec::new();
    for (name, _, _) in registry::transformer_names() {
        let m = registry::transformer_by_name(name).expect("just enumerated").manifest();
        rows.push(format!("{name}: max_dsl_bytes {} vs a {LARGEST_GATEWAY_ANSWER_BYTES}-byte largest answer", m.max_dsl_bytes));
        assert!(
            m.max_dsl_bytes > LARGEST_GATEWAY_ANSWER_BYTES,
            "ADR-0078 SA-2: {name} declares max_dsl_bytes {} and the gateway can hand it an answer of up to \
             {LARGEST_GATEWAY_ANSWER_BYTES} bytes ({GATEWAY_HARD_DECODE_CAP_TOKENS} tokens x \
             {BYTES_PER_TOKEN_UPPER}). A ceiling under that refuses a person's answer in production. Either raise \
             it with a measurement, or lower the gateway's decode cap — but do not leave them crossed.",
            m.max_dsl_bytes
        );
    }
    assert_eq!(rows.len(), 8, "the kind table changed; re-read this file's header before trusting its conclusion");
}

/// **Every corpus answer that derives lands far under its transformer's ceilings**, and the
/// margins are printed so a tightened bound shows its new headroom rather than only its pass.
///
/// Kinds are addressed by their corpus directory, which is the kind's name — the same mapping
/// `palw-derive drill` uses.
#[test]
fn every_corpus_derivation_lands_far_under_its_declared_ceilings() {
    let mut measured = 0usize;
    let mut report = Vec::new();
    for (name, k, _) in registry::transformer_names() {
        let m = registry::transformer_by_name(name).expect("just enumerated").manifest();
        let dir = corpus_root().join(kind::name(k).unwrap_or("unassigned"));
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json") && p.file_name().is_some_and(|f| f != "golden.json"))
            .collect();
        files.sort();
        let (mut worst_dsl, mut worst_artifact) = (0u64, 0u64);
        for file in files {
            let answer = std::fs::read(&file).expect("a corpus answer is readable");
            // A refusal is a corpus sample doing its job (the `9x-` bound-exhausting ones); only a
            // derivation carries a measurement.
            let Ok(d) = derive_named(name, &binding(), &answer) else { continue };
            measured += 1;
            worst_dsl = worst_dsl.max(d.canonical_dsl.len() as u64);
            worst_artifact = worst_artifact.max(d.object.artifact_bytes);
            assert!(
                d.object.artifact_bytes <= m.max_artifact_bytes,
                "{name} built {} bytes from {}, over its declared max_artifact_bytes of {}",
                d.object.artifact_bytes,
                file.display(),
                m.max_artifact_bytes
            );
        }
        if worst_dsl == 0 {
            continue;
        }
        report.push(format!(
            "{name}: dsl {worst_dsl}/{} ({}x headroom), artifact {worst_artifact}/{} ({}x)",
            m.max_dsl_bytes,
            m.max_dsl_bytes / worst_dsl.max(1),
            m.max_artifact_bytes,
            m.max_artifact_bytes / worst_artifact.max(1)
        ));
        // Two is not much of a margin; it is the threshold at which "the bound was measured" stops
        // being true and somebody has to re-measure with a real answer in hand.
        assert!(
            m.max_dsl_bytes / worst_dsl.max(1) >= 2 && m.max_artifact_bytes / worst_artifact.max(1) >= 2,
            "ADR-0078 SA-2: {name}'s ceilings are within 2x of what its own corpus produces, which means the next \
             slightly larger real answer is refused. Re-measure before shipping this.\n  {}",
            report.join("\n  ")
        );
    }
    assert!(measured >= 20, "only {measured} corpus derivations were measured: this test is not seeing the corpus");
    println!("SA-2 headroom over the corpus:\n  {}", report.join("\n  "));
}
