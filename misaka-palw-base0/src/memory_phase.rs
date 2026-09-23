//! **The execution's memory brackets** — the engine says what it holds at each phase of an attempt,
//! beside what the process holds, through a sink the node arms (ADR-0151 follow-up: measured, not
//! derived).
//!
//! The 2M producer was killed twice with the growth SILENT: no line between "producing anyway" and
//! `dmesg`, because the engine printed nothing while the attempt ran and the node's periodic line
//! could not say which term was growing. The brackets close that: at the cache's construction,
//! every [`PALW_MEMORY_BRACKET_POSITIONS_V1`] positions of the prefill, at the prefill's end, at
//! the capture's and the checkpoint leg's sealing, at the decode's end and at the return, the
//! engine emits one line carrying the K/V cache's own bytes (`A16Cache::resident_bytes_v1`), the
//! capture sink's (`Base0CaptureSinkV1::retained_bytes_v1`), the checkpoint leg's, and the
//! process's anonymous, file and swap bytes read from `/proc/self/smaps_rollup` — so the difference
//! between what the engine thinks it holds and what the kernel says it holds is a printed number.
//!
//! This crate carries no logger; the node arms [`arm_execution_phase_sink_v1`] with its own
//! (`kaspad`'s `info!`), and an unarmed sink costs one atomic load per bracket. Nothing here feeds
//! back into a figure: the profile is derived, and these lines are how it is checked against a run.

use std::sync::OnceLock;

/// How often the prefill brackets: every this many positions.
pub const PALW_MEMORY_BRACKET_POSITIONS_V1: usize = 16_384;

type Sink = Box<dyn Fn(&str) + Send + Sync>;

static SINK: OnceLock<Sink> = OnceLock::new();

/// Arm the sink, once. A second arming keeps the first.
pub fn arm_execution_phase_sink_v1(sink: Sink) {
    let _ = SINK.set(sink);
}

pub fn execution_phase_armed_v1() -> bool {
    SINK.get().is_some()
}

/// Emit one bracket line, built only when a sink is armed.
pub fn execution_phase_v1(line: impl FnOnce() -> String) {
    if let Some(sink) = SINK.get() {
        sink(&line());
    }
}

/// The process's memory as the kernel accounts it: anonymous, file-backed and swapped bytes, and
/// the resident total. Linux only (`/proc/self/smaps_rollup`); `None` elsewhere, and the line then
/// says so rather than printing zeros.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProcessMemoryV1 {
    pub anon: u64,
    pub file: u64,
    pub swap: u64,
    pub rss: u64,
}

pub fn process_memory_v1() -> Option<ProcessMemoryV1> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
        let field = |name: &str| -> u64 {
            text.lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|rest| rest.trim().strip_suffix("kB"))
                .and_then(|kb| kb.trim().parse::<u64>().ok())
                .map(|kb| kb.saturating_mul(1024))
                .unwrap_or(0)
        };
        Some(ProcessMemoryV1 { anon: field("Anonymous:"), file: field("Rss:").saturating_sub(field("Anonymous:")), swap: field("Swap:"), rss: field("Rss:") })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

pub fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

/// The process's part of a bracket line: `anon A file F swap S GiB`, or the reason it is absent.
pub fn process_memory_line_v1() -> String {
    match process_memory_v1() {
        Some(m) => format!("process anon {:.2} file {:.2} swap {:.2} GiB", gib(m.anon), gib(m.file), gib(m.swap)),
        None => "process memory n/a (not Linux)".to_string(),
    }
}
