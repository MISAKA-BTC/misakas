//! **ADR-0079 invariant S10 in the crate that has the most reason to break it.**
//!
//! *"Nothing in the tree executes, fetches, or shells out on the strength of model output."*
//!
//! This crate is where model output arrives as a THING TO BE PROCESSED: a DSL a stranger's prompt
//! produced, turned into a mesh, a MIDI file, a PNG — or, in the `code` and `contract` rows, into
//! EVM initcode and a build manifest. ADR-0078 SA-1 calls that *"the largest privilege in the
//! lineage"*, and ADR-0079 Decision 12 gives it two doors and no third:
//!
//! * `src/kinds/code.rs` — the confined spawn (the EVM runner, and the external toolchain gate);
//! * `src/bin/palw-evm-runner.rs` — the process that actually executes, holding nothing.
//!
//! A guard that only read `code.rs` would pass on the day a second door is opened in `scene.rs`,
//! which is precisely how the defect S10 names arrives. So this reads the whole crate.
//!
//! It is a text scan, and text scans are blunt. That is the trade ADR-0079's other tree guard
//! (`misaka-palw/tests/host_security_tree_guard.rs`) already took: a blunt guard that fails on a
//! new door is a review, and a review is what a new door needs.

use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The two files that may name a process API, with the citation that allows each.
///
/// * `src/kinds/code.rs` — ADR-0078 SA-1 / ADR-0079 Decision 12: the EVM runs in a separate
///   process, and an external toolchain runs under a proven backend in an ephemeral tree. The
///   spawn is the gate, so the gate is where the spawn lives.
/// * `src/bin/palw-evm-runner.rs` — the confined process itself.
const MAY_SPAWN: &[&str] = &["src/kinds/code.rs", "src/bin/palw-evm-runner.rs"];

fn rust_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = crate_root();
    let mut paths = Vec::new();
    walk(&root.join("src"), &mut paths);
    paths.sort();
    assert!(paths.len() > 10, "the scan found only {} files — the guard is not looking at the crate", paths.len());
    paths
        .into_iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(&root).unwrap_or(&p).display().to_string().replace('\\', "/");
            std::fs::read_to_string(&p).ok().map(|s| (rel, s))
        })
        .collect()
}

/// Lines that are not comments — a rule about what the code DOES should not fire on a sentence
/// about what the code does not do.
///
/// A line holding `concat!(` is skipped for the same reason: this crate's sibling scan in
/// `src/lib.rs` assembles its own needles from pieces so that its list is not an offender, and a
/// scanner reading another scanner's list is reading names, not uses.
fn code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.lines().enumerate().filter(|(_, line)| {
        let trimmed = line.trim_start();
        !(trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.is_empty() || line.contains("concat!("))
    })
}

