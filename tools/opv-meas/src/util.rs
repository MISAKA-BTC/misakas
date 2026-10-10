//! Timing, CPU time, peak RSS, load average, a phase recorder and a memory watchdog.
use serde_json::{Value, json};
use std::time::Instant;

/// `(user seconds, system seconds, peak RSS bytes)` of this process so far.
pub fn rusage() -> (f64, f64, u64) {
    // SAFETY: getrusage writes into a zeroed rusage we own.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    #[cfg(target_os = "macos")]
    let rss = ru.ru_maxrss as u64; // bytes on macOS
    #[cfg(not(target_os = "macos"))]
    let rss = (ru.ru_maxrss as u64) * 1024; // KiB elsewhere
    (tv(ru.ru_utime), tv(ru.ru_stime), rss)
}

/// 1, 5 and 15 minute load averages of the host.
pub fn loadavg() -> [f64; 3] {
    let mut l = [0f64; 3];
    // SAFETY: getloadavg fills at most 3 doubles.
    unsafe { libc::getloadavg(l.as_mut_ptr(), 3) };
    l
}

pub fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

pub fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

pub fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// Kill this process (exit 86) when its peak RSS passes `cap_bytes`: a measurement must not take the shared host down.
pub fn start_rss_watchdog(cap_bytes: u64) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let (_, _, rss) = rusage();
            if rss > cap_bytes {
                eprintln!("{{\"watchdog\":\"peak RSS {rss} passed the cap {cap_bytes}; exiting 86\"}}");
                std::process::exit(86);
            }
        }
    });
}

/// Records one JSON object per phase: wall, CPU (user+system), bytes moved, cumulative peak RSS, the 1-minute load.
#[derive(Default)]
pub struct Rec {
    pub phases: Vec<Value>,
}

impl Rec {
    pub fn phase<R>(&mut self, name: &str, f: impl FnOnce() -> (R, u64)) -> R {
        let t0 = Instant::now();
        let (u0, s0, _) = rusage();
        let l0 = loadavg()[0];
        let (r, bytes) = f();
        let wall = t0.elapsed().as_secs_f64();
        let (u1, s1, rss) = rusage();
        self.phases.push(json!({
            "name": name, "wall_s": wall, "cpu_s": (u1 - u0) + (s1 - s0), "user_s": u1 - u0, "sys_s": s1 - s0,
            "bytes": bytes, "peak_rss_bytes": rss, "load1_start": l0, "load1_end": loadavg()[0],
        }));
        r
    }
}
