//! **RFC-0001 §2.7 serving properties: what a refused, cancelled or retried request does to the gateway.**
//!
//! Four small, separately testable pieces the entrance (`main.rs`) is built from, kept apart from the
//! socket and the worker so each is exercised without either:
//!
//! * [`render_head`] — the one spelling of a response head. **`Retry-After` is a function of the status line**, so a call
//!   site cannot forget it: the 2026-10 audit found `MAX_IN_FLIGHT_JOBS`' comment promising "a 503 with a Retry-After"
//!   while `respond` wrote none (the header was documented, never emitted). A 503 or a 429 now carries one whatever
//!   path produced it.
//! * [`QueueGate`] / [`InFlightGuard`] — the bounded in-flight queue as a reservation that is RELEASED BY DROP. The old
//!   `fetch_add`-then-check could refuse two requests that together fit (both saw the other's increment) and leaked
//!   the count on a panic; a compare-and-swap reservation does neither.
//! * [`ClientLink`] — "is the person who asked still there?", answered without reading their request again. A request
//!   whose client has gone is **never committed** (the claim would reserve the operator's exposure for an answer nobody
//!   received), and a request still queued for a slot is not run at all.
//! * [`CancelledByClient`] — the one spelling of that outcome, so the entrance can tell it from a worker failure.
//!
//! Nothing here touches a consensus object.

use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Seconds a client refused for a full queue is told to wait. Short: a slot frees when a job ends, and a job is bounded by
/// the decode cap.
pub const RETRY_AFTER_QUEUE_FULL_SECS: u32 = 2;
/// Seconds for a refused connection (the connection cap, a source's share): the next accept loop turn.
pub const RETRY_AFTER_CONNECTION_SECS: u32 = 1;
/// Seconds for a rate refusal (a source over its hourly share). Not the window's remaining time: the gateway keeps
/// counters, not a schedule, and an honest short hint is better than a precise wrong one.
pub const RETRY_AFTER_RATE_SECS: u32 = 30;

/// The `Retry-After` a status line implies, in seconds: 503 → [`RETRY_AFTER_QUEUE_FULL_SECS`], 429 → [`RETRY_AFTER_RATE_SECS`],
/// anything else none.
pub fn default_retry_after(status: &str) -> Option<u32> {
    match status.split_whitespace().next() {
        Some("503") => Some(RETRY_AFTER_QUEUE_FULL_SECS),
        Some("429") => Some(RETRY_AFTER_RATE_SECS),
        _ => None,
    }
}

/// **A response head** (status line, headers, blank line). `retry_after` overrides the status's default; a header value with
/// a CR or LF is dropped rather than written (response splitting), though every caller passes constants.
pub fn render_head(status: &str, content_type: &str, content_length: usize, retry_after: Option<u32>, extra: &[(&str, &str)]) -> String {
    let mut head = format!("HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {content_length}\r\nconnection: close\r\n");
    if let Some(secs) = retry_after.or_else(|| default_retry_after(status)) {
        head.push_str(&format!("retry-after: {secs}\r\n"));
    }
    for (name, value) in extra {
        if name.bytes().chain(value.bytes()).any(|b| b == b'\r' || b == b'\n') {
            continue;
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    head
}

/// The queue had no room for this many jobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueFull {
    pub in_flight: usize,
    pub cap: usize,
}

/// A reservation of `jobs` places in the bounded in-flight queue, released when it drops.
#[derive(Debug)]
pub struct InFlightGuard<'a> {
    counter: &'a AtomicUsize,
    jobs: usize,
}

impl InFlightGuard<'_> {
    pub fn jobs(&self) -> usize {
        self.jobs
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.counter.fetch_sub(self.jobs, Ordering::AcqRel);
    }
}

/// The in-flight counter and its cap: reserve atomically or refuse, never over-reserve even for an instant.
pub struct QueueGate;

