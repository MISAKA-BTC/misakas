//! What this machine actually has — and how much of it a model may use.
//!
//! Two jobs, and they are different. [`HardwareSnapshot::probe`] answers *what is installed*
//! (once, at startup, and on demand); [`HardwareMonitor`] answers *what is happening now* (many
//! times a second, cheaply). Conflating them is how a performance monitor ends up forking
//! `nvidia-smi` sixty times a second.
//!
//! # On not inventing numbers
//!
//! Accelerator memory is discovered by asking the vendor's own tool (`nvidia-smi`, `rocm-smi`) or
//! the OS (`sysctl` on Apple Silicon). Where no tool answers, the field is `None` and the UI says
//! "unknown" — an invented VRAM figure is worse than a blank one, because a person will size a
//! download against it.
//!
//! Apple Silicon is the case that does not fit the vocabulary: there is no VRAM, there is one
//! pool. Reporting "0 MB VRAM" on a 128 GB M-series machine — which is what a discrete-GPU
//! assumption produces — is the single most misleading thing a local-LLM app can say, so a
//! unified-memory device reports the system pool and the fraction the GPU may wire down
//! (`iogpu.wired_limit_pct`, whose default is ~75 %).

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Kind of compute device, which is also the backend that will drive it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceleratorKind {
    /// Apple Silicon: Metal over unified memory. No separate VRAM pool.
    AppleUnified,
    /// NVIDIA, via CUDA.
    Cuda,
    /// AMD, via ROCm/HIP.
    Rocm,
    /// A GPU visible to Vulkan but not to a vendor SDK we can query.
    Vulkan,
    /// No accelerator; the CPU does the work.
    Cpu,
}

/// One compute device.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Accelerator {
    pub kind: AcceleratorKind,
    pub name: String,
    /// Device memory in bytes. On a unified-memory machine this is the system pool.
    pub total_memory: Option<u64>,
    /// Free device memory in bytes, when the vendor tool reports it.
    pub free_memory: Option<u64>,
    /// Bytes a model may realistically occupy: device memory less what the OS and display keep.
    /// This — not `total_memory` — is what a fit check should compare against.
    pub usable_memory: Option<u64>,
    /// Driver or runtime version string, when discoverable.
    pub driver: Option<String>,
    /// Index as the backend will address it (`CUDA_VISIBLE_DEVICES` ordering).
    pub index: u32,
}

/// The static picture: CPU, memory, accelerators, OS.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HardwareSnapshot {
    pub os: String,
    pub arch: String,
    pub cpu_name: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub total_memory: u64,
    pub available_memory: u64,
    pub accelerators: Vec<Accelerator>,
}

/// A live sample: the numbers a performance monitor draws.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HardwareSample {
    /// System-wide CPU utilisation, 0-100.
    pub cpu_percent: f32,
    /// This process's CPU utilisation, 0-100 per core-equivalent.
    pub process_cpu_percent: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    /// Resident memory of the Studio's own process tree.
    pub process_memory: u64,
    /// Per-accelerator utilisation and memory, in [`HardwareSnapshot::accelerators`] order.
    pub accelerators: Vec<AcceleratorSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AcceleratorSample {
    pub index: u32,
    pub name: String,
    pub utilization_percent: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub temperature_c: Option<f32>,
}

impl HardwareSnapshot {
    /// Probe the machine. Runs vendor tools, so call it at startup and on request — not in a
    /// render loop.
    pub fn probe() -> Self {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        let cpu_name = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_else(|| "unknown CPU".into());
        let total_memory = sys.total_memory();

        let mut accelerators = detect_nvidia();
        accelerators.extend(detect_apple(total_memory));
        accelerators.extend(detect_amd());
        if accelerators.is_empty() {
            accelerators.push(Accelerator {
                kind: AcceleratorKind::Cpu,
                name: cpu_name.clone(),
                total_memory: Some(total_memory),
                free_memory: Some(sys.available_memory()),
                usable_memory: Some(cpu_usable_memory(total_memory)),
                driver: None,
                index: 0,
            });
        }

        HardwareSnapshot {
            os: format!(
                "{} {}",
                sysinfo::System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
                sysinfo::System::os_version().unwrap_or_default()
            )
            .trim()
            .to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_name,
            physical_cores: sys.physical_core_count(),
            logical_cores: sys.cpus().len(),
            total_memory,
            available_memory: sys.available_memory(),
            accelerators,
        }
    }

