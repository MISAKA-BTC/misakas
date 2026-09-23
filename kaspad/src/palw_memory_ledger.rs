//! **The node-local memory reservation ledger: estimate → atomic reserve → execute → release**
//! (ADR-0151 follow-up, item 3).
//!
//! # The failure a per-attempt gate cannot see
//!
//! `replay_memory_budget_v1` answers "does this need fit what the host has free NOW". Three duties
//! in one process — a producer's attempt, a panel seat's replay, a court's resume — each ask it,
//! each hear yes, and each start: on a 32 GiB host three 16 GiB starts are 48 GiB and a dead host,
//! because a figure that has been approved but not yet allocated is invisible to `MemAvailable`.
//! The gate was right about every one of them and wrong about all of them together.
//!
//! So a reservation is taken BEFORE the bytes exist and released AFTER they are gone, and the
//! ledger is one per process (one per pool — host memory today, a device's memory when a GPU
//! backend arrives) so that the second duty sees the first. What a duty reserves is the role's
//! [resource profile](kaspa_consensus_core::palw_resource_profile_v1) plus the holding's
//! incremental bytes — the figure `palw_backends::PalwRoleMemoryNeedV1` composes — never a number
//! it measured about itself.
//!
//! # The rule
//!
//! A request of `need` bytes is granted when `need ≤ min(share, live) − reserved`, over the bounds
//! that exist:
//!
//! * `share` is the operator's declared per-node budget (`--palw-host-memory-budget` divided by
//!   `--palw-host-node-count`, `palw_host_share_bytes_v1`) — the number that is right for N nodes
//!   on one host, where the live probe is right for one;
//! * `live` is the host's headroom now. WITHOUT a declared share it is `70 % × (MemAvailable −
//!   1 GiB)`, the policy the gate applied before the ledger existed (`replay_memory_budget_v1`'s
//!   constants) — the haircut stands in for the other processes an undeclared host may run. WITH a
//!   declared share it is `MemAvailable − 1 GiB`: the operator has already divided the host by
//!   role, the share is the budget, and the live figure only guards the shares not summing to what
//!   the host has. The first acceptance run of the fold (2026-09-23 07:41) held a 12.42 GiB
//!   attempt under a 21 GiB share because the haircut bound it at 11.53 GiB of 17.5 available —
//!   a duty that fitted, held by a factor meant for a host nobody had budgeted;
//! * `reserved` is every outstanding reservation in this pool.
//!
//! Subtracting `reserved` from the LIVE bound double-counts the part of an outstanding reservation
//! its duty has already touched (those pages have left `MemAvailable` too). That is deliberate:
//! the error is in the direction of holding a duty that would have fitted, never of starting one
//! that would not, and the duty holds only until the outstanding one releases. When neither bound
//! is known — a platform with no `MemAvailable` and no declared share — every request is granted,
//! which is the node's behaviour before this policy and is said in the snapshot as `available:
//! None`.
//!
//! A reservation is an RAII guard: dropping it — on completion, on an error return, or during a
//! panic's unwind — returns the bytes. A refusal names the need, both bounds, and every
//! reservation already held (role, class, job, bytes), so a hold is diagnosed from the log line
//! and not from `dmesg`.
//!
//! **Node-local, never consensus.** Nothing here is read by the chain; a refusal delays a duty and
//! rejects no block. And capacity is not capability: what a class earns and locks is derived from
//! its work, and this ledger is not an input to any of it.

use kaspa_hashes::Hash64;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Which memory a reservation is against. `Host` is the only pool armed today; a device pool is
/// declared by whichever backend brings one, under the same rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwMemoryPoolV1 {
    Host,
    Device(u8),
}

impl PalwMemoryPoolV1 {
    pub fn name(self) -> String {
        match self {
            PalwMemoryPoolV1::Host => "host".to_string(),
            PalwMemoryPoolV1::Device(i) => format!("device-{i}"),
        }
    }
}

/// Who holds a reservation: the role (`producer`, `full-seat`, `partial-seat`, `court`), the class,
/// and the job (a context hash or a claim id) — the three facts a refusal names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMemoryReservationKeyV1 {
    pub role: &'static str,
    pub class_id: Hash64,
    pub job: Hash64,
}

/// One outstanding reservation, as a snapshot or a refusal lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMemoryReservationRowV1 {
    pub id: u64,
    pub key: PalwMemoryReservationKeyV1,
    pub bytes: u64,
    pub since_unix: u64,
}

