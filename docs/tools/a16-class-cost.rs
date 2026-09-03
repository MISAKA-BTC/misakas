// **Archived source, not part of this crate's build.**
//
// This measured the A16 court costs quoted in `docs/palw-second-class-weight-2026-08-26.md` §4.
// It cannot compile on `main`: `qwen25_a16_profile_v1` and `PALW_RC_QWEN25_1_5B` live on
// `palw-mainnet-rc-integration`, which is where the A16 decomposition is. To re-run it, drop this
// file into `misaka-palw-base0/examples/` on that branch and:
//
//     cargo run --release -p misaka-palw-base0 --example a16-class-cost
//
// It is archived here rather than committed there because that branch belongs to other work in
// flight; a measurement tool should not arrive in somebody else's tree unannounced.

//! **What the A16 Qwen class costs a court** — the half of the weight decision that only this
//! branch can answer, because only this branch has the A16 decomposition.
//!
//! The int8 (W8A8) Qwen profile on `main` is jammed against two ceilings at once: every tile above
//! 64 is refused by `max_opening_bytes`, and at tile 64 the ladder is 99.9% spent at `n_ctx 90`.
//! The A16 profile is a different graph. This prints the four court costs it actually implies, so
//! the next mint's ceilings are chosen from a measurement rather than from the shipped values.

use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_class_admission_v2::{PALW_RC_COURT_MAX_STEP_LEAF_COUNT, derive_court_cost_v1};
use kaspa_consensus_core::palw_qwen25_profile::{
    PALW_RC_QWEN25_1_5B, PalwQwen25GeometryV1, qwen25_a16_profile_v1, qwen25_profile_v1,
};
use kaspa_consensus_core::palw_step::{
    PALW_STEP_MAX_LEAVES, PalwStepError, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1,
};

/// What testnet-11's shipped bundle declares, for the "would it fit today" column.
const T11_MAX_OPENING_BYTES: u64 = 1_048_576;
const T11_MAX_TERMINAL_MACS: u64 = 16_777_216;
const T11_MAX_OPERANDS: u32 = 8;

fn row(name: &str, g: PalwQwen25GeometryV1, a16: bool) {
    let profile = match if a16 { qwen25_a16_profile_v1(g) } else { qwen25_profile_v1(g) } {
        Ok(p) => p,
        Err(e) => {
            println!("| {name} | tile {} / ctx {} | not expressible ({e:?}) | | | | |", g.tile_len, g.n_ctx);
            return;
        }
    };
    let canonical = rc_job_context(&profile, 8, 4);
    // **Counted at the RC ruleset's ladder, not at the executor's constant.** Both were already
    // printed side by side below, and the columns were nevertheless taken against
    // `PALW_STEP_MAX_LEAVES` — so a class the RC ruleset admits read as `TooManyLeaves` in the
    // very table meant to price it.
    let pwu = step_leaf_count_capped_v1(&profile, &canonical, PALW_RC_COURT_MAX_STEP_LEAF_COUNT)
        .map(|n| n.to_string())
        .unwrap_or_else(|e| format!("{e:?}"));
    // The count stops at the cap, so a refused number is a lower bound and is printed as one.
    let worst = match worst_case_step_leaf_count_capped_v1(&profile, PALW_RC_COURT_MAX_STEP_LEAF_COUNT) {
        Ok(n) => format!("{n}"),
        Err(PalwStepError::TooManyLeaves { got, .. }) => format!("≥{got} OVER"),
        Err(e) => format!("{e:?}"),
    };
    let cost = derive_court_cost_v1(&profile);
    let (open, macs, ops) = match &cost {
        Ok(c) => (c.max_opening_bytes.to_string(), c.max_terminal_macs.to_string(), c.max_operand_count.to_string()),
        Err(e) => (format!("{e:?}"), "—".into(), "—".into()),
    };
    let t11 = match &cost {
        Ok(c) => {
            let mut over: Vec<String> = Vec::new();
            if c.max_opening_bytes > T11_MAX_OPENING_BYTES {
                over.push(format!("open {:.1}×", c.max_opening_bytes as f64 / T11_MAX_OPENING_BYTES as f64));
            }
            if c.max_terminal_macs > T11_MAX_TERMINAL_MACS {
                over.push(format!("macs {:.1}×", c.max_terminal_macs as f64 / T11_MAX_TERMINAL_MACS as f64));
            }
            if c.max_operand_count > T11_MAX_OPERANDS {
                over.push(format!("operands {:.1}×", c.max_operand_count as f64 / T11_MAX_OPERANDS as f64));
            }
            if over.is_empty() { "fits t11".to_string() } else { over.join(" · ") }
        }
        Err(_) => "—".to_string(),
    };
    println!("| {name} | tile {} / ctx {} | {pwu} | {worst} | {open} | {macs} | {ops} | {t11} |", g.tile_len, g.n_ctx);
}

fn main() {
    println!("# What the A16 Qwen class costs a court");
    println!();
    println!("PALW_STEP_MAX_LEAVES (the EXECUTOR's code constant, not what these columns count at) = {PALW_STEP_MAX_LEAVES}");
    println!("PALW_RC_COURT_MAX_STEP_LEAF_COUNT (the RC ladder — every leaf column below is counted at THIS) = {PALW_RC_COURT_MAX_STEP_LEAF_COUNT}");
    println!("testnet-11 shipped: open {T11_MAX_OPENING_BYTES} · macs {T11_MAX_TERMINAL_MACS} · operands {T11_MAX_OPERANDS}");
    println!();
    println!("| profile | geometry | pwu/inference (8+4) | worst case | max_opening_bytes | max_terminal_macs | operands | against t11 |");
    println!("| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |");
    row("Qwen2.5-1.5B A16", PALW_RC_QWEN25_1_5B, true);
    row("Qwen2.5-1.5B int8, same geometry", PALW_RC_QWEN25_1_5B, false);
    row("Qwen2.5-1.5B int8, t11's admissible", PalwQwen25GeometryV1 { n_ctx: 90, tile_len: 64, ..PALW_RC_QWEN25_1_5B }, false);
    for (tile, ctx) in [(2048u32, 4096u32), (4096, 2048), (1024, 2048), (512, 2048), (256, 2048), (128, 2048), (64, 2048), (64, 90), (128, 90), (2048, 90)] {
        row(
            &format!("A16 @ tile {tile} / ctx {ctx}"),
            PalwQwen25GeometryV1 { n_ctx: ctx, tile_len: tile, ..PALW_RC_QWEN25_1_5B },
            true,
        );
    }
}
