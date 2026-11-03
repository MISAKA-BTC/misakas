//! Carry the input allocation owner through work that can outlive its caller.
use kaspa_core::memory_budget::MemoryLease;

/// One captured tuple guarantees input-before-ticket destruction even when a
/// blocking task is abandoned before it starts. Its returned tuple likewise
/// keeps an unreceived output alive ahead of the input memory ticket.
pub(super) fn run_with_input_memory<T, R>(input: (T, MemoryLease), work: impl FnOnce(T) -> R) -> (R, MemoryLease) {
    let (input, memory) = input;
    let output = work(input);
    (output, memory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_core::memory_budget::{MemoryBudget, MemoryClass};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    struct Payload {
        budget: Arc<MemoryBudget>,
        alive_during_drop: Arc<AtomicBool>,
    }
    impl Drop for Payload {
        fn drop(&mut self) {
            self.alive_during_drop.store(self.budget.snapshot().reserved_bytes == 80, Ordering::SeqCst);
        }
    }
    #[test]
    fn cancelled_receiver_does_not_release_running_work_or_unreceived_output() {
        let budget = MemoryBudget::new(100, [100; 4]);
        let lease = budget.try_reserve(MemoryClass::Snapshot, 80).unwrap();
        let observed = Arc::new(AtomicBool::new(false));
        let input = (Payload { budget: budget.clone(), alive_during_drop: observed.clone() }, lease.clone());
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let (output_tx, output_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = run_with_input_memory(input, move |payload| {
                started_tx.send(()).unwrap();
                finish_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
                payload
            });
            // A dropped receiver models a cancelled async caller/JoinHandle.
            drop(output_tx.send(result));
        });
        started_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        drop(output_rx);
        drop(lease);
        assert_eq!(budget.snapshot().reserved_bytes, 80, "blocking work retains input after caller cancellation");
        finish_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(observed.load(Ordering::SeqCst), "unreceived payload is destroyed before the ticket returns");
        assert_eq!(budget.snapshot().reserved_bytes, 0);
    }
}
