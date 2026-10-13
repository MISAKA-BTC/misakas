//! Read existing kernel-enforced quotas. Never install a quota or infer one from
//! a configured number, MemAvailable, RSS or fixture measurements.
use super::NativeCoverageV1;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeHardMemoryQuotaV1 {
    limit_bytes: u64,
    used_bytes: u64,
    evidence_path: String,
}
impl NativeHardMemoryQuotaV1 {
    pub fn limit_bytes(&self) -> u64 {
        self.limit_bytes
    }
    pub fn headroom_bytes(&self) -> u64 {
        self.limit_bytes.saturating_sub(self.used_bytes)
    }
    pub fn evidence_path(&self) -> &str {
        &self.evidence_path
    }
}
#[cfg(target_os = "linux")]
pub fn actual_hard_memory_quota_v1() -> Result<NativeHardMemoryQuotaV1, NativeCoverageV1> {
    use std::{
        fs,
        path::{Component, Path},
    };
    let refuse = |why: &str| NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>("allocator.physical-memory", why);
    let read = |path: &Path| -> Result<String, NativeCoverageV1> {
        let metadata = fs::metadata(path).map_err(|_| refuse("kernel quota file is unavailable"))?;
        if metadata.len() > 64 * 1024 {
            return Err(refuse("kernel quota metadata exceeds bounded input"));
        }
        fs::read_to_string(path).map_err(|_| refuse("kernel quota metadata cannot be read"))
    };
    let mountinfo = read(Path::new("/proc/self/mountinfo"))?;
    if !mountinfo.lines().any(|line| {
        line.split_once(" - ").is_some_and(|(left, right)| {
            left.split_whitespace().nth(4) == Some("/sys/fs/cgroup") && right.split_whitespace().next() == Some("cgroup2")
        })
    }) {
        return Err(refuse("an actual cgroup-v2 kernel mount at /sys/fs/cgroup was not observed"));
    }
    let membership = read(Path::new("/proc/self/cgroup"))?;
    let group = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| refuse("current process has no unified cgroup-v2 membership"))?;
    let relative = Path::new(group.trim_start_matches('/'));
    if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(refuse("kernel cgroup path is not a confined normal path"));
    }
    let root = Path::new("/sys/fs/cgroup").canonicalize().map_err(|_| refuse("kernel cgroup mount is unavailable"))?;
    let actual = root.join(relative).canonicalize().map_err(|_| refuse("current cgroup directory is unavailable"))?;
    if !actual.starts_with(&root) {
        return Err(refuse("kernel cgroup membership escapes its mount"));
    }
    let max = read(&actual.join("memory.max"))?;
    let limit_bytes = max
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n < u64::MAX)
        .ok_or_else(|| refuse("memory.max is unlimited or has no finite hard limit"))?;
    let used_bytes =
        read(&actual.join("memory.current"))?.trim().parse::<u64>().map_err(|_| refuse("memory.current has no kernel usage value"))?;
    // Membership is read again, so a migration cannot lend another group's cap.
    if read(Path::new("/proc/self/cgroup"))? != membership {
        return Err(refuse("current process changed cgroup while quota evidence was read"));
    }
    Ok(NativeHardMemoryQuotaV1 { limit_bytes, used_bytes, evidence_path: actual.to_string_lossy().into_owned() })
}
#[cfg(not(target_os = "linux"))]
pub fn actual_hard_memory_quota_v1() -> Result<NativeHardMemoryQuotaV1, NativeCoverageV1> {
    Err(NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>(
        "allocator.physical-memory",
        "no implemented OS hard-memory-quota evidence provider on this target; generic/mimalloc physical arena overhead is unknown",
    ))
}
