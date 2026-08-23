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
    /// **A fresh clone must build.** `misaka-palw-worker` links a pinned llama.cpp tree this
    /// repository does not contain, so it is excluded from `default-members`; every other member
    /// stays in. Two hand-maintained lists drift, and the way this one drifts is that somebody adds
    /// a crate to `members`, never notices it is absent from `default-members`, and `cargo build`
    /// quietly stops building it. So the lists are compared rather than trusted.
    ///
    /// The reason the exclusion exists at all: `build.rs` defaulted `MISAKA_LLAMA_SRC` to a path
    /// under one developer's home directory. On that machine the workspace built; on every other
    /// machine `cc` was handed include paths that do not exist and died on `#include "llama.h"`.
    /// An operator following the testnet-11 join instructions hit it on `cargo build --release`.
    #[test]
    fn default_members_covers_every_member_but_the_pinned_runtime() {
        const EXCLUDED: &[&str] = &["misaka-palw-worker"];

        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"))
            .expect("the workspace manifest sits one directory above this crate");
        let list = |key: &str| -> Vec<String> {
            let start = manifest.find(&format!("\n{key} = [")).unwrap_or_else(|| panic!("{key} is not in the workspace manifest"));
            let body = &manifest[start..];
            let end = body.find("\n]").expect("the list is closed");
            body[..end]
                .lines()
                .skip(1)
                .filter_map(|l| l.trim().strip_prefix('"').and_then(|l| l.split('"').next()))
                .map(str::to_owned)
                .collect()
        };

        let members = list("members");
        let defaults = list("default-members");
        assert!(!members.is_empty() && !defaults.is_empty(), "both lists are non-empty");

        let missing: Vec<_> = members.iter().filter(|m| !defaults.contains(m) && !EXCLUDED.contains(&m.as_str())).collect();
        assert!(missing.is_empty(), "these members are not in the default build and are not deliberately excluded: {missing:?}");

        let stale: Vec<_> = defaults.iter().filter(|d| !members.contains(d)).collect();
        assert!(stale.is_empty(), "default-members names crates that are not workspace members: {stale:?}");

        for e in EXCLUDED {
            assert!(members.iter().any(|m| m == e), "{e} must still be a member — it shares the lockfile and lints");
            assert!(!defaults.iter().any(|d| d == e), "{e} must stay out of the default build; a fresh clone cannot build it");
        }
    }
}
