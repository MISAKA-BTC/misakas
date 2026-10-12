//! **The `full` depth** (RFC-0002 Part II §II.2.2): the artifact is supplied and verified. The shape depth said what the headers and the
//! rules say; this one adds what only the bytes can: the runtime pack (`palw-class pack verify`: the source, the frontend, the artifact's
//! digest and root, the declared classes, the conformance vectors on every executor, the Hugging Face fit) and, with a node's facts, what
//! the chain already holds over that artifact root and how many seats hold the class.
//!
//! Two codes join the table (§II.2.4): `ARTIFACT_ROOT_KNOWN` (register: a class is already registered over this root — the
//! registration would meet `palw_artifact_root_ownership`) and `PACK_NOT_VERIFIED` (mine: the pack is not VERIFIED, so nothing says an
//! independent executor reproduces the class). A class already on the chain is also read for `READY_SEATS_INSUFFICIENT`
//! and `INDEPENDENT_OPERATORS`, with the numbers the registry serves.

use super::node::NodeFacts;
use super::{Blocker, Stage};
use crate::runtime_pack::manifest::RuntimePackV1;
use crate::runtime_pack::verify::{Status, VerifyOpts, verify};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// What `--pack` and `--artifact` name.
#[derive(Clone, Debug, Default)]
pub struct FullInputs {
    /// The pack directory (`pack.json` and its sidecars).
    pub pack: Option<PathBuf>,
    /// The artifact to check against the pack (else the pack's own claims are all that is checked).
    pub artifact: Option<PathBuf>,
}

/// One check of the pack verification, as the report carries it.
#[derive(Clone, Debug, Serialize)]
pub struct FullCheck {
    pub name: String,
    pub status: &'static str,
    pub detail: String,
}

/// The full depth's findings.
#[derive(Clone, Debug, Serialize)]
pub struct FullInfo {
    /// `VERIFIED`, `NOT_FULLY_VERIFIED` (nothing failed, some checks were skipped), `FAILED`, or `NO_PACK`.
    pub pack: String,
    pub pack_digest: Option<String>,
    pub checks: Vec<FullCheck>,
    /// The artifact's inventory root as the pack records it (hex).
    pub artifact_root: Option<String>,
    /// The class ids the pack declares, by network.
    pub declared: Vec<(String, String)>,
    /// A class on the node's registry over this root: its id, state and readiness.
    pub on_chain: Vec<String>,
}

