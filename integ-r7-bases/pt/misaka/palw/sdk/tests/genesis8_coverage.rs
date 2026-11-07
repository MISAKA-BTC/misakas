//! **Genesis 8 — every TIR-route genesis model lowers from its REAL headers and every cell boundary is covered** (RFC-0006
//! `palw_tir_cell_coverage_v1` under K2-TIR-v4, the real-scale descriptor).
//!
//! The fixtures are the pinned revisions' `config.json` and safetensors HEADERS only (no weight byte;
//! `scripts/genesis/genesis_header_fixtures.py`). Each text decoder is lowered shape-only at an 8,192-position context the way the
//! SDK preflight lowers a repository, then the coverage checker walks every relation, routing, carry, state and the decode door: an
//! uncovered or unknown boundary is listed by name. #1 (A16) and #8 (the existing official-derived PALWQ361 runtime) ship on the
//! legacy routes, whose step court is checked by `palw_legacy_cell_coverage_v{1,2}`; #6 is a GGUF (the same qwen35 hybrid program as
//! #2/#3, read from the GGUF). Scope: text generation only — vision towers, projectors and MTP heads are never lowered.
//!
//! Written, not run here: the validation lane compiles and runs it.

use kaspa_consensus_core::palw_cell_coverage_v1::palw_tir_cell_coverage_v1;
use misaka_palw_kernel::descriptor::k2_tir_v4_descriptor;
use misaka_palw_sdk::preflight::{Options, model, source};
use misaka_palw_sdk::tir_layout::tir_program_with_scheme_v1;
use misaka_palw_tir::TirProgramV1;
use std::path::PathBuf;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tir/lower/tests/fixtures").join(rel)
}

/// The text decoder lowered shape-only from the headers at `ctx` positions, under the tiled logits scheme.
fn program(dir: &PathBuf, ctx: u32) -> Result<TirProgramV1, String> {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let src = source::open(dir, source::detect(dir)?, None, reg)?;
    let analysis = model::analyze(&src, &Options { max_context: Some(ctx), ..Options::default() }, reg, None);
    let program = analysis
        .program
        .ok_or_else(|| format!("no program: {:?}", analysis.blockers.iter().map(|b| b.code.clone()).collect::<Vec<_>>()))?;
    tir_program_with_scheme_v1(&program, None).map_err(|e| format!("the tiled scheme: {e}"))
}

const TIR_ROUTE: [(&str, &str); 5] = [
    ("#2 Huihui-Qwen3.5-2B (GDN hybrid)", "genesis8/huihui-qwen35-2b"),
    ("#3 Huihui-Qwen3.5-9B (GDN hybrid)", "vlm-generic/huihui-qwen3.5-9b"),
    ("#4 Huihui-gemma-4-12B-it (Gemma-4 Unified)", "genesis8/huihui-gemma4-12b"),
    ("#5 Huihui-gpt-oss-20b BF16 (MoE, sinks)", "genesis8/huihui-gpt-oss-20b"),
    ("#10 Llama-3.3-70B-Instruct-abliterated (dense)", "genesis8/llama33-70b-abliterated"),
];

#[test]
fn every_tir_route_genesis_model_lowers_from_its_real_headers_and_every_cell_is_covered() {
    let d = k2_tir_v4_descriptor();
    let mut failures = Vec::new();
    for (genesis, rel) in TIR_ROUTE {
        let p = match program(&fixture(rel), 8192) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{genesis}: does not lower: {e}"));
                continue;
            }
        };
        match palw_tir_cell_coverage_v1(&d, &p, None) {
            Ok(covered) => eprintln!(
                "{genesis}: {} blocks, {} params, {} states — {} boundaries covered",
                p.blocks.len(),
                p.params.len(),
                p.states.len(),
                covered.len()
            ),
            Err(gaps) => failures.push(format!("{genesis}: {} uncovered: {gaps:?}", gaps.len())),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_gdn_hybrids_recurrence_is_a_checked_state_not_an_uncommitted_carry() {
    for (genesis, rel) in TIR_ROUTE.iter().take(2) {
        let p = program(&fixture(rel), 8192).unwrap_or_else(|e| panic!("{genesis}: {e}"));
        assert!(!p.states.is_empty(), "{genesis}: the Gated-DeltaNet recurrence lowers to a fixed state");
        palw_tir_cell_coverage_v1(&k2_tir_v4_descriptor(), &p, Some((2, 1)))
            .unwrap_or_else(|g| panic!("{genesis}: sharded S_L=2: {g:?}"));
    }
}

/// **Text generation only** (user policy 2026-10-15): no genesis text decoder lowers a job input (`input.image_rows`, …) or a vision,
/// audio, video or multi-token-prediction component; the container's scope check passes on each real program.
#[test]
fn no_genesis_text_decoder_lowers_an_image_audio_or_mtp_component() {
    use misaka_palw_tir_artifact::palwart_v3::{PalwArtProgramV3, check_text_scope_v3, program_kind};
    for (genesis, rel) in TIR_ROUTE {
        let p = program(&fixture(rel), 8192).unwrap_or_else(|e| panic!("{genesis}: {e}"));
        let rec = PalwArtProgramV3 { kind: program_kind::TIR_V1, bytes: p.encode(), ..PalwArtProgramV3::none() };
        check_text_scope_v3(&rec, &[]).unwrap_or_else(|e| panic!("{genesis}: {e}"));
    }
}
