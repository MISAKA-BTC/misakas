//! Module with structs for supporting discrete event simulation in virtual time.
//! Inspired by python's simpy library.
//!
//! Users should define the message type `T` required for the simulation, derive `Process<T>` with
//! various simulation actor logic and plug the processes into a `Simulation<T>` instance.
//!
//! Determinism contract: given identical process logic and RNG seeds, two runs produce identical
//! event sequences. Events are totally ordered by `(timestamp, seq)` where `seq` is a monotonic
//! counter stamped at scheduling time, so same-timestamp events are delivered in scheduling order
//! (FIFO) rather than in unspecified heap order.

use std::collections::{BinaryHeap, HashMap, HashSet};

/// Internal structure representing a scheduled simulator event
struct Event<T> {
    timestamp: u64,
    /// Monotonic scheduling counter — the tiebreaker making the event order total (FIFO among
    /// same-timestamp events) and thus deterministic.
    seq: u64,
    dest: u64,
    msg: Option<T>,
}

impl<T> Event<T> {
    pub fn new(timestamp: u64, seq: u64, dest: u64, msg: Option<T>) -> Self {
        Self { timestamp, seq, dest, msg }
    }
}

impl<T> PartialEq for Event<T> {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp && self.seq == other.seq
    }
}

impl<T> Eq for Event<T> {}

impl<T> PartialOrd for Event<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Event<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reversing so that min timestamp is scheduled first, FIFO within a timestamp
        other.timestamp.cmp(&self.timestamp).then_with(|| other.seq.cmp(&self.seq))
    }
}

/// What happens to a message crossing a partition cut while the cut is active
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartitionMode {
    /// The message is lost — models relay severance with no recovery (receivers needing the
    /// data must obtain it via application-level catch-up logic)
    Drop,
    /// The message is delivered at `window.end + link delay` — models relay resuming after the
    /// cut heals, without requiring the harness to implement catch-up. Note that the healed
    /// delivery time is NOT re-checked against later partition windows: a message deferred out
    /// of one window is delivered even if another cut is active at that moment.
    DelayUntilHeal,
}

/// A time-windowed network partition: while `start <= now < end` (virtual time), messages between
/// processes belonging to different groups are subject to `mode`. A process not listed in any
/// group is unrestricted (treated as connected to everyone). Windows are evaluated in insertion
/// order and the first window containing the send time that separates the pair applies.
#[derive(Clone, Debug)]
pub struct PartitionWindow {
    pub start: u64,
    pub end: u64,
    pub groups: Vec<HashSet<u64>>,
    pub mode: PartitionMode,
}

impl PartitionWindow {
    /// Returns whether `a` and `b` are on different sides of this cut
    fn separates(&self, a: u64, b: u64) -> bool {
        let ga = self.groups.iter().position(|g| g.contains(&a));
        let gb = self.groups.iter().position(|g| g.contains(&b));
        match (ga, gb) {
            (Some(x), Some(y)) => x != y,
            _ => false, // unlisted processes are unrestricted
        }
    }
}

/// Per-link delivery model: per-(src, dst) delay overrides over a default delay, plus
/// time-windowed partitions. Self-delivery (src == dst) is instantaneous — a process knows its
/// own broadcast immediately.
#[derive(Clone, Debug, Default)]
pub struct Topology {
    default_delay: u64,
    link_delays: HashMap<(u64, u64), u64>,
    partitions: Vec<PartitionWindow>,
}

impl Topology {
    pub fn new(default_delay: u64) -> Self {
        Self { default_delay, link_delays: HashMap::new(), partitions: Vec::new() }
    }

    /// Overrides the delay of the directed link `src -> dst`
    pub fn with_link_delay(mut self, src: u64, dst: u64, delay: u64) -> Self {
        self.link_delays.insert((src, dst), delay);
        self
    }

    /// Adds a partition window (windows are evaluated in insertion order)
    pub fn with_partition(mut self, window: PartitionWindow) -> Self {
        // A single-group window can never separate anything — catch the "cut the rest off"
        // misreading (unlisted processes are unrestricted, not isolated)
        debug_assert!(window.groups.len() >= 2, "a partition window needs at least two groups to separate anything");
        self.partitions.push(window);
        self
    }

    fn link_delay(&self, src: u64, dst: u64) -> u64 {
        if src == dst { 0 } else { *self.link_delays.get(&(src, dst)).unwrap_or(&self.default_delay) }
    }

