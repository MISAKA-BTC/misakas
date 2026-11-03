//! Nonblocking native stdio adapters for the explicit Hello2 process owner.
//! One controller sets an absolute exchange deadline shared by both pipes;
//! individual read/write/poll/retry calls never restart it. No threads, queues,
//! userspace I/O buffers, global signal changes or process ownership live here.
//! Supported syscall profiles are Linux and macOS. Other targets refuse.
//!
//! The Rust Node/SDK runtime's existing ignored-SIGPIPE policy is a precondition
//! for pipe writes. Arbitrary embedding hosts must establish that policy outside
//! this adapter; we do not change signal dispositions/masks. OS scheduling,
//! allocator, kill/wait/process-tree and full CPU/RSS guarantees are separate.

use super::process::WideExchangeControllerV2;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::{AsRawFd, RawFd};
use std::{
    io::{self, Read, Write},
    mem::size_of,
    process::{ChildStdin, ChildStdout},
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::{Duration, Instant},
};

struct DeadlineState {
    deadline: Option<Instant>,
}
// Quote-only shape of the pinned std ArcInner: two atomic counters followed by
// the value, under repr(C). This is requested allocation backing, not RSS.
#[repr(C)]
#[allow(dead_code)]
struct ArcDeadlineEnvelope {
    strong: AtomicUsize,
    weak: AtomicUsize,
    data: Mutex<DeadlineState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideUnixAdapterBackingV2 {
    /// Sum of the two following components, counted once in adapter retention.
    pub shared_heap_bytes: u64,
    pub arc_heap_bytes: u64,
    pub platform_mutex_heap_bytes: u64,
    pub reader_inline_bytes: u64,
    pub writer_inline_bytes: u64,
    pub controller_inline_bytes: u64,
}
/// Pre-admit shared_heap_bytes ONCE as adapter_retained_bytes in the process
/// quote. R/W/controller inline storage is already included by sizeof(owner).
/// There are no other adapter heap buffers, worker stacks or queues.
pub fn unix_stdio_adapter_backing_v2() -> WideUnixAdapterBackingV2 {
    let arc_heap_bytes = size_of::<ArcDeadlineEnvelope>() as u64;
    // Pinned Rust1.93 macOS Mutex owns OnceBox<pal::Mutex>, whose sole field
    // is UnsafeCell<pthread_mutex_t>. Linux's futex Mutex is inline instead.
    #[cfg(target_os = "macos")]
    let platform_mutex_heap_bytes = size_of::<libc::pthread_mutex_t>() as u64;
    #[cfg(not(target_os = "macos"))]
    let platform_mutex_heap_bytes = 0;
    WideUnixAdapterBackingV2 {
        shared_heap_bytes: arc_heap_bytes + platform_mutex_heap_bytes,
        arc_heap_bytes,
        platform_mutex_heap_bytes,
        reader_inline_bytes: size_of::<WideUnixReadV2>() as u64,
        writer_inline_bytes: size_of::<WideUnixWriteV2>() as u64,
        controller_inline_bytes: size_of::<WideUnixDeadlineV2>() as u64,
    }
}

pub struct WideUnixReadV2 {
    pipe: ChildStdout,
    state: Arc<Mutex<DeadlineState>>,
}
pub struct WideUnixWriteV2 {
    pipe: ChildStdin,
    state: Arc<Mutex<DeadlineState>>,
}
/// Deliberately not Clone. The process owner retains the only controller and
/// calls it once before connect and once before each complete client.request.
/// It must not be called between prefix/body/read/write/flush retries.
pub struct WideUnixDeadlineV2 {
    state: Arc<Mutex<DeadlineState>>,
    timeout: Duration,
}

fn timed_out() -> io::Error {
    io::ErrorKind::TimedOut.into()
}
fn live(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline { Err(timed_out()) } else { Ok(()) }
}
fn deadline(state: &Mutex<DeadlineState>) -> io::Result<Instant> {
    // No blocking lock acquisition is hidden in I/O. The controller/pipe owners
    // are used serially; accidental concurrent access or poisoning fails closed.
    let guard = state.try_lock().map_err(|_| io::Error::from(io::ErrorKind::Other))?;
    guard.deadline.ok_or_else(|| io::ErrorKind::NotConnected.into())
}
impl WideExchangeControllerV2 for WideUnixDeadlineV2 {
    fn begin_exchange(&mut self) -> io::Result<()> {
        let next = Instant::now().checked_add(self.timeout).ok_or(io::ErrorKind::InvalidInput)?;
        let mut state = self.state.try_lock().map_err(|_| io::Error::from(io::ErrorKind::Other))?;
        state.deadline = Some(next);
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn nonblocking(fd: RawFd, until: Instant) -> io::Result<()> {
    let flags = loop {
        live(until)?;
        // SAFETY: fd is borrowed from the live owning Child pipe. F_GETFL takes
        // no variadic argument and does not transfer or duplicate ownership.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 {
            live(until)?;
            break flags;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    };
    loop {
        live(until)?;
        // SAFETY: same live fd; the third argument is the required integer flag
        // word. Preserve existing flags and add only O_NONBLOCK.
        let result = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
        if result >= 0 {
            return live(until);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn poll_millis(until: Instant) -> io::Result<libc::c_int> {
    let left = until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or_else(timed_out)?;
    // Round up, so a sub-millisecond remainder cannot become a busy zero wait.
    // Long waits are clamped per poll; every repetition still uses `until`.
    let millis = left.as_millis().saturating_add(1).min(libc::c_int::MAX as u128);
    Ok(millis as libc::c_int)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn poll_once(fd: RawFd, events: libc::c_short, millis: libc::c_int) -> io::Result<Option<libc::c_short>> {
    let mut descriptor = libc::pollfd { fd, events, revents: 0 };
    // SAFETY: a single initialized pollfd lives for this call; count is exactly1.
    // poll borrows its live owned descriptor and writes only revents.
    let result = unsafe { libc::poll(&mut descriptor, 1, millis) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if result == 0 { Ok(None) } else { Ok(Some(descriptor.revents)) }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn wait_with(
    fd: RawFd,
    events: libc::c_short,
    until: Instant,
    mut poll: impl FnMut(RawFd, libc::c_short, libc::c_int) -> io::Result<Option<libc::c_short>>,
) -> io::Result<()> {
    loop {
        let millis = poll_millis(until)?;
        let result = poll(fd, events, millis);
        live(until)?; // Even readiness/Interrupted cannot extend the deadline.
        match result {
            Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => continue,
            Err(error) => return Err(error),
            Ok(None) => continue,
            Ok(Some(ready)) => {
                if ready & libc::POLLNVAL != 0 {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                // HUP/ERR may coexist with readable tail data. Let the owning std
                // pipe perform one nonblocking I/O to expose data/EOF/EPIPE.
                if ready & (events | libc::POLLHUP | libc::POLLERR) != 0 {
                    return Ok(());
                }
            }
        }
    }
}

/// Factory for the process owner's required (reader,writer,controller) callback.
/// Caller pre-admits unix_stdio_adapter_backing_v2().shared_heap_bytes. Partial
/// failure/unwind drops only native pipe handles/Arc state: no worker joins or
/// child waits. The process owner remains responsible for kill/wait.
pub fn unix_stdio_deadline_adapters_v2(
    stdout: ChildStdout,
    stdin: ChildStdin,
    timeout: Duration,
) -> io::Result<(WideUnixReadV2, WideUnixWriteV2, WideUnixDeadlineV2)> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        construct(stdout, stdin, timeout, nonblocking)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (stdout, stdin, timeout);
        Err(io::ErrorKind::Unsupported.into())
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn construct(
    stdout: ChildStdout,
    stdin: ChildStdin,
    timeout: Duration,
    mut configure: impl FnMut(RawFd, Instant) -> io::Result<()>,
) -> io::Result<(WideUnixReadV2, WideUnixWriteV2, WideUnixDeadlineV2)> {
    if timeout.is_zero() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let until = Instant::now().checked_add(timeout).ok_or(io::ErrorKind::InvalidInput)?;
    configure(stdout.as_raw_fd(), until)?;
    configure(stdin.as_raw_fd(), until)?;
    live(until)?;
    let state = Arc::new(Mutex::new(DeadlineState { deadline: None }));
    // Initialize the quoted macOS lazy mutex box serially before exposing any
    // adapter, so concurrent first-use allocation is not part of this subset.
    drop(state.try_lock().map_err(|_| io::Error::from(io::ErrorKind::Other))?);
    live(until)?;
    Ok((
        WideUnixReadV2 { pipe: stdout, state: state.clone() },
        WideUnixWriteV2 { pipe: stdin, state: state.clone() },
        WideUnixDeadlineV2 { state, timeout },
    ))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn transfer_with(
    fd: RawFd,
    events: libc::c_short,
    state: &Mutex<DeadlineState>,
    mut native: impl FnMut() -> io::Result<usize>,
) -> io::Result<usize> {
    let until = deadline(state)?;
    loop {
        wait_with(fd, events, until, poll_once)?;
        let result = native();
        live(until)?;
        match result {
            Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => continue,
            other => return other,
        }
    }
}
impl Read for WideUnixReadV2 {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            transfer_with(self.pipe.as_raw_fd(), libc::POLLIN, &self.state, || self.pipe.read(bytes))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Err(io::ErrorKind::Unsupported.into())
        }
    }
}
impl Write for WideUnixWriteV2 {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            transfer_with(self.pipe.as_raw_fd(), libc::POLLOUT, &self.state, || self.pipe.write(bytes))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Err(io::ErrorKind::Unsupported.into())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        // Native ChildStdin is unbuffered. A flush must still observe the shared
        // exchange deadline; it must not start a fresh interval.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            live(deadline(&self.state)?)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Err(io::ErrorKind::Unsupported.into())
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    struct ChildGuard(Child);
    impl ChildGuard {
        fn pipes(script: &str) -> (Self, ChildStdout, ChildStdin) {
            let mut child = Command::new("/bin/sh")
                .args(["-c", script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let out = child.stdout.take().unwrap();
            let input = child.stdin.take().unwrap();
            (Self(child), out, input)
        }
        fn finished(&mut self) {
            let until = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    assert!(status.success());
                    return;
                }
                assert!(Instant::now() < until, "child did not finish");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn real_slow_peer_cannot_restart_deadline_with_each_byte() {
        let (_child, out, input) =
            ChildGuard::pipes("IFS= read -r gate; i=0; while [ $i -lt 16 ]; do printf x; sleep 0.03; i=$((i+1)); done");
        let (mut read, mut write, mut control) = unix_stdio_deadline_adapters_v2(out, input, Duration::from_millis(120)).unwrap();
        control.begin_exchange().unwrap();
        let started = Instant::now();
        write.write_all(b"go\n").unwrap(); // Child starts its drip only inside this exchange.
        let mut received = 0;
        loop {
            let mut byte = [0];
            match read.read(&mut byte) {
                Ok(1) => {
                    assert_eq!(byte, [b'x']);
                    received += 1;
                    assert!(received < 16, "byte progress restarted the clock");
                }
                Err(e) => {
                    assert_eq!(e.kind(), io::ErrorKind::TimedOut);
                    break;
                }
                other => panic!("unexpected slow peer result {other:?}"),
            }
        }
        assert!(started.elapsed() < Duration::from_secs(3)); // CI guard, not an OS scheduling guarantee.
    }
    #[test]
    fn real_partial_read_and_write_use_original_native_handles() {
        let (mut child, out, input) = ChildGuard::pipes("IFS= read -r gate; printf abc; sleep 0.05; cat >/dev/null");
        let (mut read, mut write, mut control) = unix_stdio_deadline_adapters_v2(out, input, Duration::from_secs(2)).unwrap();
        control.begin_exchange().unwrap();
        write.write_all(b"go\n").unwrap();
        let mut buf = [0; 1024];
        // Force a valid short native operation through the same production
        // transfer loop; do not assume printf coalescing or OS pipe capacity.
        let first = transfer_with(read.pipe.as_raw_fd(), libc::POLLIN, &read.state, || read.pipe.read(&mut buf[..2])).unwrap();
        assert!(first > 0 && first <= 2);
        read.read_exact(&mut buf[first..3]).unwrap();
        assert_eq!(&buf[..3], b"abc");
        let payload = vec![7u8; 64 << 10]; // Test-owned external input.
        let first = transfer_with(write.pipe.as_raw_fd(), libc::POLLOUT, &write.state, || write.pipe.write(&payload[..7])).unwrap();
        assert!(first > 0 && first <= 7 && first < payload.len());
        write.write_all(&payload[first..]).unwrap();
        write.flush().unwrap();
        drop(write);
        drop(read);
        drop(control);
        child.finished();
    }
    #[test]
    fn a_write_then_read_share_one_absolute_exchange_clock() {
        let (_child, out, input) = ChildGuard::pipes("IFS= read -r gate; sleep 0.24; printf r");
        let (mut read, mut write, mut control) = unix_stdio_deadline_adapters_v2(out, input, Duration::from_millis(200)).unwrap();
        control.begin_exchange().unwrap();
        let started = Instant::now();
        write.write_all(b"go\n").unwrap();
        write.flush().unwrap();
        // Deliberate caller-side elapsed time within the SAME exchange. A fresh
        // per-read timeout would incorrectly accept the child's later response.
        // No assumption about OS pipe buffer capacity is needed for this case.
        std::thread::sleep(Duration::from_millis(120));
        let error = read.read_exact(&mut [0u8; 1]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn interrupted_and_wouldblock_retries_keep_the_same_deadline() {
        let until = Instant::now() + Duration::from_millis(25);
        let mut calls = 0;
        // Deliberate syscall-result injection through the production wait loop:
        // no process-wide signal-handler changes or timing-dependent SIGUSR1.
        let error = wait_with(-1, libc::POLLIN, until, |_, _, _| {
            calls += 1;
            std::thread::sleep(Duration::from_millis(1));
            Err(if calls % 2 == 0 { io::ErrorKind::Interrupted } else { io::ErrorKind::WouldBlock }.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(calls > 1);
    }
    #[test]
    fn partial_constructor_error_and_unwind_close_pipes_without_joining_child() {
        for panic in [false, true] {
            let (mut child, out, input) = ChildGuard::pipes("exec cat >/dev/null");
            let mut calls = 0;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                construct(out, input, Duration::from_secs(1), |fd, until| {
                    calls += 1;
                    if calls == 2 {
                        if panic {
                            panic!("injected partial construction panic");
                        }
                        return Err(io::ErrorKind::Other.into());
                    }
                    nonblocking(fd, until)
                })
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            child.finished(); // Native stdin close delivers EOF; no adapter worker exists.
        }
    }
}
