//! Process-local admission for audited retained bytes. Reserve before allocating,
//! carry the lease with the last owner, and reserve simultaneous copies separately.
//! This is an admission bound, not a measurement of allocator RSS.
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum MemoryClass {
    Transport,
    Snapshot,
    Native,
    Cache,
}
const CLASSES: usize = 4;
const MIB: u64 = 1 << 20;

#[derive(Clone, Debug)]
pub struct MemoryBudgetSnapshot {
    pub limit_bytes: u64,
    pub reserved_bytes: u64,
    pub class_bytes: [u64; CLASSES],
    pub pending_bytes: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryBudgetError {
    pub class: MemoryClass,
    pub requested_bytes: u64,
    pub available_bytes: u64,
    pub external_reason: Option<String>,
}
impl fmt::Display for MemoryBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} byte budget refused {} bytes ({} available)", self.class, self.requested_bytes, self.available_bytes)?;
        if let Some(reason) = &self.external_reason {
            write!(f, ": {reason}")?;
        }
        Ok(())
    }
}
impl std::error::Error for MemoryBudgetError {}

#[derive(Debug, Default)]
struct Usage {
    total: u64,
    pending: u64,
    classes: [u64; CLASSES],
}
/// All classes share one total. Class caps are ceilings within it, never extra pools.
pub type ExternalMemoryAccounting = dyn Fn([u64; CLASSES], u64) -> Result<(), String> + Send + Sync;
pub struct MemoryBudget {
    limit: u64,
    class_limits: [u64; CLASSES],
    usage: Mutex<Usage>,
    external: OnceLock<Box<ExternalMemoryAccounting>>,
    headroom: OnceLock<Box<dyn Fn() -> Option<u64> + Send + Sync>>,
}
impl fmt::Debug for MemoryBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.snapshot().fmt(f)
    }
}
impl MemoryBudget {
    pub fn new(limit: u64, class_limits: [u64; CLASSES]) -> Arc<Self> {
        Arc::new(Self {
            limit,
            class_limits,
            usage: Mutex::new(Usage::default()),
            external: OnceLock::new(),
            headroom: OnceLock::new(),
        })
    }
    fn charge(&self, class: MemoryClass, bytes: u64, pending: bool) -> Result<(), MemoryBudgetError> {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        let available = self
            .limit
            .saturating_sub(usage.total)
            .min(self.class_limits[class as usize].saturating_sub(usage.classes[class as usize]));
        // The live probe already excludes resident allocations. Charge only
        // this increment against it; subtracting all retained tickets again
        // can permanently deny tiny requests while their state owners live.
        let available =
            self.headroom.get().and_then(|probe| probe()).map_or(available, |live| available.min(live.saturating_sub(usage.pending)));
        if bytes > available {
            return Err(MemoryBudgetError { class, requested_bytes: bytes, available_bytes: available, external_reason: None });
        }
        let mut next_classes = usage.classes;
        next_classes[class as usize] += bytes;
        if let Some(external) = self.external.get() {
            external(next_classes, usage.pending + if pending { bytes } else { 0 }).map_err(|reason| MemoryBudgetError {
                class,
                requested_bytes: bytes,
                available_bytes: 0,
                external_reason: Some(reason),
            })?;
        }
        usage.total += bytes;
        if pending {
            usage.pending += bytes;
        }
        usage.classes = next_classes;
        Ok(())
    }
    pub fn set_headroom_probe(&self, probe: Box<dyn Fn() -> Option<u64> + Send + Sync>) -> Result<(), String> {
        self.headroom.set(probe).map_err(|_| "byte budget headroom probe is already installed".to_string())
    }
    /// Install a host-accounting bridge once, before services start. The hook
    /// atomically replaces this bank's aggregate, rather than appending a second
    /// reservation for clones, splits or transfers. Never call back into this bank.
    pub fn set_external_accounting(&self, hook: Box<ExternalMemoryAccounting>) -> Result<(), String> {
        let usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        if self.external.get().is_some() {
            return Err("byte budget host accounting is already installed".into());
        }
        hook(usage.classes, usage.pending)?;
        self.external.set(hook).map_err(|_| "byte budget host accounting raced startup".to_string())
    }
    /// Reconcile a previously failed host release/phase update. A daemon can
    /// retry this periodically without retaining allocation owners or payloads.
    pub fn refresh_external_accounting(&self) -> Result<(), String> {
        let usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        self.external.get().map_or(Ok(()), |hook| hook(usage.classes, usage.pending))
    }
    pub fn try_reserve(self: &Arc<Self>, class: MemoryClass, bytes: u64) -> Result<MemoryLease, MemoryBudgetError> {
        self.reserve_phase(class, bytes, true)
    }
    /// Charge a known retained allocation whose construction boundary is not
    /// represented by this lease. Prefer try_reserve before new construction.
    pub fn try_reserve_retained(self: &Arc<Self>, class: MemoryClass, bytes: u64) -> Result<MemoryLease, MemoryBudgetError> {
        self.reserve_phase(class, bytes, false)
    }
    fn reserve_phase(self: &Arc<Self>, class: MemoryClass, bytes: u64, pending: bool) -> Result<MemoryLease, MemoryBudgetError> {
        self.charge(class, bytes, pending)?;
        Ok(MemoryLease(Arc::new(Reservation {
            budget: Arc::clone(self),
            class,
            bytes,
            pending: AtomicU64::new(if pending { bytes } else { 0 }),
        })))
    }
    pub fn snapshot(&self) -> MemoryBudgetSnapshot {
        let usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        MemoryBudgetSnapshot {
            limit_bytes: self.limit,
            reserved_bytes: usage.total,
            class_bytes: usage.classes,
            pending_bytes: usage.pending,
        }
    }
}
#[derive(Debug)]
struct Reservation {
    budget: Arc<MemoryBudget>,
    class: MemoryClass,
    bytes: u64,
    pending: AtomicU64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let mut usage = self.budget.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.total -= self.bytes;
        usage.pending -= self.pending.load(Ordering::Relaxed);
        usage.classes[self.class as usize] -= self.bytes;
        if let Some(external) = self.budget.external.get() {
            // A failed release may leave a conservative host row. The next
            // mutation replaces that row, including reductions, so it cannot
            // require the stale row's bytes in order to make progress.
            let _ = external(usage.classes, usage.pending);
        }
    }
}
/// Clones share a reservation: bytes return only when the last clone is dropped.
#[must_use = "keep this lease until the corresponding allocation's last owner is gone"]
#[derive(Clone, Debug)]
pub struct MemoryLease(Arc<Reservation>);
impl MemoryLease {
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }
    /// Call after the reserved allocation exists. Retention remains charged,
    /// while only not-yet-materialized promises subtract from live headroom.
    /// Clones share this phase transition; repeated calls have no effect.
    pub fn materialized(&self) {
        let mut usage = self.0.budget.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.pending -= self.0.pending.swap(0, Ordering::Relaxed);
        if let Some(external) = self.0.budget.external.get() {
            let _ = external(usage.classes, usage.pending);
        }
    }
    /// Split unique ownership without charging either budget a second time.
    pub fn split(&mut self, bytes: u64) -> Result<Self, MemoryBudgetError> {
        let class = self.0.class;
        let available = if Arc::strong_count(&self.0) == 1 { self.0.bytes } else { 0 };
        if bytes > available || Arc::strong_count(&self.0) != 1 {
            return Err(MemoryBudgetError { class, requested_bytes: bytes, available_bytes: available, external_reason: None });
        }
        let source = Arc::get_mut(&mut self.0).expect("unique lease checked");
        source.bytes -= bytes;
        let pending = source.pending.load(Ordering::Relaxed).min(bytes);
        source.pending.fetch_sub(pending, Ordering::Relaxed);
        Ok(Self(Arc::new(Reservation { budget: Arc::clone(&source.budget), class, bytes, pending: AtomicU64::new(pending) })))
    }
    /// Consolidate unique tickets from the same bank/class without reserving
    /// the transferred bytes again. Refusal returns the original ticket.
    pub fn merge(&mut self, mut other: Self) -> Result<(), Self> {
        if Arc::strong_count(&self.0) != 1
            || Arc::strong_count(&other.0) != 1
            || !Arc::ptr_eq(&self.0.budget, &other.0.budget)
            || self.0.class != other.0.class
        {
            return Err(other);
        }
        let source = Arc::get_mut(&mut other.0).expect("unique merge source");
        let destination = Arc::get_mut(&mut self.0).expect("unique merge target");
        destination.bytes += source.bytes;
        destination.pending.fetch_add(source.pending.swap(0, Ordering::Relaxed), Ordering::Relaxed);
        source.bytes = 0;
        Ok(())
    }
    /// Increase a uniquely owned reservation before growing its allocation.
    /// Shared leases must reserve a distinct simultaneous allocation instead.
    pub fn grow(&mut self, additional: u64) -> Result<(), MemoryBudgetError> {
        let class = self.0.class;
        let Some(source) = Arc::get_mut(&mut self.0) else {
            return Err(MemoryBudgetError { class, requested_bytes: additional, available_bytes: 0, external_reason: None });
        };
        source.budget.charge(class, additional, true)?;
        source.bytes += additional;
        source.pending.fetch_add(additional, Ordering::Relaxed);
        Ok(())
    }
}