/// Why a reservation was refused. Every number a reader needs to see the arithmetic is here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMemoryRefusalV1 {
    pub pool: PalwMemoryPoolV1,
    pub key: Option<PalwMemoryReservationKeyV1>,
    pub need_bytes: u64,
    pub share_bytes: Option<u64>,
    pub live_bytes: Option<u64>,
    pub reserved_bytes: u64,
    pub available_bytes: u64,
    pub held: Vec<PalwMemoryReservationRowV1>,
}

impl std::fmt::Display for PalwMemoryRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the {} memory ledger cannot cover {:.2} GiB: {:.2} GiB available (",
            self.pool.name(),
            gib(self.need_bytes),
            gib(self.available_bytes)
        )?;
        match (self.share_bytes, self.live_bytes) {
            (Some(share), Some(live)) => write!(f, "declared share {:.2} GiB, host headroom {:.2} GiB", gib(share), gib(live))?,
            (Some(share), None) => write!(f, "declared share {:.2} GiB", gib(share))?,
            (None, Some(live)) => write!(f, "host headroom {:.2} GiB", gib(live))?,
            (None, None) => write!(f, "no bound known")?,
        }
        write!(f, ", less {:.2} GiB already reserved", gib(self.reserved_bytes))?;
        if self.held.is_empty() {
            write!(f, ")")
        } else {
            write!(f, " by ")?;
            for (i, row) in self.held.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{} of class {} job {} ({:.2} GiB)", row.key.role, row.key.class_id, row.key.job, gib(row.bytes))?;
            }
            write!(f, ")")
        }
    }
}

/// What the ledger holds right now — the telemetry view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMemoryLedgerSnapshotV1 {
    pub pool: PalwMemoryPoolV1,
    pub share_bytes: Option<u64>,
    pub live_bytes: Option<u64>,
    pub reserved_bytes: u64,
    /// `None` when no bound is known (every request is granted); otherwise what a new request may
    /// still take.
    pub available_bytes: Option<u64>,
    pub rows: Vec<PalwMemoryReservationRowV1>,
}

struct LedgerState {
    next_id: u64,
    rows: Vec<PalwMemoryReservationRowV1>,
}

/// The host's headroom, in the two readings the bound chooses between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHostHeadroomV1 {
    /// `MemAvailable − reserve`: what a node whose operator budgeted the host may take.
    pub past_reserve: u64,
    /// `70 % × (MemAvailable − reserve)`: what a node on an unbudgeted host may take.
    pub haircut: u64,
}

/// One pool's ledger. `live` is the host-headroom probe, injected so a test can hold it fixed and
/// production can read `MemAvailable`.
pub struct PalwMemoryLedgerV1 {
    pool: PalwMemoryPoolV1,
    share: Option<u64>,
    live: Box<dyn Fn() -> Option<PalwHostHeadroomV1> + Send + Sync>,
    state: Mutex<LedgerState>,
}

/// **A held reservation.** Dropping it returns the bytes — on the normal path, on an early error
/// return, and during a panic's unwind, which is the property `a_panic_returns_the_bytes` holds.
#[must_use = "a reservation that is dropped at once reserves nothing — hold it for the duty's life"]
pub struct PalwMemoryReservationV1 {
    ledger: Arc<PalwMemoryLedgerV1>,
    id: u64,
    bytes: u64,
}

impl PalwMemoryReservationV1 {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl std::fmt::Debug for PalwMemoryReservationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalwMemoryReservationV1").field("pool", &self.ledger.pool).field("id", &self.id).field("bytes", &self.bytes).finish()
    }
}

impl Drop for PalwMemoryReservationV1 {
    fn drop(&mut self) {
        self.ledger.release(self.id);
    }
}

