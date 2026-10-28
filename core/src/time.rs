use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Returns the number of milliseconds since UNIX EPOCH
#[inline]
pub fn unix_now() -> u64 {
    let real = real_unix_now();
    #[cfg(feature = "test-clock")]
    { test_clock::read(real) }
    #[cfg(not(feature = "test-clock"))]
    { real }
}

#[inline]
fn real_unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// A process-local wall clock for isolated native service tests only. Monotonic
/// timers remain real; no command-line or environment value activates this clock.
#[cfg(feature = "test-clock")]
#[doc(hidden)]
pub mod test_clock {
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
    use std::time::Instant;

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static OFFSET_MS: AtomicI64 = AtomicI64::new(0);

    pub struct ClockGuard {
        initial_ms: u64,
        wall: Instant,
    }

    pub fn active() -> bool { ACTIVE.load(Ordering::SeqCst) }

    pub fn start_at(initial_ms: u64) -> ClockGuard {
        assert_eq!(std::env::var("KASPA_TEST_CLOCK_ISOLATED").as_deref(), Ok("1"),
            "the simulated wall clock requires an isolated test process");
        assert!(!ACTIVE.load(Ordering::SeqCst), "only one wall-clock scope per test process");
        let real = super::real_unix_now();
        OFFSET_MS.store(i64::try_from(initial_ms as i128 - real as i128).expect("test clock offset fits"), Ordering::SeqCst);
        ACTIVE.store(true, Ordering::SeqCst);
        eprintln!("[test-clock] isolated simulated wall clock starts at {initial_ms}; monotonic timers remain real");
        ClockGuard { initial_ms, wall: Instant::now() }
    }

    pub(super) fn read(real_ms: u64) -> u64 {
        if ACTIVE.load(Ordering::SeqCst) {
            (real_ms as i128 + OFFSET_MS.load(Ordering::SeqCst) as i128).clamp(0, u64::MAX as i128) as u64
        } else {
            real_ms
        }
    }

    /// Move observation time to the production template's own admitted slot.
    /// Calls outside an explicit scope leave ordinary process time untouched.
    pub fn advance_to(timestamp_ms: u64) {
        if ACTIVE.load(Ordering::SeqCst) {
            let current = super::unix_now();
            if timestamp_ms > current {
                OFFSET_MS.fetch_add(i64::try_from(timestamp_ms - current).expect("test clock step fits"), Ordering::SeqCst);
            }
        }
    }

    impl Drop for ClockGuard {
        fn drop(&mut self) {
            let elapsed_ms = super::unix_now().saturating_sub(self.initial_ms);
            ACTIVE.store(false, Ordering::SeqCst);
            OFFSET_MS.store(0, Ordering::SeqCst);
            eprintln!("[test-clock] simulated elapsed {elapsed_ms}ms; actual monotonic elapsed {:?}; scope released", self.wall.elapsed());
        }
    }
}

/// Stopwatch which reports on drop if the timed operation passed the threshold `TR` in milliseconds
pub struct Stopwatch<const TR: u64 = 1000> {
    name: &'static str,
    start: Instant,
}

impl Stopwatch {
    pub fn new(name: &'static str) -> Self {
        Self { name, start: Instant::now() }
    }
}

impl<const TR: u64> Stopwatch<TR> {
    pub fn with_threshold(name: &'static str) -> Self {
        Self { name, start: Instant::now() }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl<const TR: u64> Drop for Stopwatch<TR> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        if elapsed > Duration::from_millis(TR) {
            kaspa_core::trace!("[{}] Abnormal time: {:#?}", self.name, elapsed);
        }
    }
}
