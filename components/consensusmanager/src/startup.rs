//! Local startup admission. Never take a pruning read guard while waiting for the
//! synchronous recovery that must take the pruning write guard before workers start.

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

pub(crate) struct StartupGate {
    started: AtomicBool,
    changed: Notify,
}

impl StartupGate {
    pub(crate) fn new(started: bool) -> Self {
        Self { started: AtomicBool::new(started), changed: Notify::new() }
    }

    pub(crate) fn is_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    pub(crate) fn finish(&self) {
        self.started.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }

    pub(crate) async fn wait(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            // Register before checking the flag: finish between checking and
            // awaiting must not leave a reader asleep after the one-time event.
            changed.as_mut().enable();
            if self.is_started() {
                return;
            }
            changed.await;
        }
    }
}