impl QueueGate {
    /// Reserve `jobs` places against `cap`. A compare-and-swap loop: the count never exceeds `cap`, so two requests that
    /// together fit are both admitted and two that do not are not both refused.
    pub fn try_reserve(counter: &AtomicUsize, jobs: usize, cap: usize) -> Result<InFlightGuard<'_>, QueueFull> {
        let mut current = counter.load(Ordering::Acquire);
        loop {
            if jobs > cap || current > cap - jobs {
                return Err(QueueFull { in_flight: current, cap });
            }
            match counter.compare_exchange_weak(current, current + jobs, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Ok(InFlightGuard { counter, jobs }),
                Err(seen) => current = seen,
            }
        }
    }
}

/// Is the client of this request still there?
pub trait ClientLink: Sync {
    /// `true` once the peer has closed or reset the connection. Never blocks.
    fn is_gone(&self) -> bool;
}

/// A request with no socket to watch (a test, an in-process caller): never gone.
pub struct AlwaysPresent;

impl ClientLink for AlwaysPresent {
    fn is_gone(&self) -> bool {
        false
    }
}

/// A TCP connection watched with a non-blocking `peek`.
///
/// **The convention is nginx's** (`proxy_ignore_client_abort off`): a read side that reports end-of-stream, or any
/// error other than "would block", is a client that left. A client that half-closes its write side after sending the
/// request (a bare `nc` without `-N`) therefore reads as gone — `--no-cancel-on-disconnect` turns the watch off for
/// such a client. The watch toggles `O_NONBLOCK` and restores it before returning, on the one thread that also writes
/// the response, so it never races a write.
pub struct TcpLink {
    stream: TcpStream,
}

impl TcpLink {
    pub fn new(stream: &TcpStream) -> Option<Self> {
        stream.try_clone().ok().map(|stream| Self { stream })
    }
}

impl ClientLink for TcpLink {
    fn is_gone(&self) -> bool {
        if self.stream.set_nonblocking(true).is_err() {
            return true;
        }
        let mut byte = [0u8; 1];
        let seen = self.stream.peek(&mut byte);
        let restored = self.stream.set_nonblocking(false);
        match seen {
            // End of stream: the peer closed its side.
            Ok(0) => true,
            // Pipelined bytes: the peer is there (and sent more than one request, which this server does not serve).
            Ok(_) => restored.is_err(),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => restored.is_err(),
            // A reset, a broken pipe, a timeout on a dead route.
            Err(_) => true,
        }
    }
}

/// The outcome of a request whose client left before it was committed. Spelled once so the entrance can tell it from a
/// worker failure and write nothing back to a socket nobody reads.
pub const CANCELLED_BY_CLIENT: &str = "cancelled: the client disconnected before the answer was committed";

