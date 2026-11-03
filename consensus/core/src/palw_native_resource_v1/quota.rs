//! Read existing kernel-enforced quotas. Never install a quota or infer one from
//! a configured number, MemAvailable, RSS or fixture measurements.
use super::NativeCoverageV1;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeHardMemoryQuotaV1 {
    limit_bytes: u64,
    headroom_bytes: u64,
    evidence_path: String,
}
impl NativeHardMemoryQuotaV1 {
    pub fn limit_bytes(&self) -> u64 {
        self.limit_bytes
    }
    pub fn headroom_bytes(&self) -> u64 {
        self.headroom_bytes
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
        use std::io::Read;
        let mut text = String::new();
        fs::File::open(path)
            .map_err(|_| refuse("kernel quota file is unavailable"))?
            .take((64 << 10) + 1)
            .read_to_string(&mut text)
            .map_err(|_| refuse("kernel quota metadata cannot be read"))?;
        if text.len() > 64 << 10 {
            return Err(refuse("kernel quota metadata exceeds bounded input"));
        }
        Ok(text)
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
    // A service can inherit its only finite limit from a parent. Every
    // ancestor's current charge constrains this process, including siblings.
    let mut dir = actual.clone();
    let mut limit_bytes: Option<u64> = None;
    let mut headroom_bytes: Option<u64> = None;
    loop {
        let max = read(&dir.join("memory.max"))?;
        if max.trim() != "max" {
            let max = max
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|n| *n > 0 && *n < u64::MAX)
                .ok_or_else(|| refuse("ancestor memory.max has no valid hard limit"))?;
            let used = read(&dir.join("memory.current"))?
                .trim()
                .parse::<u64>()
                .map_err(|_| refuse("ancestor memory.current has no kernel usage value"))?;
            limit_bytes = Some(limit_bytes.map_or(max, |old| old.min(max)));
            let available = max.saturating_sub(used);
            headroom_bytes = Some(headroom_bytes.map_or(available, |old| old.min(available)));
        }
        if dir == root || !dir.pop() {
            break;
        }
    }
    let limit_bytes = limit_bytes.ok_or_else(|| refuse("no finite hard limit in the process's cgroup ancestry"))?;
    let headroom_bytes = headroom_bytes.unwrap_or(0).min(limit_bytes);
    // Membership is read again, so a migration cannot lend another group's cap.
    if read(Path::new("/proc/self/cgroup"))? != membership {
        return Err(refuse("current process changed cgroup while quota evidence was read"));
    }
    Ok(NativeHardMemoryQuotaV1 { limit_bytes, headroom_bytes, evidence_path: actual.to_string_lossy().into_owned() })
}
#[cfg(not(target_os = "linux"))]
pub fn actual_hard_memory_quota_v1() -> Result<NativeHardMemoryQuotaV1, NativeCoverageV1> {
    Err(NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>(
        "allocator.physical-memory",
        "no implemented OS hard-memory-quota evidence provider on this target; generic/mimalloc physical arena overhead is unknown",
    ))
}