    /// The largest single memory pool a model could live in: the best accelerator, or system RAM.
    pub fn best_usable_memory(&self) -> u64 {
        self.accelerators.iter().filter_map(|a| a.usable_memory).max().unwrap_or_else(|| cpu_usable_memory(self.total_memory))
    }

    /// True when GPU offload is possible at all — the UI hides offload controls otherwise.
    pub fn has_gpu(&self) -> bool {
        self.accelerators.iter().any(|a| a.kind != AcceleratorKind::Cpu)
    }
}

/// System RAM a model may take: total less a 2 GB floor for the OS, capped at 85 %.
///
/// A machine that swaps its own desktop out to hold a model has not run the model, it has
/// stopped being usable — so the reserve is not negotiable and the estimate is deliberately
/// conservative.
pub fn cpu_usable_memory(total: u64) -> u64 {
    const OS_RESERVE: u64 = 2 << 30;
    let by_reserve = total.saturating_sub(OS_RESERVE);
    let by_fraction = (total as f64 * 0.85) as u64;
    by_reserve.min(by_fraction)
}

/// Samples the machine repeatedly, and does not fork a vendor tool to do it more often than
/// [`GPU_POLL_INTERVAL`].
pub struct HardwareMonitor {
    system: sysinfo::System,
    pid: sysinfo::Pid,
    accelerators: Vec<Accelerator>,
    gpu_cache: Vec<AcceleratorSample>,
    gpu_polled_at: Option<Instant>,
}

/// `nvidia-smi` costs tens of milliseconds and spawns a process. Twice a second is plenty for a
/// number a human reads.
pub const GPU_POLL_INTERVAL: Duration = Duration::from_millis(500);

impl HardwareMonitor {
    pub fn new(snapshot: &HardwareSnapshot) -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_cpu_all();
        system.refresh_memory();
        HardwareMonitor {
            system,
            pid: sysinfo::Pid::from_u32(std::process::id()),
            accelerators: snapshot.accelerators.clone(),
            gpu_cache: Vec::new(),
            gpu_polled_at: None,
        }
    }

    /// Take a sample.
    ///
    /// CPU percentages need two refreshes separated in time to mean anything; the caller's poll
    /// interval supplies that separation, and the first sample after construction is therefore
    /// the only inaccurate one.
    pub fn sample(&mut self) -> HardwareSample {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[self.pid]));

        let cpu_percent = self.system.global_cpu_usage();
        let (process_cpu_percent, process_memory) =
            self.system.process(self.pid).map(|p| (p.cpu_usage(), p.memory())).unwrap_or((0.0, 0));

        let stale = self.gpu_polled_at.map(|t| t.elapsed() >= GPU_POLL_INTERVAL).unwrap_or(true);
        if stale && self.accelerators.iter().any(|a| a.kind != AcceleratorKind::Cpu) {
            self.gpu_cache = sample_accelerators(&self.accelerators);
            self.gpu_polled_at = Some(Instant::now());
        }

        HardwareSample {
            cpu_percent,
            process_cpu_percent,
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            process_memory,
            accelerators: self.gpu_cache.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Vendor probes. Each returns an empty vector when its tool is absent, which is the normal case
// on a machine without that vendor's hardware — never an error.
// ---------------------------------------------------------------------------

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn detect_nvidia() -> Vec<Accelerator> {
    let Some(text) =
        run("nvidia-smi", &["--query-gpu=index,name,memory.total,memory.free,driver_version", "--format=csv,noheader,nounits"])
    else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() < 5 {
                return None;
            }
            let total = f[2].parse::<u64>().ok().map(|mib| mib * 1024 * 1024);
            let free = f[3].parse::<u64>().ok().map(|mib| mib * 1024 * 1024);
            Some(Accelerator {
                kind: AcceleratorKind::Cuda,
                name: f[1].to_string(),
                total_memory: total,
                free_memory: free,
                // A discrete GPU also holds the desktop's framebuffer and the driver's own
                // allocations; ~90 % of free is what a loader can actually take.
                usable_memory: free.map(|b| (b as f64 * 0.9) as u64),
                driver: Some(f[4].to_string()),
                index: f[0].parse().unwrap_or(0),
            })
        })
        .collect()
}

fn detect_amd() -> Vec<Accelerator> {
    detect_amd_inner().unwrap_or_default()
}