/// Effective capacity includes ancestor cgroups, including a limit at the mount
/// root. It never treats swap as RAM. Off Linux use a finite conservative default.
fn effective_capacity() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        capacity_from(
            &std::fs::read_to_string("/proc/meminfo").ok()?,
            &std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default(),
            |path| std::fs::read_to_string(path).ok(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
#[cfg(any(target_os = "linux", test))]
fn capacity_from(meminfo: &str, membership: &str, read: impl Fn(&std::path::Path) -> Option<String>) -> Option<u64> {
    use std::path::{Component, Path};
    let mut limit = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:")?.split_whitespace().next()?.parse::<u64>().ok()?.checked_mul(1024));
    if let Some(group) = membership.lines().find_map(|line| line.strip_prefix("0::")) {
        let relative = Path::new(group.trim_start_matches('/'));
        if !relative.components().any(|p| matches!(p, Component::ParentDir)) {
            let root = Path::new("/sys/fs/cgroup");
            let mut path = root.join(relative);
            loop {
                if let Some(max) = read(&path.join("memory.max")).and_then(|s| s.trim().parse::<u64>().ok()) {
                    limit = Some(limit.map_or(max, |l| l.min(max)));
                }
                if path == root || !path.pop() {
                    break;
                }
            }
        }
    }
    limit
}

/// Default total: 70% of effective physical capacity. On an unknown platform use
/// 2 GiB. Operators can lower the total via MISAKA_MEMORY_BUDGET_BYTES; the
/// environment cannot raise it above the capacity-derived bound. Leave 30% for
/// unaudited node state, allocator overhead, consensus and other services.
pub fn global_memory_budget() -> &'static Arc<MemoryBudget> {
    static GLOBAL: OnceLock<Arc<MemoryBudget>> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        let derived = effective_capacity().map(|n| n / 10 * 7).unwrap_or(2 << 30);
        let total =
            std::env::var("MISAKA_MEMORY_BUDGET_BYTES").ok().and_then(|s| s.parse::<u64>().ok()).map_or(derived, |n| n.min(derived));
        // Transport can carry a legacy 320 MiB message with simultaneous codec
        // copies. All classes still compete for the same total.
        MemoryBudget::new(
            total,
            [
                total.min(1536 * MIB),
                total.saturating_sub((total / 8).min(128 * MIB)),
                total.saturating_sub((total / 8).min(128 * MIB)),
                total.min(512 * MIB),
            ],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classes_compete_and_last_owner_releases() {
        let b = MemoryBudget::new(100, [80, 100, 100, 100]);
        let a = b.try_reserve(MemoryClass::Transport, 70).unwrap();
        let copied_owner = a.clone();
        assert!(b.try_reserve(MemoryClass::Transport, 11).is_err());
        assert!(b.try_reserve(MemoryClass::Native, 31).is_err());
        drop(a);
        assert_eq!(b.snapshot().reserved_bytes, 70);
        drop(copied_owner);
        assert!(b.try_reserve(MemoryClass::Native, 100).is_ok());
        assert_eq!(b.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn transfer_growth_failure_and_progress() {
        let b = MemoryBudget::new(100, [100; 4]);
        let mut owner = b.try_reserve(MemoryClass::Native, 80).unwrap();
        let mut split = owner.split(50).unwrap();
        assert_eq!(b.snapshot().reserved_bytes, 80);
        assert!(split.grow(21).is_err());
        assert_eq!(split.bytes(), 50);
        drop(owner);
        split.grow(50).unwrap();
        assert_eq!(b.snapshot().reserved_bytes, 100);
        drop(split);
        assert!(b.try_reserve(MemoryClass::Snapshot, 100).is_ok());
        assert!(b.try_reserve(MemoryClass::Native, u64::MAX).is_err());
    }
    #[test]
    fn shared_lease_cannot_split_or_grow() {
        let b = MemoryBudget::new(100, [100; 4]);
        let mut a = b.try_reserve(MemoryClass::Native, 80).unwrap();
        let copy = a.clone();
        assert!(a.split(10).is_err());
        assert!(a.grow(10).is_err());
        drop(copy);
        assert!(a.split(10).is_ok());
    }
    #[test]
    fn live_headroom_counts_only_pending_promises_and_materialization_makes_progress() {
        let live = Arc::new(AtomicU64::new(100));
        let b = MemoryBudget::new(1000, [1000; 4]);
        let probe = live.clone();
        b.set_headroom_probe(Box::new(move || Some(probe.load(Ordering::SeqCst)))).unwrap();
        let first = b.try_reserve(MemoryClass::Native, 80).unwrap();
        assert!(b.try_reserve(MemoryClass::Snapshot, 30).is_err(), "same free bytes cannot be promised twice");
        live.store(20, Ordering::SeqCst); // the first allocation now appears in actual usage
        first.materialized();
        first.materialized();
        assert_eq!(b.snapshot().pending_bytes, 0);
        let small = b.try_reserve(MemoryClass::Transport, 10).unwrap();
        assert_eq!(b.snapshot().reserved_bytes, 90);
        assert_eq!(b.snapshot().pending_bytes, 10);
        drop(small);
        drop(first);
        assert_eq!(b.snapshot().pending_bytes, 0);
        assert!(b.try_reserve(MemoryClass::Transport, 20).is_ok());
    }
    #[test]
    fn host_hook_is_atomic_and_transfer_does_not_duplicate_bytes() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let host = Arc::new(AtomicU64::new(0));
        let failure = Arc::new(AtomicBool::new(false));
        let b = MemoryBudget::new(100, [100; 4]);
        let h = host.clone();
        let fail = failure.clone();
        b.set_external_accounting(Box::new(move |classes, _pending| {
            if fail.load(Ordering::SeqCst) {
                return Err("simulated host I/O failure".into());
            }
            h.store(classes.iter().sum(), Ordering::SeqCst);
            Ok(())
        }))
        .unwrap();
        let mut owner = b.try_reserve(MemoryClass::Native, 80).unwrap();
        let part = owner.split(30).unwrap();
        assert_eq!(host.load(Ordering::SeqCst), 80);
        owner.merge(part).unwrap();
        failure.store(true, Ordering::SeqCst);
        assert!(owner.grow(1).is_err());
        assert_eq!(owner.bytes(), 80);
        assert_eq!(b.snapshot().reserved_bytes, 80);
        drop(owner); // failed release leaves a stale external row
        assert_eq!(b.snapshot().reserved_bytes, 0);
        failure.store(false, Ordering::SeqCst);
        b.refresh_external_accounting().unwrap();
        assert_eq!(host.load(Ordering::SeqCst), 0, "a failed release is repairable without a new allocation");
        let next = b.try_reserve(MemoryClass::Transport, 10).unwrap();
        assert_eq!(host.load(Ordering::SeqCst), 10, "replaces stale row rather than appending");
        drop(next);
        assert_eq!(host.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn root_and_ancestor_limits_are_effective() {
        let cap = capacity_from("MemTotal: 16000 kB", "0::/a/b", |p| match p.to_str().unwrap() {
            "/sys/fs/cgroup/a/b/memory.max" => Some("max".into()),
            "/sys/fs/cgroup/a/memory.max" => Some("6000000".into()),
            "/sys/fs/cgroup/memory.max" => Some("4000000".into()),
            _ => None,
        });
        assert_eq!(cap, Some(4000000));
    }
    #[test]
    fn concurrent_admission_is_atomic_and_returns_capacity() {
        let b = MemoryBudget::new(100, [100; 4]);
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let b = b.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    b.try_reserve(MemoryClass::Native, 80)
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        drop(results);
        assert_eq!(b.snapshot().reserved_bytes, 0);
        assert!(b.try_reserve(MemoryClass::Transport, 100).is_ok());
    }
}
