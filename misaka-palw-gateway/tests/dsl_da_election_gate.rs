//! **ADR-0078 SA-4: the DSL data-availability election is served like an opening, or not at all.**
//!
//! > *"The DSL DA election (Decision 6) is served like an opening: to bonded requesters only,
//! > bounded, rate-limited (ADR-0077 SA-2), so opting a DSL in cannot make the executor a public
//! > file server."*
//!
//! # What is in the tree today, and what this guard is for
//!
//! Decision 6's election is HALF built, and the half that exists is the half that writes:
//!
//! * the gateway writes `<stem>.dsl-payload.fpd1` into its outbox when `--serve-dsl` is on
//!   (`misaka_palw_gateway::derive::run`, `DeriveConfig::serve_dsl` — off by default);
//! * `misaka palw fp-submit --dsl-payload` stages it into the node's retention directory as
//!   `<claim_id>.dsl` (`misaka_palw_fp_submit::plan_submission`);
//! * **and nothing reads it back out.** No node path, no flow, no RPC serves a retained `.dsl`.
//!   The election therefore cannot make an executor a public file server today, because there is
//!   no server.
//!
//! That is a good state and a fragile one. The day somebody adds the serve lane — it is Q-07 in
//! ADR-0078 §6's order of work, and it is a small change next to the openings machinery that is
//! already there — the natural implementation is a fresh handler beside the material one, and a
//! fresh handler is precisely how an unauthenticated one gets written. SA-4 exists to say that the
//! gate must be reused rather than rewritten, and this file is that sentence in a form that fails.
//!
//! # The rule, for whoever wires Q-07
//!
//! Serve a retained DSL through the SAME gate ADR-0077 SA-2 already built for openings, which is
//! `kaspa_p2p_flows::palw_gossip`:
//!
//! 1. `check_opening_request_shape` — the two length comparisons, before anything is read;
//! 2. `PalwGossipCenter::authorize_serve` — the per-peer rate, then the ML-DSA-87 signature and
//!    the chain's bond lookup off the runtime, then the per-bond rate. It refuses with
//!    `PalwServeRefusalV1::NotBonded` when no authorizer is registered, which is the fail-closed
//!    default an amplifier would otherwise enjoy;
//! 3. a byte allowance reserved BEFORE the read, and a size ceiling on what comes back —
//!    `PALW_FP_DSL_V1_MAX_BYTES` is the DSL's own, already enforced by `palw_fp_dsl_decode_v1`.
//!
//! Do not copy `resolve_material_for_serve_signed`'s "authenticate only if an authorizer is
//! registered" shape. That conditional is documented in its own body as a deliberate exception,
//! taken because a seat starved of material signs `Unavailable` and three of those slash an honest
//! producer — a liveness debt the DSL lane does not carry. A DSL nobody serves costs nobody a
//! bond, so its lane authenticates unconditionally, like `resolve_interval_opening_for_serve`.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("the gateway crate sits in the workspace").to_path_buf()
}

fn rust_sources(dir: &Path) -> Vec<(String, String)> {
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
    let mut paths = Vec::new();
    walk(dir, &mut paths);
    paths.sort();
    let root = repo_root();
    paths
        .into_iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(&root).unwrap_or(&p).display().to_string().replace('\\', "/");
            std::fs::read_to_string(&p).ok().map(|s| (rel, s))
        })
        .collect()
}

/// Non-comment lines only: a rule about what the code DOES must not fire on a sentence about what
/// the code does not do — including this file's own prose, quoted into a reviewer's notes.
fn code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.lines().enumerate().filter(|(_, line)| {
        let t = line.trim_start();
        !(t.starts_with("//") || t.starts_with('*') || t.is_empty())
    })
}