impl PalwMemoryLedgerV1 {
    pub fn new(
        pool: PalwMemoryPoolV1,
        share: Option<u64>,
        live: impl Fn() -> Option<PalwHostHeadroomV1> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self { pool, share, live: Box::new(live), state: Mutex::new(LedgerState { next_id: 1, rows: Vec::new() }) })
    }

    pub fn pool(&self) -> PalwMemoryPoolV1 {
        self.pool
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LedgerState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The bounds and the outstanding total, read under the lock so a decision is one atomic read.
    fn bounds(&self, state: &LedgerState) -> (Option<u64>, Option<u64>, u64, Option<u64>) {
        let reserved: u64 = state.rows.iter().fold(0u64, |acc, r| acc.saturating_add(r.bytes));
        let headroom = (self.live)();
        // The live reading the bound uses: past the reserve when the operator budgeted the host,
        // the haircut when nobody did (the module doc has the run that decided this).
        let live = headroom.map(|h| if self.share.is_some() { h.past_reserve } else { h.haircut });
        let bound = match (self.share, live) {
            (Some(share), Some(live)) => Some(share.min(live)),
            (Some(share), None) => Some(share),
            (None, Some(live)) => Some(live),
            (None, None) => None,
        };
        (self.share, live, reserved, bound.map(|b| b.saturating_sub(reserved)))
    }

    /// Whether `need_bytes` could be reserved now — the dry run a pre-check runs before a duty is
    /// even assembled. Grants nothing.
    pub fn can_reserve(&self, need_bytes: u64) -> Result<(), PalwMemoryRefusalV1> {
        let state = self.lock();
        let (share, live, reserved, available) = self.bounds(&state);
        match available {
            Some(available) if need_bytes > available => Err(PalwMemoryRefusalV1 {
                pool: self.pool,
                key: None,
                need_bytes,
                share_bytes: share,
                live_bytes: live,
                reserved_bytes: reserved,
                available_bytes: available,
                held: state.rows.clone(),
            }),
            _ => Ok(()),
        }
    }

    /// **Reserve `need_bytes` for `key`, or be told exactly why not.** Atomic under the ledger's
    /// lock: two duties that race for the last bytes are serialised, the first is granted, the
    /// second is refused naming the first.
    pub fn reserve(self: &Arc<Self>, key: PalwMemoryReservationKeyV1, need_bytes: u64) -> Result<PalwMemoryReservationV1, PalwMemoryRefusalV1> {
        let mut state = self.lock();
        let (share, live, reserved, available) = self.bounds(&state);
        if let Some(available) = available
            && need_bytes > available
        {
            return Err(PalwMemoryRefusalV1 {
                pool: self.pool,
                key: Some(key),
                need_bytes,
                share_bytes: share,
                live_bytes: live,
                reserved_bytes: reserved,
                available_bytes: available,
                held: state.rows.clone(),
            });
        }
        let id = state.next_id;
        state.next_id += 1;
        state.rows.push(PalwMemoryReservationRowV1 { id, key, bytes: need_bytes, since_unix: unix_now_secs() });
        Ok(PalwMemoryReservationV1 { ledger: Arc::clone(self), id, bytes: need_bytes })
    }

    fn release(&self, id: u64) {
        let mut state = self.lock();
        state.rows.retain(|r| r.id != id);
    }

    pub fn snapshot(&self) -> PalwMemoryLedgerSnapshotV1 {
        let state = self.lock();
        let (share, live, reserved, available) = self.bounds(&state);
        PalwMemoryLedgerSnapshotV1 {
            pool: self.pool,
            share_bytes: share,
            live_bytes: live,
            reserved_bytes: reserved,
            available_bytes: available,
            rows: state.rows.clone(),
        }
    }

    pub fn reserved_bytes(&self) -> u64 {
        self.lock().rows.iter().fold(0u64, |acc, r| acc.saturating_add(r.bytes))
    }
}

fn unix_now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

// -------------------------------------------------------------------------------------------------
// The process-wide pools
// -------------------------------------------------------------------------------------------------

struct Pools {
    host: Arc<PalwMemoryLedgerV1>,
    devices: HashMap<u8, Arc<PalwMemoryLedgerV1>>,
}

static POOLS: OnceLock<Mutex<Pools>> = OnceLock::new();

fn pools() -> &'static Mutex<Pools> {
    POOLS.get_or_init(|| {
        Mutex::new(Pools {
            host: PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, crate::palw_backends::host_headroom_v1),
            devices: HashMap::new(),
        })
    })
}

/// **Arm the host pool with the operator's declared share**, once, from the daemon — beside
/// `arm_ram_scale_v1`, before any service can reserve. `None` leaves the live probe as the only
/// bound. Arming twice keeps the first (a service cannot re-budget itself).
pub fn arm_host_share_v1(share: Option<u64>) {
    let mut pools = pools().lock().unwrap_or_else(|p| p.into_inner());
    if pools.host.reserved_bytes() == 0 && pools.host.share.is_none() && share.is_some() {
        pools.host = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, share, crate::palw_backends::host_headroom_v1);
    }
}

/// The host pool's ledger.
pub fn host_ledger_v1() -> Arc<PalwMemoryLedgerV1> {
    Arc::clone(&pools().lock().unwrap_or_else(|p| p.into_inner()).host)
}

/// **Declare a device pool** of `bytes` (a GPU's memory, as the backend that brings it reports
/// it). Declared once; a later declaration keeps the first.
pub fn arm_device_share_v1(index: u8, bytes: u64) {
    let mut pools = pools().lock().unwrap_or_else(|p| p.into_inner());
    pools.devices.entry(index).or_insert_with(|| PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Device(index), Some(bytes), || None));
}