/// Run the full depth: verify the pack, then read the node's registry for the artifact root and the declared classes. Returns the
/// findings and the blockers they raise.
pub fn judge_full(inputs: &FullInputs, model: Option<&Path>, network: Option<&str>, node: Option<&NodeFacts>) -> (FullInfo, Vec<Blocker>) {
    let mut blockers = Vec::new();
    let Some(pack_dir) = inputs.pack.as_deref() else {
        blockers.push(
            Blocker::new(Stage::Mine, "PACK_NOT_VERIFIED", "no runtime pack was given, so nothing says an independent executor reproduces the class")
                .safe(vec![
                    "build one: palw-tir-fidelity / palw-class pack build --model <dir> --out <artifact.palwtir> --pack <dir> --calib <tokens.json>".into(),
                    "then run this again with --pack <dir> [--artifact <artifact.palwtir>]".into(),
                ]),
        );
        return (FullInfo { pack: "NO_PACK".into(), pack_digest: None, checks: Vec::new(), artifact_root: None, declared: Vec::new(), on_chain: Vec::new() }, blockers);
    };
    let mut opts = VerifyOpts::new(pack_dir);
    opts.model = model.filter(|m| m.is_dir()).map(Path::to_path_buf);
    opts.artifact = inputs.artifact.clone();
    let mut info = FullInfo { pack: "FAILED".into(), pack_digest: None, checks: Vec::new(), artifact_root: None, declared: Vec::new(), on_chain: Vec::new() };
    match verify(&opts, &|_| {}) {
        Ok(report) => {
            info.pack_digest = Some(report.pack_digest.clone());
            info.checks = report
                .checks
                .iter()
                .map(|c| FullCheck {
                    name: c.name.clone(),
                    status: match c.status {
                        Status::Pass => "PASS",
                        Status::Fail => "FAIL",
                        Status::Skipped => "SKIPPED",
                    },
                    detail: c.detail.clone(),
                })
                .collect();
            info.pack = if report.verified() { "VERIFIED" } else if report.ok() { "NOT_FULLY_VERIFIED" } else { "FAILED" }.into();
            if !report.verified() {
                let why: Vec<String> = report
                    .checks
                    .iter()
                    .filter(|c| c.status != Status::Pass)
                    .map(|c| format!("{} {}: {}", if c.status == Status::Fail { "FAIL" } else { "SKIPPED" }, c.name, c.detail))
                    .collect();
                blockers.push(
                    Blocker::new(
                        Stage::Mine,
                        "PACK_NOT_VERIFIED",
                        if report.ok() {
                            "the pack is not fully verified: some checks could not be made, so nothing yet says an independent executor reproduces the class"
                        } else {
                            "the pack does not verify: the artifact does not follow from it"
                        },
                    )
                    .evidence(why)
                    .safe(vec!["palw-class pack verify <pack> --model <dir> [--artifact <file>] shows each check".into()]),
                );
            }
        }
        Err(e) => {
            blockers.push(Blocker::new(Stage::Mine, "PACK_NOT_VERIFIED", format!("the pack could not be read: {e}")));
            return (info, blockers);
        }
    }
    // The pack's own claims about the artifact and the classes it declares.
    if let Ok(text) = std::fs::read_to_string(pack_dir.join("pack.json"))
        && let Ok(pack) = RuntimePackV1::parse(&text)
    {
        info.artifact_root = Some(pack.result.inventory_root.clone());
        info.declared = pack.declared.iter().map(|d| (d.network.clone(), d.class_id.clone())).collect();
    }
    if let Some(node) = node {
        if let Some(root) = &info.artifact_root {
            for c in node.classes_over_root(root) {
                info.on_chain.push(format!("{} ({})", c.class_id, c.state));
            }
            if !info.on_chain.is_empty() {
                blockers.push(
                    Blocker::new(
                        Stage::Register,
                        "ARTIFACT_ROOT_KNOWN",
                        "a class is already registered over this artifact root: the registration would meet palw_artifact_root_ownership",
                    )
                    .evidence(info.on_chain.clone())
                    .safe(vec!["the class on the chain is the model: run a seat for it (`misaka model readiness <class>`) instead of registering again".into()]),
                );
            }
        }
        // A declared class already on the chain: its seats.
        for (net, id) in &info.declared {
            if network.is_some_and(|n| n != net) {
                continue;
            }
            if let Some(c) = node.class(id) {
                if c.ready_seats < c.required_ready_seats {
                    blockers.push(
                        Blocker::new(
                            Stage::Mine,
                            "READY_SEATS_INSUFFICIENT",
                            format!("{} of {} seats hold the class with a fresh possession proof", c.ready_seats, c.required_ready_seats),
                        )
                        .numbers(u64::from(c.ready_seats), u64::from(c.required_ready_seats), "seats")
                        .safe(vec!["run seats that hold the artifact: `misaka model readiness <class>` lists each seat".into()]),
                    );
                }
                if let Some(s) = &c.seating
                    && s.independent_operators < s.needed_independent
                {
                    blockers.push(
                        Blocker::new(
                            Stage::Mine,
                            "INDEPENDENT_OPERATORS",
                            format!(
                                "{} of the operators that hold the class are independent of its registrant; {} are needed (licensable share {} ‰ of {} base operators)",
                                s.independent_operators, s.needed_independent, s.licensable_share_permille, s.base_operators
                            ),
                        )
                        .numbers(u64::from(s.independent_operators), u64::from(s.needed_independent), "operators"),
                    );
                }
            }
        }
    }
    (info, blockers)
}