    /// Resolves delivery of a message sent `src -> dst` at time `now`: `Some(delivery time)` or
    /// `None` if the message is dropped by an active partition cut
    fn deliver_at(&self, now: u64, src: u64, dst: u64) -> Option<u64> {
        let delay = self.link_delay(src, dst);
        match self.partitions.iter().find(|w| w.start <= now && now < w.end && w.separates(src, dst)) {
            None => Some(now + delay),
            Some(w) => match w.mode {
                PartitionMode::Drop => None,
                // saturating: an `end = u64::MAX` (permanent cut) window must not overflow
                PartitionMode::DelayUntilHeal => Some(w.end.saturating_add(delay)),
            },
        }
    }
}

/// Process resumption trigger
pub enum Resumption<T> {
    Initial,
    Scheduled,
    Message(T),
}

/// Process suspension reason
pub enum Suspension {
    Timeout(u64),
    Idle,
    Halt, // Halt the simulation
}

/// A simulation process
pub trait Process<T> {
    fn resume(&mut self, resumption: Resumption<T>, env: &mut Environment<T>) -> Suspension;
}

pub type BoxedProcess<T> = Box<dyn Process<T>>;

/// The simulation environment
#[derive(Default)]
pub struct Environment<T> {
    now: u64,
    broadcast_delay: u64,
    event_queue: BinaryHeap<Event<T>>,
    process_ids: HashSet<u64>,
    next_seq: u64,
    topology: Option<Topology>,
}

impl<T: Clone> Environment<T> {
    pub fn new(delay: u64) -> Self {
        Self::with_start_time(delay, 0)
    }

    pub fn with_start_time(delay: u64, start_time: u64) -> Self {
        Self {
            now: start_time,
            broadcast_delay: delay,
            event_queue: BinaryHeap::new(),
            process_ids: HashSet::new(),
            next_seq: 0,
            topology: None,
        }
    }

    /// Installs a per-link topology. When set, `broadcast`/`unicast` route through it (link
    /// delays, partition cuts, instantaneous self-delivery) instead of the uniform
    /// `broadcast_delay`. `send`/`timeout` are unaffected — they are explicit scheduling.
    ///
    /// Semantics change vs. the legacy (no-topology) path: legacy `broadcast` delivers to the
    /// sender itself after `broadcast_delay` like everyone else, while a topology delivers the
    /// sender's own message instantly (`src == dst` ⇒ delay 0). A process migrated onto a
    /// topology therefore hears itself earlier than before.
    pub fn set_topology(&mut self, topology: Topology) {
        self.topology = Some(topology);
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    fn push_event(&mut self, timestamp: u64, dest: u64, msg: Option<T>) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.event_queue.push(Event::new(timestamp, seq, dest, msg))
    }

    pub fn send(&mut self, delay: u64, dest: u64, msg: T) {
        self.push_event(self.now + delay, dest, Some(msg))
    }

    pub fn timeout(&mut self, timeout: u64, dest: u64) {
        self.push_event(self.now + timeout, dest, None)
    }

    pub fn broadcast(&mut self, sender: u64, msg: T) {
        // Iterate in sorted order so the seq stamping (and hence delivery order among
        // same-timestamp arrivals) is independent of HashSet iteration order
        let mut ids = self.process_ids.iter().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        match self.topology.take() {
            None => {
                for id in ids {
                    self.push_event(self.now + self.broadcast_delay, id, Some(msg.clone()));
                }
            }
            Some(topology) => {
                for id in ids {
                    if let Some(at) = topology.deliver_at(self.now, sender, id) {
                        self.push_event(at, id, Some(msg.clone()));
                    }
                }
                self.topology = Some(topology);
            }
        }
    }

    /// Sends `msg` over the (topology-routed, if set) link `sender -> dest`
    pub fn unicast(&mut self, sender: u64, dest: u64, msg: T) {
        match self.topology.take() {
            None => self.push_event(self.now + self.broadcast_delay, dest, Some(msg)),
            Some(topology) => {
                if let Some(at) = topology.deliver_at(self.now, sender, dest) {
                    self.push_event(at, dest, Some(msg));
                }
                self.topology = Some(topology);
            }
        }
    }

    fn next_event(&mut self) -> Option<Event<T>> {
        let event = self.event_queue.pop()?;
        self.now = event.timestamp;
        Some(event)
    }
}

/// The simulation manager
#[derive(Default)]
pub struct Simulation<T> {
    env: Environment<T>,
    processes: HashMap<u64, BoxedProcess<T>>,
}

impl<T: Clone> Simulation<T> {
    pub fn new(delay: u64) -> Self {
        Self { env: Environment::new(delay), processes: HashMap::new() }
    }

    pub fn with_start_time(delay: u64, start_time: u64) -> Self {
        Self { env: Environment::with_start_time(delay, start_time), processes: HashMap::new() }
    }

    /// See [`Environment::set_topology`]
    pub fn set_topology(&mut self, topology: Topology) {
        self.env.set_topology(topology);
    }

