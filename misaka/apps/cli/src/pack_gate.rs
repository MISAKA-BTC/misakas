//! **The pack gate of a registration** (RFC-0002 Part II §II.7.5, decided 2026-10-01).
//!
//! A registration of an IR class (`misaka palw tir-registration`, and `misaka model add` for an IR artifact) **requires a runtime
//! pack that verifies** against the artifact being registered: `palw-class pack verify <pack> --artifact <file>` with nothing
//! failed and, of the checks that bind the pack to *this* artifact, `artifact` (file digest, inventory root, graph root, tokenizer)
//! and `conformance` (the vectors on every executor) passed — not skipped. An expert may register without one with
//! `--skip-pack-verify`, which prints a warning that nothing the pack would have checked was checked.
//!
//! This is a policy of the CLI, **not of the chain**: the chain does not read a pack, and a registration made any other way is valid
//! (RFC-0002 Part II P7). What the gate prevents is the usual way to waste a fee and a fleet's disk: registering an artifact whose
//! executors do not agree with its own program, or one that is not the artifact its pack says.

use crate::{CliError, exit};
use clap::Args;
use misaka_palw_sdk::runtime_pack::{Status, VerifyOpts, verify_pack};
use std::path::{Path, PathBuf};

/// The checks of a verification that must PASS: they tie the pack to the artifact being registered.
const REQUIRED_CHECKS: &[&str] = &["artifact", "conformance"];

/// The flags every registering command takes.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct PackGateFlags {
    /// The runtime pack of the artifact (`palw-class pack build --pack <dir>`). A registration of an IR class needs a pack that
    /// verifies against the artifact (`palw-class pack verify`), unless `--skip-pack-verify`.
    #[arg(long, value_name = "DIR", conflicts_with = "skip_pack_verify")]
    pub(crate) pack: Option<PathBuf>,
    /// The model's public source (a Hugging Face directory or a GGUF file), to check the source files and the frontend against the
    /// pack as well. Without it those checks are skipped, and the report says so.
    #[arg(long, value_name = "PATH", requires = "pack")]
    pub(crate) pack_source: Option<PathBuf>,
    /// An expert's override: register WITHOUT a verified runtime pack. A warning is printed that nothing the pack would have checked
    /// (the source, the frontend, the artifact's roots, conformance on three executors, the reference fit) was checked. The chain
    /// does not require a pack either way.
    #[arg(long)]
    pub(crate) skip_pack_verify: bool,
}

/// What the gate decided, for the caller to print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PackGate {
    /// The pack verified; the report to show, and how many checks were skipped.
    Verified { report: String, skipped: usize },
    /// The expert override was given; the warning to show.
    Skipped { warning: String },
}

impl PackGate {
    /// The lines to print (to stderr: the command's own output stays what it was).
    pub(crate) fn lines(&self) -> Vec<String> {
        match self {
            PackGate::Verified { report, skipped } => {
                let mut out = vec![report.trim_end().to_string()];
                if *skipped > 0 {
                    out.push(format!(
                        "{skipped} check(s) of the pack were skipped; give --pack-source <the model's directory or GGUF> to check the source and the frontend as well"
                    ));
                }
                out
            }
            PackGate::Skipped { warning } => vec![warning.clone()],
        }
    }

    /// For `--json` output: `verified` or `skipped`.
    pub(crate) fn word(&self) -> &'static str {
        match self {
            PackGate::Verified { .. } => "verified",
            PackGate::Skipped { .. } => "skipped",
        }
    }
}

