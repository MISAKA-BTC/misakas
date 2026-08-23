pub mod args;
pub mod chain_participation_store;
pub mod compute;
pub mod daemon;
pub mod palw_agent;
#[cfg(feature = "evm")]
pub mod eth_rpc;
pub mod palw_panel;
pub mod palw_backends;
pub mod palw_producer;
pub mod validator_service;

#[cfg(test)]
mod workspace_default_members {
    /// **A fresh clone must build, and the default set must not quietly lose a crate.**
    ///
    /// `misaka-palw-worker` links a pinned llama.cpp tree this repository does not contain, so it
    /// is the one member excluded from `default-members`; everything else has to stay in.
    ///
    /// This asks CARGO, not the manifest. The first version of this test parsed the two arrays out
    /// of `Cargo.toml` and compared them — and passed while the default build had silently dropped
    /// `kaspa-p2p-mining`, because **Cargo adds a member's path dependencies to the workspace even
    /// when they are not listed in `members`**, and `default-members` is exact. Two text lists that
    /// agree with each other can both disagree with the workspace, so the workspace is the thing to
    /// ask. (`protocol/mining` is now named explicitly as well, so the manifest says what it means.)
    ///
    /// The reason the exclusion exists at all: `build.rs` defaulted `MISAKA_LLAMA_SRC` to a path
    /// under one developer's home directory. On that machine the workspace built; everywhere else
    /// `cc` was handed include paths that do not exist and died on `#include "llama.h"`. An
    /// operator following the testnet-11 join instructions hit it on `cargo build --release`.
    #[test]
    fn default_members_covers_every_member_but_the_pinned_runtime() {
        const EXCLUDED: &[&str] = &["misaka-palw-worker"];

        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml");
        let out = std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path", root])
            .output()
            .expect("cargo metadata runs");
        assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
        let json = String::from_utf8(out.stdout).expect("metadata is utf-8");

        // Package ids carry the crate name; the surrounding syntax has changed between cargo
        // versions, so membership is tested by containment rather than by parsing an id format.
        let list = |key: &str| -> Vec<String> {
            let at = json.find(&format!("\"{key}\":[")).unwrap_or_else(|| panic!("{key} is not in cargo metadata"));
            let body = &json[at + key.len() + 4..];
            let end = body.find(']').expect("the array closes");
            body[..end].split(',').map(|s| s.trim().trim_matches('"').to_owned()).filter(|s| !s.is_empty()).collect()
        };
        let members = list("workspace_members");
        let defaults = list("workspace_default_members");
        assert!(members.len() > 50, "the workspace has many members; got {}", members.len());

        let missing: Vec<&String> = members
            .iter()
            .filter(|m| !defaults.contains(m) && !EXCLUDED.iter().any(|e| m.contains(e)))
            .collect();
        assert!(missing.is_empty(), "these members are not in the default build and are not deliberately excluded: {missing:?}");

        for e in EXCLUDED {
            assert!(members.iter().any(|m| m.contains(e)), "{e} must still be a member — it shares the lockfile and lints");
            assert!(!defaults.iter().any(|d| d.contains(e)), "{e} must stay out of the default build; a fresh clone cannot build it");
        }
    }
}