/// A device pool's ledger, or `None` when no backend declared that device.
pub fn device_ledger_v1(index: u8) -> Option<Arc<PalwMemoryLedgerV1>> {
    pools().lock().unwrap_or_else(|p| p.into_inner()).devices.get(&index).map(Arc::clone)
}

/// Every pool's snapshot, host first — the telemetry view.
pub fn ledger_snapshots_v1() -> Vec<PalwMemoryLedgerSnapshotV1> {
    let pools = pools().lock().unwrap_or_else(|p| p.into_inner());
    let mut out = vec![pools.host.snapshot()];
    let mut devices: Vec<_> = pools.devices.iter().collect();
    devices.sort_by_key(|(i, _)| **i);
    out.extend(devices.into_iter().map(|(_, l)| l.snapshot()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn key(role: &'static str, job: u64) -> PalwMemoryReservationKeyV1 {
        PalwMemoryReservationKeyV1 { role, class_id: Hash64::from_u64_word(0xC1A55), job: Hash64::from_u64_word(job) }
    }

    /// **Two duties that each fit alone do not both fit, and the second is told about the
    /// first.** The 32 GiB host of the addendum, cut into one 16 GiB share with no live probe:
    /// producer A reserves 10 GiB, seat B asks for 10 GiB and is refused naming A's 10 GiB; A
    /// releases and B is granted.
    #[test]
    fn a_second_reservation_past_the_share_holds_and_names_the_first() {
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(16 * GIB), || None);
        let a = ledger.reserve(key("producer", 1), 10 * GIB).expect("A fits alone");
        let refused = ledger.reserve(key("full-seat", 2), 10 * GIB).expect_err("B does not fit beside A");
        assert_eq!(refused.need_bytes, 10 * GIB);
        assert_eq!(refused.share_bytes, Some(16 * GIB));
        assert_eq!(refused.live_bytes, None);
        assert_eq!(refused.reserved_bytes, 10 * GIB);
        assert_eq!(refused.available_bytes, 6 * GIB);
        assert_eq!(refused.held.len(), 1);
        assert_eq!(refused.held[0].key, key("producer", 1));
        let text = refused.to_string();
        assert!(text.contains("producer of class") && text.contains("10.00 GiB"), "{text}");
        // The dry run says the same thing without granting anything.
        assert!(ledger.can_reserve(10 * GIB).is_err());
        assert!(ledger.can_reserve(6 * GIB).is_ok());
        assert_eq!(ledger.reserved_bytes(), 10 * GIB, "a dry run reserved nothing");
        drop(a);
        assert_eq!(ledger.reserved_bytes(), 0, "release returned A's bytes");
        let _b = ledger.reserve(key("full-seat", 2), 10 * GIB).expect("B fits once A is gone");
        assert_eq!(ledger.reserved_bytes(), 10 * GIB);
    }

    /// The same race, run as a race: many threads each try to take the whole share at once;
    /// exactly one wins, every other refusal names the winner, and the outcome is the same on
    /// every run.
    #[test]
    fn concurrent_requests_for_the_last_bytes_are_serialised_and_exactly_one_wins() {
        for _round in 0..20 {
            let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(16 * GIB), || None);
            let barrier = Arc::new(std::sync::Barrier::new(8));
            // Each thread hands its GUARD back rather than a number: a guard dropped inside the
            // thread would release the bytes at once and let the next thread win too — which is
            // exactly the double-start the ledger exists to stop, and the first draft of this test
            // did it to itself.
            let handles: Vec<_> = (0..8u64)
                .map(|i| {
                    let (ledger, barrier) = (Arc::clone(&ledger), Arc::clone(&barrier));
                    std::thread::spawn(move || {
                        barrier.wait();
                        ledger.reserve(key("full-seat", i), 12 * GIB)
                    })
                })
                .collect();
            let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().expect("no panic")).collect();
            let won = outcomes.iter().filter(|o| o.is_ok()).count();
            assert_eq!(won, 1, "exactly one of eight takes the share");
            assert_eq!(ledger.reserved_bytes(), 12 * GIB, "and holds it while its guard lives");
            for refusal in outcomes.iter().filter_map(|o| o.as_ref().err()) {
                assert_eq!(refusal.held.len(), 1, "every loser is told who holds it");
                assert_eq!(refusal.reserved_bytes, 12 * GIB);
                assert_eq!(refusal.available_bytes, 4 * GIB);
            }
            drop(outcomes);
            assert_eq!(ledger.reserved_bytes(), 0, "the winner's guard released with the outcomes");
        }
    }

    /// **A panic returns the bytes.** The guard is an RAII value, so a duty that panics mid-run
    /// unwinds through it, and the next duty is not held by a ghost.
    #[test]
    fn a_panic_returns_the_bytes() {
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(16 * GIB), || None);
        let inner = Arc::clone(&ledger);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _held = inner.reserve(key("producer", 7), 12 * GIB).expect("fits");
            assert_eq!(inner.reserved_bytes(), 12 * GIB);
            panic!("the duty died with the reservation in hand");
        }));
        assert!(result.is_err(), "the closure panicked");
        assert_eq!(ledger.reserved_bytes(), 0, "and the unwind released it");
        let _next = ledger.reserve(key("full-seat", 8), 12 * GIB).expect("the next duty is not held by a ghost");
        // An error return releases too.
        let failed: Result<(), String> = (|| {
            let _held = ledger.reserve(key("court", 9), 1 * GIB).map_err(|e| e.to_string())?;
            Err("the replay refused".to_string())
        })();
        assert!(failed.is_err());
        assert_eq!(ledger.reserved_bytes(), 12 * GIB, "only the live guard remains");
    }

    /// The live bound is subtracted like the share: a host with 8 GiB past its reserve and a 16 GiB
    /// share grants 8 GiB, and once 6 are reserved grants 2 — the outstanding reservation is
    /// charged against the live figure too, in the safe direction. And the reading is the share's:
    /// a declared share reads the headroom past the reserve; an undeclared one reads the haircut.
    #[test]
    fn the_tighter_of_share_and_live_headroom_binds_and_reserved_bytes_are_charged_against_both() {
        let probe = || Some(PalwHostHeadroomV1 { past_reserve: 8 * GIB, haircut: 5 * GIB });
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(16 * GIB), probe);
        let snap = ledger.snapshot();
        assert_eq!((snap.share_bytes, snap.live_bytes, snap.available_bytes), (Some(16 * GIB), Some(8 * GIB), Some(8 * GIB)));
        let _a = ledger.reserve(key("producer", 1), 6 * GIB).expect("fits");
        assert_eq!(ledger.snapshot().available_bytes, Some(2 * GIB));
        assert!(ledger.reserve(key("full-seat", 2), 3 * GIB).is_err());
        let _b = ledger.reserve(key("full-seat", 3), 2 * GIB).expect("the last two fit");
        assert_eq!(ledger.snapshot().available_bytes, Some(0));
        // **The 2026-09-23 07:41 hold, replayed**: 17.5 GiB available, a 21 GiB share, a 12.42 GiB
        // attempt. The haircut (11.53) held a duty that fitted; the reading past the reserve (16.5)
        // admits it, and an undeclared host still reads the haircut.
        let fleet = || Some(PalwHostHeadroomV1 { past_reserve: 16_500 << 20, haircut: 11_530 << 20 });
        let stated = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(21 * GIB), fleet);
        assert!(stated.can_reserve(12_420 << 20).is_ok(), "a budgeted host admits the attempt that fits past its reserve");
        assert_eq!(stated.snapshot().live_bytes, Some(16_500 << 20));
        let unstated = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, fleet);
        assert!(unstated.can_reserve(12_420 << 20).is_err(), "an unbudgeted host keeps the haircut");
        assert_eq!(unstated.snapshot().live_bytes, Some(11_530 << 20));
        // No bound at all: everything is granted and the snapshot says so.
        let open = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None);
        assert_eq!(open.snapshot().available_bytes, None);
        let _c = open.reserve(key("producer", 4), 1_000 * GIB).expect("granted: the platform cannot say");
        assert!(open.can_reserve(u64::MAX).is_ok());
    }

    /// A device pool is its own ledger: a host reservation does not consume device bytes and the
    /// other way round.
    #[test]
    fn pools_are_independent() {
        let host = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4 * GIB), || None);
        let device = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Device(0), Some(4 * GIB), || None);
        let _h = host.reserve(key("producer", 1), 4 * GIB).expect("fits");
        let _d = device.reserve(key("producer", 1), 4 * GIB).expect("the device pool is untouched");
        assert!(host.reserve(key("court", 2), 1).is_err());
        assert!(device.reserve(key("court", 2), 1).is_err());
        assert_eq!(device.snapshot().pool, PalwMemoryPoolV1::Device(0));
        assert_eq!(PalwMemoryPoolV1::Device(3).name(), "device-3");
    }
}
