//! ADR-0042 Decision 4 (PR-02): the consensus build carries no model dependency — enforced on
//! the DEPENDENCY GRAPH, not stated in a code comment.
//!
//! A full node validates and adjudicates without any LLM; inference lives only in sidecars and
//! in the driver crate a composition root may link. The property that makes that a boundary
//! rather than a convention is that the crates which define validity cannot even NAME the
//! crates that reach a model. This test reads `cargo metadata` for the workspace and fails on
//! any declared edge — normal, build, dev, optional included — from a guarded consensus crate
//! to a runtime-reaching crate.
//!
//! Optional dependencies are forbidden too, deliberately: an optional edge is one feature flip
//! away from being real, which is the five-fences pattern ADR-0042 exists to remove, restated
//! in `Cargo.toml`.
//!
//! `--no-deps` metadata resolves nothing and touches no network: it parses the workspace
//! manifests and reports each package's DECLARED dependencies, which is exactly the artifact
//! this test is about.

use std::collections::{HashMap, HashSet, VecDeque};

/// The crates that define validity. `kaspa-consensus-core` carries the pure tag/seed/projection
/// helpers, `kaspa-pow` the finalizer and the runtime SLOT, `kaspa-consensus` the pipeline that
/// consumes them. If any of these could reach a runtime crate, "the full node runs no model"
/// would be a fact about today's call sites instead of a fact about the build.
const GUARDED: &[&str] = &["kaspa-consensus", "kaspa-consensus-core", "kaspa-pow"];

/// Every workspace crate that drives, supervises, or IS a model runtime. Not listed: the
/// BASE-0 arithmetic crates (`misaka-palw-base0*`, `misaka-palw-reference2`) — those are pure
/// integer courts with no model, no subprocess and no I/O, and the court being in consensus is
/// the design, not a leak.
const FORBIDDEN: &[&str] = &[
    "misaka-palw",            // VLT compute bridge: spawns the pinned worker as a subprocess
    "misaka-palw-pow-driver", // the legacy algo-4/5 tag driver PR-02 extracts out of kaspa-pow
    "misaka-palw-worker",     // the pinned llama.cpp harness itself
    "misaka-palw-agent",      // its UDS supervisor
    "misaka-palw-reexecutor", // re-execution client
    "misaka-palw-shadow",     // shadow drill harness
];

/// Declared dependencies of every workspace package, as `name -> [(dep name, kind)]`.
/// `kind` is `cargo metadata`'s: `null` = normal, `"build"`, `"dev"`.
fn workspace_graph() -> HashMap<String, Vec<(String, String)>> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.ancestors().nth(2).expect("consensus/pow sits two levels under the workspace root");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(&cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .expect("cargo metadata runs");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("cargo metadata emits JSON");
    let packages = doc.get("packages").and_then(|v| v.as_array()).expect("metadata carries a packages array");
    packages
        .iter()
        .map(|package| {
            let name = package.get("name").and_then(|v| v.as_str()).expect("a package has a name").to_owned();
            let deps = package
                .get("dependencies")
                .and_then(|v| v.as_array())
                .map(|deps| {
                    deps.iter()
                        .map(|dep| {
                            let dep_name = dep.get("name").and_then(|v| v.as_str()).expect("a dependency has a name").to_owned();
                            let kind = dep.get("kind").and_then(|v| v.as_str()).unwrap_or("normal").to_owned();
                            (dep_name, kind)
                        })
                        .collect()
                })
                .unwrap_or_default();
            (name, deps)
        })
        .collect()
}

/// Every workspace package reachable from `start` over normal + build edges (the edges that are
/// compiled into the artifact). Dev edges do not propagate — a dependency's dev-dependencies are
/// never built — so they are checked separately, at the guarded roots only.
fn compiled_closure(graph: &HashMap<String, Vec<(String, String)>>, start: &str) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::from([start]);
    while let Some(package) = queue.pop_front() {
        let Some(deps) = graph.get(package) else { continue }; // external crates end the walk
        for (dep, kind) in deps {
            if kind != "dev" && seen.insert(dep.clone()) {
                queue.push_back(dep);
            }
        }
    }
    seen
}

#[test]
fn consensus_reaches_no_model_runtime_crate() {
    let graph = workspace_graph();
    for guarded in GUARDED {
        assert!(graph.contains_key(*guarded), "{guarded} must exist in the workspace for this test to mean anything");
        let closure = compiled_closure(&graph, guarded);
        for forbidden in FORBIDDEN {
            assert!(
                !closure.contains(*forbidden),
                "{guarded} reaches {forbidden} through its compiled dependency graph — the consensus build \
                 carries a model-runtime edge, which ADR-0042 Decision 4 forbids. Inference belongs in the \
                 driver/sidecar crates, wired in by a composition root; move the code, not this test."
            );
        }
        // Dev-dependencies do not ship, but a guarded crate's own tests pulling the driver in is
        // how a runtime edge grows back — a test helper today is an import away from the library
        // tomorrow. The guarded crates' tests must run model-free, full stop.
        for (dep, kind) in &graph[*guarded] {
            assert!(
                !(kind == "dev" && FORBIDDEN.contains(&dep.as_str())),
                "{guarded} declares {dep} as a dev-dependency — even the consensus crates' tests must not \
                 link a runtime-reaching crate (ADR-0042 Decision 4). Driver behavior is tested in the \
                 driver's own crate."
            );
        }
    }
}

/// The daemon is the composition root: it MAY link the driver (`misaka-palw-pow-driver`) and the
/// VLT compute bridge (`misaka-palw`) to wire the legacy lane in where a network demands it. It
/// must never link the runtime itself — the worker and its supervisor are separate PROCESSES,
/// and folding either into the daemon binary would put a model loader back inside the node that
/// Decision 4 just removed it from.
#[test]
fn the_daemon_links_drivers_not_runtimes() {
    let graph = workspace_graph();
    let closure = compiled_closure(&graph, "kaspad");
    for runtime in ["misaka-palw-worker", "misaka-palw-agent"] {
        assert!(
            !closure.contains(runtime),
            "kaspad reaches {runtime} — the model runtime must stay a sidecar process, never a library \
             inside the daemon (ADR-0042 Decision 4)."
        );
    }
}
