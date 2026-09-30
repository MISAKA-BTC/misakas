//! **Admission of a lowered program**: `tir_admit_v1` (spec 04b §10.3, tir/core) run on what the
//! lowerer emits, with the two inputs a class layout declares — `tile_len` (values per step leaf)
//! and the canonical history chunk (`h_tile`, admission's `h_chunk`) — and a set of ceilings, and
//! the result rendered for `palw-tir-check` and `palw-class check-architecture`.
//!
//! Nothing here decides anything: the verdict and every number are `tir_admit_v1`'s. What this
//! module adds is the report — the per-position costs, the state and live bytes, the court cones
//! (the worst terminal tile, the dissected ones), the checkpoint interval and admission's own work.

use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::admit::{TirAdmissionV1, TirAdmitError, TirAdmitInputsV1, TirCeilingsV1, tir_admit_program_v1};
use serde_json::json;

/// The inputs the tools admit with unless told otherwise: tiles of 64 values (the legacy step
/// graph's usual `tile_len`), a 64-position history chunk, and tir/core's starting ceilings (the
/// legacy court's 16 Mi terminal MACs per tile).
pub fn default_inputs() -> TirAdmitInputsV1 {
    TirAdmitInputsV1 { tile_len: 64, h_chunk: 64, ceilings: TirCeilingsV1::legacy_court_v1() }
}

/// `tir_admit_v1` of a program in memory.
pub fn admit(p: &TirProgramV1, inputs: &TirAdmitInputsV1) -> Result<TirAdmissionV1, TirAdmitError> {
    tir_admit_program_v1(p, inputs)
}

fn si(v: u64) -> String {
    let f = v as f64;
    if v < 100_000 { v.to_string() } else { format!("{f:.3e}") }
}

fn bytes(v: u64) -> String {
    const K: f64 = 1024.0;
    let f = v as f64;
    if f >= K * K * K {
        format!("{:.2} GiB", f / (K * K * K))
    } else if f >= K * K {
        format!("{:.2} MiB", f / (K * K))
    } else if f >= K {
        format!("{:.1} KiB", f / K)
    } else {
        format!("{v} B")
    }
}

/// The history windows the program runs at, one per block that appends to a history.
pub fn windows(p: &TirProgramV1) -> Vec<(u8, u32)> {
    use misaka_palw_tir::Prim;
    use misaka_palw_tir::program::StateKind;
    p.blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            b.nodes.iter().find_map(|n| match n.prim {
                Prim::HistAppend { state } => match p.states[state as usize].kind {
                    StateKind::Hist { window } => Some((i as u8, window)),
                    StateKind::Fixed { .. } => None,
                },
                _ => None,
            })
        })
        .collect()
}

/// The report's text lines (indented two spaces, as `palw-tir-check` prints its TIR section).
pub fn render(p: &TirProgramV1, inputs: &TirAdmitInputsV1, verdict: &Result<TirAdmissionV1, TirAdmitError>) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(
        s,
        "  admission (tir_admit_v1; tile_len {}, h_chunk {}, terminal ceiling {} MACs/tile): ",
        inputs.tile_len,
        inputs.h_chunk,
        si(inputs.ceilings.max_tile_macs)
    );
    let a = match verdict {
        Ok(a) => a,
        Err(e) => {
            let _ = writeln!(s, "REFUSED — {e}");
            return s;
        }
    };
    let _ = writeln!(s, "ADMITTED");
    let c = &a.position.cost;
    let _ = writeln!(
        s,
        "    one position at H = W: {} MACs, {} elementwise, {} transcendentals; state {}; peak live {}; {} commit lanes in {} step leaves",
        si(c.macs),
        si(c.elementwise),
        si(c.transcendentals),
        bytes(a.position.state_bytes),
        bytes(a.position.peak_live_bytes),
        si(a.position.commit_lanes),
        si(a.position.step_leaves)
    );
    let w = windows(p);
    if !w.is_empty() {
        let list: Vec<String> = w.iter().map(|(b, w)| format!("block {b}: {w}")).collect();
        let _ = writeln!(s, "    history windows W: {}", list.join(", "));
    }
    let dissected = a.cones.iter().filter(|c| !c.h_reductions.is_empty()).count();
    if let Some(worst) = a.cones.iter().max_by_key(|c| (c.terminal().macs, c.terminal_opened_bytes())) {
        let _ = writeln!(
            s,
            "    court cones: {} ({dissected} dissected over H at {} positions a chunk); worst terminal: {} MACs, {} opened, {} committed operands (block {} node {}{})",
            a.cones.len(),
            inputs.h_chunk,
            si(worst.terminal().macs),
            bytes(worst.terminal_opened_bytes()),
            worst.operands,
            worst.block,
            worst.node,
            if worst.chunk.is_some() { ", one H chunk" } else { ", one tile" }
        );
    }
    if a.states.is_empty() {
        let _ = writeln!(s, "    checkpoint interval C: {} (no Fixed state; the ceiling's cap)", a.checkpoint_interval);
    } else {
        let worst = a.states.iter().min_by_key(|x| x.interval).expect("a state");
        let _ = writeln!(
            s,
            "    checkpoint interval C = min C_j: {} (state `{}`: {} replay group(s), {} MACs a position a group)",
            a.checkpoint_interval,
            p.states[worst.state as usize].name,
            worst.groups,
            si(worst.per_position.macs)
        );
    }
    let _ = writeln!(s, "    admission's cone work: {} of {}", a.cone_work, si(inputs.ceilings.max_cone_work));
    s
}

/// The report as JSON.
pub fn to_json(p: &TirProgramV1, inputs: &TirAdmitInputsV1, verdict: &Result<TirAdmissionV1, TirAdmitError>) -> serde_json::Value {
    let head = json!({ "tile_len": inputs.tile_len, "h_chunk": inputs.h_chunk, "max_tile_macs": inputs.ceilings.max_tile_macs });
    match verdict {
        Err(e) => json!({ "inputs": head, "admitted": false, "refusal": e.to_string() }),
        Ok(a) => {
            let c = &a.position.cost;
            let worst = a.cones.iter().max_by_key(|c| (c.terminal().macs, c.terminal_opened_bytes()));
            json!({
                "inputs": head,
                "admitted": true,
                "position": {
                    "macs": c.macs, "elementwise": c.elementwise, "transcendentals": c.transcendentals,
                    "state_bytes": a.position.state_bytes, "peak_live_bytes": a.position.peak_live_bytes,
                    "commit_lanes": a.position.commit_lanes, "step_leaves": a.position.step_leaves,
                },
                "windows": windows(p).iter().map(|(b, w)| json!({"block": b, "window": w})).collect::<Vec<_>>(),
                "cones": a.cones.len(),
                "dissected_cones": a.cones.iter().filter(|c| !c.h_reductions.is_empty()).count(),
                "worst_terminal": worst.map(|w| json!({
                    "block": w.block, "node": w.node, "macs": w.terminal().macs,
                    "transcendentals": w.terminal().transcendentals,
                    "opened_bytes": w.terminal_opened_bytes(), "operands": w.operands, "chunk": w.chunk.is_some(),
                })),
                "checkpoint_interval": a.checkpoint_interval,
                "cone_work": a.cone_work,
            })
        }
    }
}