/// **Ask the gate** about `artifact`: refuse (naming what to do) unless the pack verifies or the override is given.
pub(crate) fn require_verified_pack(flags: &PackGateFlags, artifact: &Path) -> Result<PackGate, CliError> {
    if flags.skip_pack_verify {
        return Ok(PackGate::Skipped {
            warning: "WARNING: --skip-pack-verify: this registration was NOT checked against a runtime pack. Nothing a pack verifies — the source's \
                      hashes, the frontend, the artifact's roots, conformance on the reference evaluator, the typed backend and the independent \
                      implementation, the fit against the Hugging Face reference — was checked. The chain does not require it; a seat that finds the \
                      executors disagree will find out by voided claims."
                .to_string(),
        });
    }
    let Some(pack) = &flags.pack else {
        return Err(CliError::new(
            exit::MODEL,
            "registering an IR class needs a runtime pack that verifies against the artifact: build one with `palw-class pack build` and pass \
             --pack <dir> (and --pack-source <the model> to check the source too), or pass --skip-pack-verify to register without one (a warning \
             is printed; the chain does not enforce a pack)",
        ));
    };
    let mut opts = VerifyOpts::new(pack);
    opts.artifact = Some(artifact.to_path_buf());
    opts.model = flags.pack_source.clone();
    let report = verify_pack(&opts, &|m| eprintln!("{m}")).map_err(|e| CliError::new(exit::MODEL, format!("pack verify: {e}")))?;
    let rendered = report.render();
    if !report.ok() {
        return Err(CliError::new(
            exit::MODEL,
            format!("the runtime pack does not verify against {} — nothing is registered:\n{rendered}", artifact.display()),
        ));
    }
    for name in REQUIRED_CHECKS {
        if !report.checks.iter().any(|c| c.name == *name && c.status == Status::Pass) {
            return Err(CliError::new(
                exit::MODEL,
                format!(
                    "the runtime pack's `{name}` check did not pass (it was skipped or not run): a registration needs the pack tied to this \
                     artifact — nothing is registered:\n{rendered}"
                ),
            ));
        }
    }
    let skipped = report.checks.iter().filter(|c| c.status == Status::Skipped).count();
    Ok(PackGate::Verified { report: rendered, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(pack: Option<&str>, skip: bool) -> PackGateFlags {
        PackGateFlags { pack: pack.map(PathBuf::from), pack_source: None, skip_pack_verify: skip }
    }

    #[test]
    fn a_registration_without_a_pack_is_refused_and_says_what_to_do() {
        let e = require_verified_pack(&flags(None, false), Path::new("/nonexistent/model.palwtir")).unwrap_err();
        assert_eq!(e.code, exit::MODEL);
        assert!(
            e.msg.contains("palw-class pack build") && e.msg.contains("--pack <dir>") && e.msg.contains("--skip-pack-verify"),
            "{}",
            e.msg
        );
    }

    #[test]
    fn the_expert_override_registers_and_warns() {
        let g = require_verified_pack(&flags(None, true), Path::new("/nonexistent/model.palwtir")).unwrap();
        assert_eq!(g.word(), "skipped");
        let lines = g.lines().join("\n");
        assert!(
            lines.contains("WARNING")
                && lines.contains("NOT checked against a runtime pack")
                && lines.contains("chain does not require"),
            "{lines}"
        );
    }

    #[test]
    fn a_pack_that_cannot_be_read_is_an_error_not_a_pass() {
        let e =
            require_verified_pack(&flags(Some("/nonexistent/pack-dir"), false), Path::new("/nonexistent/model.palwtir")).unwrap_err();
        assert_eq!(e.code, exit::MODEL);
        assert!(e.msg.contains("pack verify"), "{}", e.msg);
    }

    #[test]
    fn the_flags_exclude_each_other_and_the_source_needs_a_pack() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct T {
            #[command(flatten)]
            g: PackGateFlags,
        }
        assert!(T::try_parse_from(["t", "--pack", "p"]).is_ok());
        assert!(T::try_parse_from(["t", "--skip-pack-verify"]).is_ok());
        assert!(
            T::try_parse_from(["t", "--pack", "p", "--skip-pack-verify"]).is_err(),
            "a pack and its override together are refused at the parser"
        );
        assert!(T::try_parse_from(["t", "--pack-source", "m"]).is_err(), "a source with no pack is refused");
        assert!(T::try_parse_from(["t", "--pack", "p", "--pack-source", "m"]).is_ok());
    }
}
