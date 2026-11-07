//! `--help` and `--version` are answers, not failures.
//!
//! clap returns both as `Err`, purely to carry the rendered text, and [`kaspad::args::parse_args`]
//! used to treat that arm as a failure: everything to stdout, exit 1. Nothing caught it, because
//! the output was right and only the exit code lied — which is exactly the kind of defect that
//! survives a hand check. The VPS setup wizard's own `probe_binary "kaspad" … --version` was
//! reporting "did not finish cleanly" on a healthy binary the whole time.
//!
//! So the assertion here is the exit code and the stream, not the wording. It spawns the real
//! binary rather than calling `Args::parse`, because `parse_args` — the thing that was wrong — is
//! the layer between the two and is reachable no other way.

use std::process::{Command, Output};

fn kaspad(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kaspad")).args(args).output().expect("spawn kaspad")
}

#[test]
fn help_succeeds_on_stdout() {
    for flag in ["--help", "-h"] {
        let out = kaspad(&[flag]);
        assert!(out.status.success(), "kaspad {flag} exited {:?}", out.status.code());
        assert!(String::from_utf8_lossy(&out.stdout).contains("Usage: kaspad"), "kaspad {flag} printed no usage to stdout");
        assert!(out.stderr.is_empty(), "kaspad {flag} wrote to stderr: {}", String::from_utf8_lossy(&out.stderr));
    }
}

#[test]
fn version_succeeds_on_stdout() {
    for flag in ["--version", "-V"] {
        let out = kaspad(&[flag]);
        assert!(out.status.success(), "kaspad {flag} exited {:?}", out.status.code());
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("kaspad "), "kaspad {flag} printed no version to stdout");
        assert!(out.stderr.is_empty(), "kaspad {flag} wrote to stderr: {}", String::from_utf8_lossy(&out.stderr));
    }
}

/// The other half: a real usage error must still fail, and must not be mistaken for output by
/// anything reading stdout.
#[test]
fn a_usage_error_fails_on_stderr() {
    let out = kaspad(&["--not-a-flag"]);
    assert!(!out.status.success(), "kaspad --not-a-flag exited 0");
    assert!(out.stdout.is_empty(), "kaspad --not-a-flag wrote to stdout: {}", String::from_utf8_lossy(&out.stdout));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--not-a-flag"), "the usage error does not name the offending argument");
}
