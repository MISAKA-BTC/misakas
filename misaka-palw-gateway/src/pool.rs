//! **RFC-0001 §2.7 stage 1: the per-class worker pool and the entrance's connection bounds.**
//!
//! A resident worker is one whole-model subprocess with one KV cache. A class that wants more than
//! one answer at a time runs several of them: the artifact is memory-mapped read-only, so the
//! weights are shared through the OS page cache and each process pays only its own KV and scratch;
//! determinism is untouched because every process runs the same integer kernels on its own job.
//!
//! This module is the scheduling half, kept generic over the worker so it is tested without a model:
//!
//! * [`SlotPool`] — `N` slots; [`SlotPool::acquire`] hands out an idle slot in FIFO order, and
//!   blocks (bounded by the caller's in-flight cap, which lives in the entrance) until one is free.
//!   A slot whose worker died is replaced by the caller through [`SlotGuard::replace`].
//! * [`SourceGate`] — a per-source cap on open connections and on jobs in flight, the part of the
//!   bound that is not a courtesy: it is what stops one address holding every slot.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Condvar, Mutex};

struct PoolState<T> {
    /// Idle slots, by index; `workers[i]` is `Some` while slot `i` is idle.
    idle: Vec<usize>,
    workers: Vec<Option<T>>,
    /// Tickets, in arrival order: the next idle slot goes to the front ticket.
    waiting: VecDeque<u64>,
    next_ticket: u64,
}

/// `N` slots handing out one worker each, first come first served.
pub struct SlotPool<T> {
    state: Mutex<PoolState<T>>,
    freed: Condvar,
    slots: usize,
}

/// A worker checked out of the pool; returned to its slot on drop.
pub struct SlotGuard<'a, T> {
    pool: &'a SlotPool<T>,
    index: usize,
    worker: Option<T>,
}

impl<T> SlotPool<T> {
    /// A pool over `workers` (at least one).
    pub fn new(workers: Vec<T>) -> Self {
        assert!(!workers.is_empty(), "a pool has at least one slot");
        let slots = workers.len();
        Self {
            state: Mutex::new(PoolState {
                idle: (0..slots).rev().collect(),
                workers: workers.into_iter().map(Some).collect(),
                waiting: VecDeque::new(),
                next_ticket: 0,
            }),
            freed: Condvar::new(),
            slots,
        }
    }

    pub fn slots(&self) -> usize {
        self.slots
    }

    /// How many callers are waiting for a slot right now.
    pub fn waiting(&self) -> usize {
        self.state.lock().expect("the pool lock is never poisoned").waiting.len()
    }

    /// A slot, in arrival order: blocks until this caller is first in line AND a slot is idle.
    pub fn acquire(&self) -> SlotGuard<'_, T> {
        let mut state = self.state.lock().expect("the pool lock is never poisoned");
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        state.waiting.push_back(ticket);
        loop {
            if state.waiting.front() == Some(&ticket)
                && let Some(index) = state.idle.pop()
            {
                state.waiting.pop_front();
                let worker = state.workers[index].take();
                // The next in line may also have a slot waiting for it.
                self.freed.notify_all();
                return SlotGuard { pool: self, index, worker };
            }
            state = self.freed.wait(state).expect("the pool lock is never poisoned");
        }
    }
}

impl<T> SlotGuard<'_, T> {
    #[cfg(test)]
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.worker.as_mut().expect("a guard holds its worker until it drops")
    }

    /// Replace the slot's worker (a respawn after a transport failure).
    #[cfg(test)]
    pub fn replace(&mut self, worker: T) {
        self.worker = Some(worker);
    }
}

impl<T> Drop for SlotGuard<'_, T> {
    fn drop(&mut self) {
        let mut state = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
        state.workers[self.index] = self.worker.take();
        state.idle.push(self.index);
        self.pool.freed.notify_all();
    }
}

/// **Per-source bounds on the public entrance** (RFC-0001 §2.7: "connection limits"). A source is an
/// address; behind a proxy many people share one, so these are the SECONDARY bound (ADR-0077 SA-8)
/// — the binding ones are the slot count and the bounded queue — but they are what keeps one
/// address from holding every connection and every queued job.
pub struct SourceGate {
    max_connections: u32,
    max_jobs: u32,
    inner: Mutex<HashMap<IpAddr, (u32, u32)>>,
}

/// Why a source was turned away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateRefusal {
    TooManyConnections,
    TooManyJobs,
}

impl SourceGate {
    pub fn new(max_connections: u32, max_jobs: u32) -> Self {
        Self { max_connections, max_jobs, inner: Mutex::new(HashMap::new()) }
    }