fn detect_amd_inner() -> Option<Vec<Accelerator>> {
    let text = run("rocm-smi", &["--showproductname", "--showmeminfo", "vram", "--csv"])?;
    // rocm-smi's CSV shape has changed across releases; take the fields by header name so a
    // reordering does not silently produce nonsense.
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next()?.split(',').map(str::trim).collect();
    let col = |needle: &str| header.iter().position(|h| h.to_ascii_lowercase().contains(needle));
    let name_col = col("card series").or_else(|| col("product name"));
    let total_col = col("vram total memory");
    let used_col = col("vram total used memory");
    let devices = lines
        .enumerate()
        .map(|(i, line)| {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            let get = |c: Option<usize>| c.and_then(|c| f.get(c)).copied();
            let total = get(total_col).and_then(|v| v.parse::<u64>().ok());
            let used = get(used_col).and_then(|v| v.parse::<u64>().ok());
            let free = match (total, used) {
                (Some(t), Some(u)) => Some(t.saturating_sub(u)),
                _ => None,
            };
            Accelerator {
                kind: AcceleratorKind::Rocm,
                name: get(name_col).unwrap_or("AMD GPU").to_string(),
                total_memory: total,
                free_memory: free,
                usable_memory: free.map(|b| (b as f64 * 0.9) as u64),
                driver: None,
                index: i as u32,
            }
        })
        .collect();
    Some(devices)
}

/// Apple Silicon. Not a GPU with memory — a memory with a GPU attached.
fn detect_apple(total_memory: u64) -> Vec<Accelerator> {
    if !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")) {
        return Vec::new();
    }
    let name = run("sysctl", &["-n", "machdep.cpu.brand_string"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Apple Silicon".to_string());

    // `iogpu.wired_limit_pct` is the share of unified memory Metal may wire down. Unset means
    // the kernel default, which is 75 % below 36 GB of RAM and 92 % above it.
    let pct = run("sysctl", &["-n", "iogpu.wired_limit_pct"])
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|p| *p > 0.0)
        .unwrap_or(if total_memory > 36 * (1 << 30) { 92.0 } else { 75.0 });

    vec![Accelerator {
        kind: AcceleratorKind::AppleUnified,
        name,
        total_memory: Some(total_memory),
        free_memory: None,
        usable_memory: Some((total_memory as f64 * pct / 100.0) as u64),
        driver: run("sw_vers", &["-productVersion"]).map(|s| format!("macOS {}", s.trim())),
        index: 0,
    }]
}

fn sample_accelerators(known: &[Accelerator]) -> Vec<AcceleratorSample> {
    let mut samples = Vec::new();

    if known.iter().any(|a| a.kind == AcceleratorKind::Cuda)
        && let Some(text) = run(
            "nvidia-smi",
            &["--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu", "--format=csv,noheader,nounits"],
        )
    {
        for line in text.lines() {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() < 6 {
                continue;
            }
            samples.push(AcceleratorSample {
                index: f[0].parse().unwrap_or(0),
                name: f[1].to_string(),
                utilization_percent: f[2].parse().ok(),
                memory_used: f[3].parse::<u64>().ok().map(|m| m * 1024 * 1024),
                memory_total: f[4].parse::<u64>().ok().map(|m| m * 1024 * 1024),
                temperature_c: f[5].parse().ok(),
            });
        }
    }

    // Apple's unified memory has no separate "GPU memory used" to read without private APIs, so
    // the device appears with its pool and no utilisation rather than with a fabricated one.
    for a in known.iter().filter(|a| a.kind == AcceleratorKind::AppleUnified) {
        samples.push(AcceleratorSample {
            index: a.index,
            name: a.name.clone(),
            utilization_percent: None,
            memory_used: None,
            memory_total: a.total_memory,
            temperature_c: None,
        });
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probing_always_yields_a_device() {
        // Even with no GPU and no vendor tool, there is always the CPU — an empty accelerator
        // list would leave the UI with nothing to size a model against.
        let snap = HardwareSnapshot::probe();
        assert!(!snap.accelerators.is_empty());
        assert!(snap.total_memory > 0);
        assert!(snap.logical_cores > 0);
    }

    #[test]
    fn the_os_keeps_its_reserve() {
        let total = 16 << 30;
        let usable = cpu_usable_memory(total);
        assert!(usable < total, "the OS keeps some");
        assert!(usable >= 13 << 30, "but not most of it: {usable}");
        // A tiny machine must not underflow into a huge number.
        assert_eq!(cpu_usable_memory(1 << 30), 0);
    }

    #[test]
    fn sampling_twice_is_enough_for_a_cpu_reading() {
        let snap = HardwareSnapshot::probe();
        let mut mon = HardwareMonitor::new(&snap);
        let _ = mon.sample();
        let s = mon.sample();
        assert!(s.memory_total > 0);
        assert!((0.0..=100.0 * snap.logical_cores as f32).contains(&s.cpu_percent));
    }
}
