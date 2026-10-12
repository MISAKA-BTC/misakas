//! `palw-agent` — the VPS runtime supervisor for the pinned `palw-worker`.
//!
//! The daemon itself lives in [`agent`]; this file is only the platform gate. The transport is
//! `misaka-palw-agent-borsh/v1` over an `AF_UNIX` stream socket, and admission is decided by the
//! peer's credentials (`SO_PEERCRED` / `getpeereid`) plus the socket's own filesystem mode —
//! none of which has a Windows equivalent. So the agent is built on Unix and, on any other host,
//! compiles to a binary that says so and exits non-zero rather than breaking the workspace
//! build for everyone. (Same shape as `kaspa-pq-signer`, and for the same reason.)

#[cfg(unix)]
mod agent;

#[cfg(unix)]
fn main() {
    agent::run()
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "palw-agent supervises the worker over a Unix domain socket with peer-credential \
         admission (misaka-palw-agent-borsh/v1) and only runs on Unix targets. Run it on \
         Linux/macOS — WSL2 or a container both work."
    );
    std::process::ExitCode::FAILURE
}