/// Is this error the cancellation outcome?
pub fn is_cancelled(error: &str) -> bool {
    error == CANCELLED_BY_CLIENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::net::TcpListener;

    #[test]
    fn a_503_and_a_429_always_carry_a_retry_after_and_nothing_else_does() {
        let head = render_head("503 Service Unavailable", "application/json", 12, None, &[]);
        assert!(head.contains("retry-after: 2\r\n"), "the queue-full 503 names how long to wait: {head}");
        let head = render_head("429 Too Many Requests", "application/json", 12, None, &[]);
        assert!(head.contains(&format!("retry-after: {RETRY_AFTER_RATE_SECS}\r\n")), "{head}");
        let head = render_head("200 OK", "application/json", 12, None, &[]);
        assert!(!head.to_ascii_lowercase().contains("retry-after"), "{head}");
        let head = render_head("400 Bad Request", "application/json", 12, None, &[]);
        assert!(!head.to_ascii_lowercase().contains("retry-after"), "{head}");
        // An explicit value wins over the default, and a head always ends in exactly one blank line.
        let head = render_head("503 Service Unavailable", "application/json", 0, Some(9), &[]);
        assert!(head.contains("retry-after: 9\r\n") && !head.contains("retry-after: 2"));
        assert!(head.ends_with("\r\n\r\n") && !head.ends_with("\r\n\r\n\r\n"));
    }

    #[test]
    fn a_header_value_cannot_split_the_response() {
        let head = render_head("200 OK", "application/json", 0, None, &[("x-good", "yes"), ("x-evil", "a\r\nset-cookie: pwned")]);
        assert!(head.contains("x-good: yes\r\n"));
        assert!(!head.contains("set-cookie") && !head.contains("x-evil"), "{head}");
    }

    /// **The bug, over a real socket**: what the listener's 503 writes on the wire.
    #[test]
    fn the_connection_cap_503_reaches_the_client_with_its_retry_after() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            let mut text = String::new();
            std::io::Read::read_to_string(&mut s, &mut text).unwrap();
            text
        });
        let (mut server_side, _) = listener.accept().unwrap();
        crate::respond(&mut server_side, "503 Service Unavailable", &crate::error_body("connection cap reached"));
        drop(server_side);
        let wire = client.join().unwrap();
        assert!(wire.starts_with("HTTP/1.1 503 Service Unavailable\r\n"), "{wire}");
        assert!(wire.to_ascii_lowercase().contains("\r\nretry-after: "), "the 503 must say when to retry: {wire}");
    }

    #[test]
    fn the_queue_never_holds_more_than_its_cap_and_a_drop_frees_the_place() {
        let counter = AtomicUsize::new(0);
        let a = QueueGate::try_reserve(&counter, 3, 8).expect("3 of 8");
        let b = QueueGate::try_reserve(&counter, 5, 8).expect("5 more fits exactly");
        assert_eq!(counter.load(Ordering::Acquire), 8);
        assert_eq!(QueueGate::try_reserve(&counter, 1, 8).unwrap_err(), QueueFull { in_flight: 8, cap: 8 });
        drop(a);
        assert_eq!(counter.load(Ordering::Acquire), 5);
        let c = QueueGate::try_reserve(&counter, 3, 8).expect("the freed places are reusable");
        assert_eq!((b.jobs(), c.jobs()), (5, 3));
        // A request for more jobs than the whole queue can never be admitted, and reserves nothing.
        drop((b, c));
        assert!(QueueGate::try_reserve(&counter, 9, 8).is_err());
        assert_eq!(counter.load(Ordering::Acquire), 0, "a refusal leaves no trace");
    }

    #[test]
    fn many_threads_never_push_the_in_flight_count_over_the_cap() {
        use std::sync::Arc;
        let counter = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let refused = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let (counter, peak, refused) = (Arc::clone(&counter), Arc::clone(&peak), Arc::clone(&refused));
            handles.push(std::thread::spawn(move || {
                for _ in 0..200 {
                    match QueueGate::try_reserve(&counter, 1, 4) {
                        Ok(guard) => {
                            peak.fetch_max(counter.load(Ordering::Acquire), Ordering::AcqRel);
                            std::thread::yield_now();
                            drop(guard);
                        }
                        Err(_) => {
                            refused.fetch_add(1, Ordering::AcqRel);
                        }
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(peak.load(Ordering::Acquire) <= 4, "the cap was exceeded: {}", peak.load(Ordering::Acquire));
        assert_eq!(counter.load(Ordering::Acquire), 0, "every reservation was released");
        assert!(refused.load(Ordering::Acquire) > 0, "16 threads against a cap of 4 must have been refused sometimes");
    }

    #[test]
    fn a_closed_connection_reads_as_a_client_that_left_and_an_open_one_does_not() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        let (server_side, _) = listener.accept().unwrap();
        let link = TcpLink::new(&server_side).expect("clone");
        assert!(!link.is_gone(), "a connected client that has said nothing more is present");
        client.write_all(b"x").unwrap();
        client.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!link.is_gone(), "pipelined bytes are not a departure");
        assert!(!link.is_gone(), "peeking does not consume them");
        drop(client);
        std::thread::sleep(std::time::Duration::from_millis(50));
        // The unread byte is still queued ahead of the FIN: read it, then the end of stream is visible.
        let mut sink = [0u8; 1];
        std::io::Read::read_exact(&mut &server_side, &mut sink).unwrap();
        assert!(link.is_gone(), "after the peer closed, the link reports it");
        assert!(!AlwaysPresent.is_gone());
    }

    #[test]
    fn the_cancellation_outcome_is_recognisable_and_a_worker_error_is_not() {
        assert!(is_cancelled(CANCELLED_BY_CLIENT));
        assert!(!is_cancelled("the worker refused the job: x"));
    }
}