/// **S10, the execution half.** A transformer's input is a model's answer. Nothing may run a
/// program on the strength of it except through the two gates ADR-0079 Decision 12 names.
#[test]
fn only_the_confinement_gates_may_spawn_a_process() {
    // `std::process::exit` and `std::process::id` are not execution: one ends this process, the
    // other names it. What is banned is starting another program.
    const BANNED: &[&str] = &["Command::new", "process::Command", "std::os::unix::process::CommandExt", "execvp", "execve("];
    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        if MAY_SPAWN.contains(&path.as_str()) {
            continue;
        }
        for (n, line) in code_lines(&source) {
            for banned in BANNED {
                if line.contains(banned) {
                    findings.push(format!("{path}:{}: `{banned}`", n + 1));
                }
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 S10 / Decision 12: nothing executes on the strength of model output except through the confined \
         runner ({}) — and a third door is a review, not an edit. Found:\n  {}",
        MAY_SPAWN.join(" and "),
        findings.join("\n  ")
    );
}

/// **S10, the fetch half.** A transformer is a pure function of bytes it was handed. A network
/// name anywhere in this crate — in a gate or out of it — is a transformer that could read a
/// second input, which is ADR-0078 Decision 3's discipline broken and ADR-0079 Decision 1's
/// capability list ignored in the same line.
#[test]
fn nothing_in_this_crate_reaches_the_network() {
    const BANNED: &[&str] =
        &["std::net", "TcpStream", "TcpListener", "UdpSocket", "reqwest", "ureq::", "hyper::", "http::Request", "url::Url"];
    let mut findings = Vec::new();
    for (path, source) in rust_sources() {
        for (n, line) in code_lines(&source) {
            for banned in BANNED {
                if line.contains(banned) {
                    findings.push(format!("{path}:{}: `{banned}`", n + 1));
                }
            }
        }
    }
    assert!(
        findings.is_empty(),
        "ADR-0079 Decision 1 and S10: a network read is a capability the arithmetic already forbids — the deny list \
         is READ OFF the determinism rules, and this crate is where they are enforced. Found:\n  {}",
        findings.join("\n  ")
    );
}

/// **ADR-0078 SA-1's door count.** The in-process EVM entry point exists for exactly one caller.
/// Its NAME may appear in the file that defines it and in the runner that calls it; a third file
/// naming it is a second process running model-written initcode.
#[test]
fn the_in_process_evm_has_exactly_one_caller_and_it_is_the_runner() {
    const ENTRY: &str = "execute_evm_job_in_this_process";
    let mut naming = Vec::new();
    for (path, source) in rust_sources() {
        if source.contains(ENTRY) {
            naming.push(path);
        }
    }
    naming.sort();
    assert_eq!(
        naming,
        vec!["src/bin/palw-evm-runner.rs".to_string(), "src/kinds/code.rs".to_string()],
        "ADR-0078 SA-1: `{ENTRY}` is the runner's entry point and nothing else's — a third file naming it is a \
         second process executing model-written initcode, which is the privilege the ADR is about"
    );

    // And the transformer's own path does not call it: it spawns.
    let code = std::fs::read_to_string(crate_root().join("src/kinds/code.rs")).expect("code.rs is in the crate");
    let shipped = code.split("#[cfg(test)]").next().unwrap_or(&code);
    let calls: Vec<&str> = code_lines(shipped).map(|(_, l)| l).filter(|l| l.contains(ENTRY)).collect();
    assert_eq!(
        calls.len(),
        1,
        "the only mention of `{ENTRY}` outside the runner is its own definition; found:\n  {}",
        calls.join("\n  ")
    );
    assert!(calls[0].contains("pub fn"), "and that mention is the definition: {}", calls[0]);

    // The transformer path reaches the EVM through the confined spawn, and only through it.
    assert!(
        shipped.contains("let build = build_evm_v1_confined(&code)?"),
        "`run_evm_v1` must build through the confined runner (ADR-0078 SA-1)"
    );
}

/// **ADR-0079 Decision 12's gate, as text.** The external runner refuses a `none` backend and a
/// reachable key. A future edit that deletes either check deletes a sentence that names its ADR,
/// which is what makes this readable as a rule rather than as a habit.
#[test]
fn the_external_toolchain_gate_is_still_two_refusals() {
    let code = std::fs::read_to_string(crate_root().join("src/kinds/code.rs")).expect("code.rs is in the crate");
    let shipped = code.split("#[cfg(test)]").next().unwrap_or(&code);
    let run_external = shipped.split("pub fn run_external").nth(1).expect("run_external still exists");
    assert!(
        run_external.contains("reachable_signing_secrets"),
        "ADR-0079 Decision 12: the build's output is never executed on a host that holds a bond or wallet key"
    );
    let run_in = shipped.split("fn run_in(").nth(1).expect("run_in still exists");
    assert!(
        run_in.contains("establish_confinement") && run_in.contains("ConfinementBackend::None"),
        "ADR-0079 Decision 12 / S11: an external toolchain runs under a PROVEN backend or it does not run"
    );
}