    pub fn register(&mut self, id: u64, process: BoxedProcess<T>) {
        self.processes.insert(id, process);
        self.env.process_ids.insert(id);
    }

    pub fn step(&mut self) -> bool {
        let Some(event) = self.env.next_event() else {
            return false; // Queue exhausted — the simulation is over
        };
        let process = self.processes.get_mut(&event.dest).unwrap();
        let op = if let Some(msg) = event.msg { Resumption::Message(msg) } else { Resumption::Scheduled };
        match process.resume(op, &mut self.env) {
            Suspension::Timeout(timeout) => {
                self.env.timeout(timeout, event.dest);
                true
            }
            Suspension::Idle => true,
            Suspension::Halt => false,
        }
    }

    pub fn run(&mut self, until: u64) {
        // Sorted so startup scheduling order (and seq stamps) are deterministic
        let mut ids = self.processes.keys().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        for id in ids {
            match self.processes.get_mut(&id).unwrap().resume(Resumption::Initial, &mut self.env) {
                Suspension::Timeout(timeout) => self.env.timeout(timeout, id),
                Suspension::Idle => {}
                Suspension::Halt => panic!("not expecting halt on startup"),
            }
        }

        while self.step() {
            if self.env.now() > until {
                break;
            }
        }
        self.processes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Records every message it receives as `(virtual time, payload)`
    struct Recorder {
        log: Arc<Mutex<Vec<(u64, u64)>>>,
    }

    impl Process<u64> for Recorder {
        fn resume(&mut self, resumption: Resumption<u64>, env: &mut Environment<u64>) -> Suspension {
            if let Resumption::Message(msg) = resumption {
                self.log.lock().unwrap().push((env.now(), msg));
            }
            Suspension::Idle
        }
    }

    /// Broadcasts the payloads in `script`, one per `interval` tick, then goes idle
    struct Scripted {
        id: u64,
        script: Vec<u64>,
        interval: u64,
        cursor: usize,
    }

    impl Process<u64> for Scripted {
        fn resume(&mut self, resumption: Resumption<u64>, env: &mut Environment<u64>) -> Suspension {
            match resumption {
                Resumption::Initial => Suspension::Timeout(self.interval),
                Resumption::Scheduled => {
                    if self.cursor < self.script.len() {
                        env.broadcast(self.id, self.script[self.cursor]);
                        self.cursor += 1;
                        Suspension::Timeout(self.interval)
                    } else {
                        Suspension::Idle
                    }
                }
                Resumption::Message(_) => Suspension::Idle,
            }
        }
    }

    fn recorder(log: &Arc<Mutex<Vec<(u64, u64)>>>) -> BoxedProcess<u64> {
        Box::new(Recorder { log: log.clone() })
    }

    fn run_same_timestamp_case() -> Vec<(u64, u64)> {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut sim: Simulation<u64> = Simulation::new(5);
        // Two senders broadcasting at the same virtual time: their deliveries to the recorder
        // share a timestamp, so ordering is decided purely by the seq tiebreaker
        sim.register(0, Box::new(Scripted { id: 0, script: vec![100, 101], interval: 10, cursor: 0 }));
        sim.register(1, Box::new(Scripted { id: 1, script: vec![200, 201], interval: 10, cursor: 0 }));
        sim.register(2, recorder(&log));
        sim.run(1000);
        let out = log.lock().unwrap().clone();
        out
    }

    #[test]
    fn test_same_timestamp_fifo_and_repeatability() {
        let first = run_same_timestamp_case();
        // Both senders fire at t=10 and t=20; the recorder receives at t=15 and t=25. Within a
        // timestamp, delivery follows scheduling order: process 0's timeout was stamped before
        // process 1's (sorted startup), so its broadcast is scheduled — and delivered — first.
        assert_eq!(first, vec![(15, 100), (15, 200), (25, 101), (25, 201)]);
        // Determinism: identical runs yield the identical sequence
        for _ in 0..10 {
            assert_eq!(run_same_timestamp_case(), first);
        }
    }

    #[test]
    fn test_link_delay_override_and_self_delivery() {
        let log01 = Arc::new(Mutex::new(Vec::new()));
        let log2 = Arc::new(Mutex::new(Vec::new()));

        /// Broadcasts once at t=10, and records everything it receives (its own echo included)
        struct SelfLogger {
            log: Arc<Mutex<Vec<(u64, u64)>>>,
            fired: bool,
        }
        impl Process<u64> for SelfLogger {
            fn resume(&mut self, resumption: Resumption<u64>, env: &mut Environment<u64>) -> Suspension {
                match resumption {
                    Resumption::Initial => Suspension::Timeout(10),
                    Resumption::Scheduled => {
                        if !self.fired {
                            self.fired = true;
                            env.broadcast(1, 7);
                        }
                        Suspension::Idle
                    }
                    Resumption::Message(msg) => {
                        self.log.lock().unwrap().push((env.now(), msg));
                        Suspension::Idle
                    }
                }
            }
        }

        let mut sim: Simulation<u64> = Simulation::new(5);
        sim.set_topology(Topology::new(5).with_link_delay(1, 2, 42));
        // log01 is shared by processes 0 and 1 — entries are distinguished by arrival time
        sim.register(0, recorder(&log01));
        sim.register(1, Box::new(SelfLogger { log: log01.clone(), fired: false }));
        sim.register(2, recorder(&log2));
        sim.run(1000);
        // The sender (id 1) hears itself instantly at t=10; id 0 at the default delay, t=15
        assert_eq!(log01.lock().unwrap().clone(), vec![(10, 7), (15, 7)]);
        // id 2 at the overridden 1->2 link delay, t=52
        assert_eq!(log2.lock().unwrap().clone(), vec![(52, 7)]);
    }

    fn partition_case(mode: PartitionMode) -> (Vec<(u64, u64)>, Vec<(u64, u64)>) {
        let log1 = Arc::new(Mutex::new(Vec::new()));
        let log2 = Arc::new(Mutex::new(Vec::new()));
        let mut sim: Simulation<u64> = Simulation::new(5);
        sim.set_topology(Topology::new(5).with_partition(PartitionWindow {
            start: 0,
            end: 100,
            groups: vec![[0u64].into_iter().collect(), [2u64].into_iter().collect()],
            mode,
        }));
        // Sender 0 fires at t=10 and t=20 — both inside the [0, 100) cut
        sim.register(0, Box::new(Scripted { id: 0, script: vec![1, 2], interval: 10, cursor: 0 }));
        sim.register(1, recorder(&log1)); // unlisted — unrestricted
        sim.register(2, recorder(&log2)); // cut off from 0 until t=100
        sim.run(1000);
        let a = log1.lock().unwrap().clone();
        let b = log2.lock().unwrap().clone();
        (a, b)
    }

    #[test]
    fn test_partition_drop() {
        let (unrestricted, cut) = partition_case(PartitionMode::Drop);
        // The unlisted process hears everything at the default delay
        assert_eq!(unrestricted, vec![(15, 1), (25, 2)]);
        // Both sends fall inside the window: the cut process misses them entirely
        assert_eq!(cut, vec![]);
    }

    #[test]
    fn test_unicast_routes_through_topology() {
        let log1 = Arc::new(Mutex::new(Vec::new()));
        let log2 = Arc::new(Mutex::new(Vec::new()));

        /// At t=10, unicasts 8 to process 1 and 9 to process 2 (no broadcast)
        struct Unicaster {
            fired: bool,
        }
        impl Process<u64> for Unicaster {
            fn resume(&mut self, resumption: Resumption<u64>, env: &mut Environment<u64>) -> Suspension {
                match resumption {
                    Resumption::Initial => Suspension::Timeout(10),
                    Resumption::Scheduled => {
                        if !self.fired {
                            self.fired = true;
                            env.unicast(0, 1, 8);
                            env.unicast(0, 2, 9);
                        }
                        Suspension::Idle
                    }
                    Resumption::Message(_) => Suspension::Idle,
                }
            }
        }

        let mut sim: Simulation<u64> = Simulation::new(5);
        // Process 2 is cut off from 0 for good; process 1 has an overridden link delay
        sim.set_topology(Topology::new(5).with_link_delay(0, 1, 30).with_partition(PartitionWindow {
            start: 0,
            end: u64::MAX,
            groups: vec![[0u64].into_iter().collect(), [2u64].into_iter().collect()],
            mode: PartitionMode::Drop,
        }));
        sim.register(0, Box::new(Unicaster { fired: false }));
        sim.register(1, recorder(&log1));
        sim.register(2, recorder(&log2));
        sim.run(1000);
        // Link delay override applies to unicast: 10 + 30
        assert_eq!(log1.lock().unwrap().clone(), vec![(40, 8)]);
        // The permanent cut drops the 0 -> 2 unicast; nothing else reaches 2 (no broadcast fired)
        assert_eq!(log2.lock().unwrap().clone(), vec![]);
    }

    #[test]
    fn test_partition_delay_until_heal() {
        let (unrestricted, cut) = partition_case(PartitionMode::DelayUntilHeal);
        assert_eq!(unrestricted, vec![(15, 1), (25, 2)]);
        // In-window sends are delivered at window.end + link delay = 105, in send order
        assert_eq!(cut, vec![(105, 1), (105, 2)]);
    }
}
