//! **RAM accounting** (RFC-0020 §2): live and peak bytes by kind and by stage, so a long-context run that runs out of memory
//! can be attributed (weights, carries, attention history, recurrent state, attention temporaries, statistics, logits, spilled
//! carries) instead of guessed. Off by default and then free (one relaxed atomic load per call). Turned on by
//! [`enable`] (`palw-tir-convert --ram-accounting FILE`, `palw-tir-fidelity --ram-accounting FILE`), it appends one JSON line
//! per snapshot — the stage, the live and peak bytes of every kind, and the process's resident set as the OS reports it — and
//! a per-stage summary at [`finish`].
//!
//! The kinds are the evaluator's own data structures measured by their lengths (`len × 4` for `f32` rows), not the
//! allocator's view: an allocator may keep freed memory, so the resident set (`rss`) can stay above the live total. Both are
//! recorded for that reason.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

/// What the bytes are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The params resident for the occurrence in flight (the layer, or the embedding / head), with the lazy cache.
    Weights,
    /// The carries between occurrences held in RAM (one full sequence of rows per sequence).
    Carries,
    /// Carries spilled to disk ([`crate::float_ref::stream::SpillCfg`]): not RAM, recorded beside it.
    Spilled,
    /// Attention key/value histories of the sessions of the occurrence in flight.
    KvHistory,
    /// Fixed recurrent states (Gated DeltaNet, conv windows) of those sessions.
    RecurrentState,
    /// The largest per-query attention temporaries (scores, probabilities) one call made.
    AttentionTemp,
    /// Calibration statistics.
    Stats,
    /// Logits kept for the caller.
    Logits,
}

pub const KINDS: [Kind; 8] = [
    Kind::Weights,
    Kind::Carries,
    Kind::Spilled,
    Kind::KvHistory,
    Kind::RecurrentState,
    Kind::AttentionTemp,
    Kind::Stats,
    Kind::Logits,
];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Weights => "weights",
            Kind::Carries => "carries",
            Kind::Spilled => "spilled_carries_on_disk",
            Kind::KvHistory => "kv_history",
            Kind::RecurrentState => "recurrent_state",
            Kind::AttentionTemp => "attention_temporaries",
            Kind::Stats => "stats",
            Kind::Logits => "logits",
        }
    }
    fn at(self) -> usize {
        KINDS.iter().position(|k| *k == self).unwrap_or(0)
    }
}

static ON: AtomicBool = AtomicBool::new(false);
static LIVE: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
static PEAK: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
static STATE: Mutex<Option<Acct>> = Mutex::new(None);

struct Acct {
    out: std::fs::File,
    t0: Instant,
    stage: String,
    rss_peak: u64,
    /// Per stage: the peak of every kind, of the live total, and of the resident set.
    stages: BTreeMap<String, ([u64; 8], u64, u64)>,
    order: Vec<String>,
    live_total_peak: u64,
}

/// Turn accounting on, appending JSON lines to `path`.
pub fn enable(path: &std::path::Path) -> std::io::Result<()> {
    let out = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Acct {
        out,
        t0: Instant::now(),
        stage: "start".into(),
        rss_peak: 0,
        stages: BTreeMap::new(),
        order: vec!["start".into()],
        live_total_peak: 0,
    });
    ON.store(true, Relaxed);
    Ok(())
}

pub fn enabled() -> bool {
    ON.load(Relaxed)
}

/// The live bytes of `kind` are now `bytes`.
pub fn set(kind: Kind, bytes: u64) {
    if !enabled() {
        return;
    }
    LIVE[kind.at()].store(bytes, Relaxed);
    PEAK[kind.at()].fetch_max(bytes, Relaxed);
}

/// A transient of `bytes` of `kind` existed (counts toward the peak only).
pub fn note_max(kind: Kind, bytes: u64) {
    if !enabled() {
        return;
    }
    PEAK[kind.at()].fetch_max(bytes, Relaxed);
}

/// The process's resident set in bytes, as the OS reports it (`/proc/self/statm` on Linux, `ps` elsewhere).
pub fn rss_bytes() -> Option<u64> {
    if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
        let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
        return Some(pages * 4096);
    }
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    let kib: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kib * 1024)
}

fn kinds_json(v: &[u64; 8]) -> serde_json::Value {
    serde_json::Value::Object(KINDS.iter().map(|k| (k.name().to_string(), serde_json::json!(v[k.at()]))).collect())
}

fn now_values(a: &[AtomicU64; 8]) -> [u64; 8] {
    let mut v = [0u64; 8];
    for (i, x) in a.iter().enumerate() {
        v[i] = x.load(Relaxed);
    }
    v
}

/// Append a snapshot: the stage, `label`, live and peak bytes by kind, the live total (RAM kinds only) and the resident set.
pub fn snapshot(label: &str) {
    if !enabled() {
        return;
    }
    let (live, peak) = (now_values(&LIVE), now_values(&PEAK));
    let ram_total: u64 = KINDS.iter().filter(|k| **k != Kind::Spilled).map(|k| live[k.at()]).sum();
    let rss = rss_bytes().unwrap_or(0);
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(a) = g.as_mut() else { return };
    a.rss_peak = a.rss_peak.max(rss);
    a.live_total_peak = a.live_total_peak.max(ram_total);
    let entry = a.stages.entry(a.stage.clone()).or_insert(([0; 8], 0, 0));
    for (i, p) in peak.iter().enumerate() {
        entry.0[i] = entry.0[i].max(*p);
    }
    entry.1 = entry.1.max(ram_total);
    entry.2 = entry.2.max(rss);
    let line = serde_json::json!({
        "t": a.t0.elapsed().as_secs_f64(),
        "stage": a.stage,
        "label": label,
        "live": kinds_json(&live),
        "peak": kinds_json(&peak),
        "live_ram_total": ram_total,
        "rss": rss,
    });
    let _ = writeln!(a.out, "{line}");
}

/// Enter stage `name` (`calibration`, `conversion`, `fidelity.float`, `fidelity.int`, …): the previous stage is closed with a
/// snapshot, and the per-kind peaks restart from the live values.
pub fn stage(name: &str) {
    if !enabled() {
        return;
    }
    snapshot("stage end");
    {
        let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(a) = g.as_mut() {
            a.stage = name.to_string();
            if !a.order.iter().any(|s| s == name) {
                a.order.push(name.to_string());
            }
        }
    }
    for (l, p) in LIVE.iter().zip(PEAK.iter()) {
        p.store(l.load(Relaxed), Relaxed);
    }
    snapshot("stage start");
}

/// Close the accounting: a final snapshot and one summary line with every stage's peaks.
pub fn finish() {
    if !enabled() {
        return;
    }
    snapshot("finish");
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(a) = g.as_mut() else { return };
    let stages: Vec<serde_json::Value> = a
        .order
        .iter()
        .filter_map(|s| a.stages.get(s).map(|(k, t, r)| (s, k, t, r)))
        .map(|(s, k, t, r)| serde_json::json!({ "stage": s, "peak": kinds_json(k), "peak_live_ram_total": t, "peak_rss": r }))
        .collect();
    let line = serde_json::json!({
        "summary": true,
        "t": a.t0.elapsed().as_secs_f64(),
        "stages": stages,
        "peak_live_ram_total": a.live_total_peak,
        "peak_rss": a.rss_peak,
        "note": "live bytes are the evaluator's structures by length; rss is the OS's resident set (an allocator may keep freed memory)",
    });
    let _ = writeln!(a.out, "{line}");
}