/// **SA-4's live half: no DSL serve lane exists that has not been through the gate.**
///
/// The scan is over the node and the p2p flows — the two places a serving path can live — for the
/// spellings by which a retained DSL would be read: the `.dsl` suffix the stage writes, and the
/// payload decoder that would turn those bytes back into a `PalwFpDslV1`.
///
/// When this fires, the fix is NOT to add the file to an allow-list. It is to check that the path
/// which now reads a DSL goes through `authorize_serve` first, and then to add it here with that
/// citation — the way `misaka-palw-derive`'s own tree guard names its two confinement gates.
#[test]
fn no_node_path_serves_a_retained_dsl_without_the_adr_0077_sa2_gate() {
    let root = repo_root();
    let mut readers = Vec::new();
    for dir in ["kaspad/src", "protocol/flows/src", "protocol/p2p/src", "rpc"] {
        for (path, source) in rust_sources(&root.join(dir)) {
            for (n, line) in code_lines(&source) {
                // `palw_fp_dsl_decode_v1` is the only way retained bytes become a DSL again, and
                // `.dsl"` is the only way the retained FILE is named. Either one on a node path is
                // a serving lane, or the beginning of one.
                if line.contains("palw_fp_dsl_decode_v1") || line.contains(".dsl\"") {
                    readers.push(format!("{path}:{}: {}", n + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        readers.is_empty(),
        "ADR-0078 SA-4: a node path now reads a retained DSL. It must be served like an opening — \
         `check_opening_request_shape`, then `PalwGossipCenter::authorize_serve` (bonded requester, per-peer and \
         per-bond rate), then a byte allowance reserved before the read — and NOT through a second gate written \
         beside the first. See this file's header for the wiring, then list the path here with its citation.\n  {}",
        readers.join("\n  ")
    );
}

/// **The gate SA-4 points at is still there, and is still fail-closed.**
///
/// SA-4 delegates to ADR-0077 SA-2 rather than restating it, so the delegation is only worth
/// anything while the thing delegated to exists. If this goes red, SA-4's instruction has no
/// referent and the DSL lane has nothing to be built on.
#[test]
fn the_opening_gate_sa4_delegates_to_still_exists_and_refuses_without_an_authorizer() {
    let gossip = std::fs::read_to_string(repo_root().join("protocol/flows/src/palw_gossip.rs"))
        .expect("the openings gate is in protocol/flows/src/palw_gossip.rs");
    for needle in [
        "pub fn check_opening_request_shape",
        "pub async fn authorize_serve",
        "fn charge_opening_peer_rate",
        "fn charge_opening_rate",
        "OPENING_REQUESTS_PER_BOND_PER_WINDOW",
    ] {
        assert!(gossip.contains(needle), "ADR-0077 SA-2's gate lost `{needle}`, and ADR-0078 SA-4 delegates to it");
    }
    // The fail-closed default, as text: no authorizer means refuse, never serve everyone.
    let authorize = gossip.split("pub async fn authorize_serve").nth(1).expect("authorize_serve still exists");
    let body = &authorize[..authorize.len().min(1200)];
    assert!(
        body.contains("else { return Err(PalwServeRefusalV1::NotBonded) }"),
        "ADR-0077 SA-2 / ADR-0078 SA-4: a node with no authorizer must refuse, not serve everyone — that default is \
         the amplifier the rule exists to close"
    );
}

/// **Decision 6's default is off, and "off" means there is nothing on disk to serve.**
///
/// The election is the user's, and ADR-0044 Decision 8's sentence about silently publishing
/// prompts applies to answers word for word. A default that wrote the payload and merely declined
/// to serve it would leave the disclosure one config flag away from an operator who never chose it.
#[test]
fn the_dsl_election_is_off_by_default_and_writes_nothing_when_off() {
    // The staging path refuses a DSL payload offered with nowhere to serve it from, rather than
    // dropping it silently — the plan is pure, so this is checked without touching a disk.
    let staging = misaka_palw_fp_submit::FpStaging::default();
    assert!(staging.dsl_payload.is_none(), "ADR-0078 Decision 6: the data-availability election is off by default");
    assert!(staging.retention_dir.is_none());

    let gateway = std::fs::read_to_string(repo_root().join("misaka-palw-gateway/src/derive.rs")).expect("the gateway derive step");
    assert!(
        gateway.contains("if cfg.serve_dsl"),
        "the FPD1 payload must be written only under the election, so a claim whose executor did not elect it has \
         nothing on disk for a future serve lane to find"
    );
}