    /// Count a connection from `source`; `Err` when it already holds its share.
    pub fn open_connection(&self, source: IpAddr) -> Result<(), GateRefusal> {
        let mut map = self.inner.lock().expect("the gate lock is never poisoned");
        let entry = map.entry(source).or_insert((0, 0));
        if entry.0 >= self.max_connections {
            return Err(GateRefusal::TooManyConnections);
        }
        entry.0 += 1;
        Ok(())
    }

    pub fn close_connection(&self, source: IpAddr) {
        let mut map = self.inner.lock().expect("the gate lock is never poisoned");
        if let Some(entry) = map.get_mut(&source) {
            entry.0 = entry.0.saturating_sub(1);
            if *entry == (0, 0) {
                map.remove(&source);
            }
        }
    }

    /// Count `n` jobs in flight for `source` (a request for `n` candidates is `n` jobs).
    pub fn start_jobs(&self, source: IpAddr, n: u32) -> Result<(), GateRefusal> {
        let mut map = self.inner.lock().expect("the gate lock is never poisoned");
        let entry = map.entry(source).or_insert((0, 0));
        if entry.1.saturating_add(n) > self.max_jobs {
            return Err(GateRefusal::TooManyJobs);
        }
        entry.1 += n;
        Ok(())
    }

    pub fn finish_jobs(&self, source: IpAddr, n: u32) {
        let mut map = self.inner.lock().expect("the gate lock is never poisoned");
        if let Some(entry) = map.get_mut(&source) {
            entry.1 = entry.1.saturating_sub(n);
            if *entry == (0, 0) {
                map.remove(&source);
            }
        }
    }

    #[cfg(test)]
    pub fn tracked_sources(&self) -> usize {
        self.inner.lock().expect("the gate lock is never poisoned").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn a_pool_never_runs_more_jobs_than_it_has_slots() {
        let pool = Arc::new(SlotPool::new(vec![0u32, 1, 2]));
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..12)
            .map(|_| {
                let (pool, running, peak) = (Arc::clone(&pool), Arc::clone(&running), Arc::clone(&peak));
                std::thread::spawn(move || {
                    let guard = pool.acquire();
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(15));
                    running.fetch_sub(1, Ordering::SeqCst);
                    drop(guard);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 3, "all three slots were used and never a fourth job");
        assert_eq!(pool.waiting(), 0);
    }

    #[test]
    fn waiting_callers_are_served_in_arrival_order() {
        let pool = Arc::new(SlotPool::new(vec![()]));
        let held = pool.acquire();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for i in 0..4u32 {
            let (pool, order) = (Arc::clone(&pool), Arc::clone(&order));
            handles.push(std::thread::spawn(move || {
                let _g = pool.acquire();
                order.lock().unwrap().push(i);
            }));
            // Each thread must be queued before the next starts, so arrival order is `i`.
            while pool.waiting() <= i as usize {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        drop(held);
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_replaced_worker_is_what_the_next_caller_gets() {
        let pool = SlotPool::new(vec!["old"]);
        {
            let mut g = pool.acquire();
            assert_eq!(*g.get_mut(), "old");
            g.replace("fresh");
        }
        let mut g = pool.acquire();
        assert_eq!(*g.get_mut(), "fresh");
    }

    #[test]
    fn one_source_cannot_hold_more_than_its_share_of_connections_or_jobs() {
        let gate = SourceGate::new(2, 3);
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        assert!(gate.open_connection(a).is_ok());
        assert!(gate.open_connection(a).is_ok());
        assert_eq!(gate.open_connection(a), Err(GateRefusal::TooManyConnections));
        assert!(gate.open_connection(b).is_ok(), "another source is unaffected");
        gate.close_connection(a);
        assert!(gate.open_connection(a).is_ok(), "a closed connection frees its share");
        // A request for n candidates is n jobs against the source's share.
        assert!(gate.start_jobs(a, 2).is_ok());
        assert_eq!(gate.start_jobs(a, 2), Err(GateRefusal::TooManyJobs));
        assert!(gate.start_jobs(a, 1).is_ok());
        assert_eq!(gate.start_jobs(a, 1), Err(GateRefusal::TooManyJobs));
        gate.finish_jobs(a, 3);
        assert!(gate.start_jobs(a, 3).is_ok());
        // Bookkeeping does not leak: a source with nothing open is forgotten.
        gate.finish_jobs(a, 3);
        gate.close_connection(a);
        gate.close_connection(a);
        gate.close_connection(b);
        assert_eq!(gate.tracked_sources(), 0);
    }
}
