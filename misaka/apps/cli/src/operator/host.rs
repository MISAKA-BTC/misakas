//! **The host facts a miner fails on** — ADR-0122 §6.3: disk against the retention janitor's own
//! floor, memory, a background upgrader, the clock. Each answer is `None` where this platform or
//! this user cannot read it, and the doctor then says it was not checked rather than that it passed.

use std::path::Path;

/// The free and total bytes of the volume holding `path` — the deepest mount point containing it,
/// the same walk the janitor does (`volume_space_v1` in `kaspad/src/palw_retention.rs`).
pub(crate) fn volume_space(path: &Path) -> Option<(u64, u64)> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) if parent != probe => probe = parent.to_path_buf(),
            _ => break,
        }
    }
    let probe = probe.canonicalize().unwrap_or(probe);
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if probe.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.is_none_or(|(l, _, _)| len > l) {
                best = Some((len, disk.available_space(), disk.total_space()));
            }
        }
    }
    best.map(|(_, available, total)| (available, total))
}

/// **The janitor's floor, spelled once more**: `max(8 GiB, 5 % of the volume)`
/// (`retention_reserve_bytes_v1`). The doctor checks the operator's disk against the number the
/// node itself will defend, so the two cannot disagree about what "enough space" is.
pub(crate) fn retention_floor_bytes(total_bytes: u64) -> u64 {
    const MIN_FREE: u64 = 8 << 30;
    const PERMILLE: u64 = 50;
    MIN_FREE.max(total_bytes / 1000 * PERMILLE)
}

/// Bytes and files under the retention directory. `None` when it does not exist.
pub(crate) fn dir_usage(dir: &Path) -> Option<(u64, u64)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let (mut bytes, mut files) = (0u64, 0u64);
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                bytes = bytes.saturating_add(meta.len());
                files += 1;
            } else if meta.is_dir()
                && let Some((b, f)) = dir_usage(&entry.path())
            {
                bytes = bytes.saturating_add(b);
                files += f;
            }
        }
    }
    Some((bytes, files))
}

/// The memory the kernel says a new allocation can have without swapping (`MemAvailable`).
pub(crate) fn mem_available() -> Option<u64> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let v = sys.available_memory();
    (v > 0).then_some(v)
}

/// **Is a background upgrader enabled?** Debian and Ubuntu's `unattended-upgrades` has restarted
/// and OOM-killed fleet nodes. `None` off Linux, or where apt is not the package manager.
pub(crate) fn unattended_upgrades_enabled() -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let periodic = std::fs::read_to_string("/etc/apt/apt.conf.d/20auto-upgrades").ok()?;
    Some(apt_periodic_enabled(&periodic))
}

/// `APT::Periodic::Unattended-Upgrade "1";` switches the upgrader on; `"0"` or its absence off.
fn apt_periodic_enabled(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
        .any(|l| l.starts_with("APT::Periodic::Unattended-Upgrade") && l.split('"').nth(1).is_some_and(|v| v.trim() != "0"))
}

/// **Is the clock NTP-synchronised?** From `timedatectl` on a systemd host; `None` elsewhere.
pub(crate) fn ntp_synchronized() -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let out = std::process::Command::new("timedatectl").args(["show", "-p", "NTPSynchronized", "--value"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    match String::from_utf8_lossy(&out.stdout).trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// A path as a person reads it: the home directory as `~`.
pub(crate) fn tilde(path: &Path) -> String {
    match dirs::home_dir() {
        Some(home) if path.starts_with(&home) => {
            format!("~/{}", path.strip_prefix(&home).map(|p| p.display().to_string()).unwrap_or_default())
        }
        _ => path.display().to_string(),
    }
}

/// `1.7 GiB`, `412 GiB`, `830 MiB`: binary units, one decimal under ten.
pub(crate) fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{b} B")
    } else if v < 10.0 {
        format!("{v:.1} {}", UNITS[unit])
    } else {
        format!("{v:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The janitor's floor: 8 GiB until 5 % of the volume is more.
    #[test]
    fn the_floor_is_eight_gib_or_five_percent() {
        assert_eq!(retention_floor_bytes(100 << 30), 8 << 30);
        assert_eq!(retention_floor_bytes(1000 << 30), (1000u64 << 30) / 1000 * 50);
    }

    #[test]
    fn apt_periodic_is_read_as_apt_reads_it() {
        assert!(apt_periodic_enabled("APT::Periodic::Update-Package-Lists \"1\";\nAPT::Periodic::Unattended-Upgrade \"1\";\n"));
        assert!(!apt_periodic_enabled("APT::Periodic::Unattended-Upgrade \"0\";\n"));
        assert!(!apt_periodic_enabled("// APT::Periodic::Unattended-Upgrade \"1\";\n"));
        assert!(!apt_periodic_enabled(""));
    }

    #[test]
    fn bytes_read_as_an_operator_reads_them() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1_825_361_101), "1.7 GiB");
        assert_eq!(human_bytes(412 << 30), "412 GiB");
    }
}
