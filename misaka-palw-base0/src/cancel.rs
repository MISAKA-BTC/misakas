//! **RFC-0001 P1 — cooperative cancellation of a run on the worker.**
//!
//! A resident worker serves one generation at a time on one thread: the gateway's request, then a run of `Token` frames, then one
//! terminator. When the client of a request goes away mid-run the gateway used to drain the rest of the run and discard it (killing the
//! worker would turn every dropped connection into a model re-map — an amplification attack). The `v3-serve` stream now has a `Cancel`
//! frame (`kaspa_consensus_core::palw_freeprompt_v3::fp_worker_cancel_frame_v1`); the worker reads it while the run is in progress and
//! stops at the NEXT token.
//!
//! The decode loops cannot be handed a stop signal through their sink (it is `FnMut(u32)`, shared by five backends and the court), so the
//! signal is a flag on the thread that runs the decode: the serve loop's token sink sets it when a cancel for the running request arrives,
//! and each decode loop checks it right after reporting a token and returns [`CANCELLED_V1`]. The flag is thread-local and is cleared by the
//! serve loop before and after every run, so a cancel can never leak into the next request, and a thread that never sets it (a seat, a
//! court, a drill, a test) is untouched: [`aborted_v1`] is a single thread-local read that is `false`.
//!
//! What a cancelled run leaves behind: nothing. The run returns an error before the commitment, the retained trace and the result frame
//! exist (`retain_v1` runs after the decode), and the worker writes `PalwFpWorkerFrameV1::Cancelled` and keeps its artifact resident.

use std::cell::Cell;

/// The text a cancelled run's error carries. A caller tells a cancellation from a refusal by [`aborted_v1`], not by this string.
pub const CANCELLED_V1: &str = "cancelled by the requester";

thread_local! {
    static ABORT: Cell<bool> = const { Cell::new(false) };
}

/// Ask the run on THIS thread to stop at its next token.
pub fn request_abort_v1() {
    ABORT.with(|a| a.set(true));
}

/// Has the run on this thread been asked to stop?
pub fn aborted_v1() -> bool {
    ABORT.with(|a| a.get())
}

/// Clear the flag (the serve loop calls this before a run starts and after it ends). Returns whether it was set.
pub fn take_abort_v1() -> bool {
    ABORT.with(|a| a.replace(false))
}

/// The check a decode loop makes after reporting a token: `Err(CANCELLED_V1)` when the run was asked to stop.
pub fn check_abort_v1() -> Result<(), String> {
    if aborted_v1() { Err(CANCELLED_V1.to_string()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_per_thread_set_by_a_request_and_cleared_by_take() {
        assert!(!aborted_v1() && check_abort_v1().is_ok());
        request_abort_v1();
        assert!(aborted_v1());
        assert_eq!(check_abort_v1(), Err(CANCELLED_V1.to_string()));
        // Another thread never sees it.
        assert!(!std::thread::spawn(aborted_v1).join().unwrap());
        assert!(take_abort_v1(), "take reports that it was set…");
        assert!(!aborted_v1() && !take_abort_v1(), "…and clears it");
    }
}
