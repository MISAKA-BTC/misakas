//! **The nine `transformer_id`s, pinned — because otherwise they move in silence.**
//!
//! ADR-0078 Decision 3 makes a transformer's manifest carry "the build's source-tree hash", so
//! that `transformer_id` names the code. `build.rs` computes that hash over **every non-dot file
//! under `src/`, bytes and all** — `#[cfg(test)]` modules, doc comments and message strings
//! included (see `src/source_tree.rs`, which is the one spelling of the walk). Every manifest
//! quotes it. So:
//!
//! > **A single byte changed anywhere under `misaka-palw-derive/src/` moves ALL NINE
//! > `transformer_id`s at once.**
//!
//! That is correct and deliberate — Decision 3's whole point is that the id names the code — but
//! before this file the move was *invisible*. Measured on this branch, adding one doc comment and
//! rewording one error string moved every id:
//!
//! ```text
//!   source_tree  50ffcd18…  →  d2419027…
//!   scene/glb/v1 11ee2900…  →  17f041cd…        music/smf/v1 9067320c…  →  24be3094…
//!   cad/stl/v1   ccbbc7e5…  →  cde32f30…        …and the other five, likewise
//! ```
//!
//! …and **not one test went red, and no golden moved.** The corpus goldens pin `dsl_hash` and
//! `artifact_hash`, and neither is a function of `transformer_id`; all 43 reproduced exactly. A
//! grep of the tree for any of the eight id hexes finds nothing — they are computed at build time
//! and stored nowhere.
//!
//! # Why silence is the problem
//!
//! A derivation on chain carries `transformer_id`, and ADR-0078 Decision 5's promise is that a
//! consumer can re-run the named transformer and check `artifact_hash`. A consumer does that
//! through `registry::transformer_by_id`. So once a derivation exists on a public chain, a routine
//! comment fix in this crate makes every published derivation resolve to nothing — a
//! `DeriveError::UnknownTransformer` for every verifier holding the new build — while the whole
//! test suite stays green. The failure would appear as "verification is broken" long after the
//! commit that caused it, which is the shape of defect this repository keeps re-recording.
//!
//! # What this file does, and what it does NOT
//!
//! It makes the move LOUD. It does not forbid it: an id move is legitimate, and before the first
//! relaunch that publishes a derivation it is nearly free. Re-pinning is a normal act — it just
//! has to be a decided one.
//!
//! **When this test fails**, the id set moved. Decide which case you are in:
//!
//!   * *You changed the transformer's behaviour, its bounds, or its manifest.* The move is the
//!     rule working. Re-pin, and say in the commit message which ids moved.
//!   * *You changed a comment, a test, or a message.* The move is a side effect. It is still
//!     harmless **only while no derivation naming an old id exists on a chain you care about**.
//!     Before a relaunch: re-pin and move on. After one: the old build's ids must stay resolvable,
//!     or you are stranding published derivations — that is a deploy-order decision, not an edit.
//!
//! Re-pin by running `cargo run -p misaka-palw-derive --bin palw-derive -- drill --report r.json`
//! and copying `transformers[].transformer_id` and `source_tree_sha256` from it, or by reading the
//! values out of this test's own failure message. This pin is local to this crate; it is unrelated
//! to the consensus fingerprint pins and must not be re-pinned together with them.

use misaka_palw_derive::{SOURCE_TREE_SHA256_HEX, ids::transformer_id, registry};

/// The source-tree hash every manifest quotes.
///
/// **Moved 2026-09-24, from `fa80f768…` (ea2fd48e) — twice, and only the second was decided.**
/// 7681c203 added the fifth family's arm to `rendered_output_hash_for_family` in `src/derive.rs`
/// (`Qwen36V6` renders as `Qwen36`) and one assertion beside it, which moved the tree to
/// `7025691a…` and every id with it, and re-pinned nothing: this test has been red since. The
/// testnet-12 regenesis then moved `derive.rs`, `lib.rs`, `registry.rs` and `bin/palw-derive.rs`
/// again, on purpose (ADR-0152 v3.1 post-edit 4: `output_root` under the network's rule, core's
/// `CoreV1` on testnet-12; `--network` refusals for an unshipped suffix and for a network the job
/// context contradicts), and the pin below is that tree. No manifest field but the tree moved in
/// either step, so no transformer's behaviour or bounds did. `7025691a…` is not kept resolvable:
/// the only chain that ran it is the testnet-12 the regenesis replaces. `fa80f768…` is, because
/// testnet-11's 13520042 release shipped it — see `transformer_id_prior_tree.rs`.
const SOURCE_TREE: &str = "4ada6cd1a24e15cfd523e69bea02d343fbac70ffda3a9611002d0fa207712d9f";

/// `(transformer name, transformer_id)` for every transformer this build registers.
const PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "eed06cbf04b01f436967cdd95c5bcd99c2521306f80974136510cfc3e2387877cf4cc35a743f236ed9e22b50619039a0b186d2a131f99beb8e62da634d290b21",
    ),
    (
        "code/evm/v1",
        "8f63266c65dbdb81f44d87493f519beec360f54911133fe1b7ca5c0619984b1d0f4c86e038ee5991a37d09aeefba076d685bf5270dd0fe615c218b93ca24bb67",
    ),
    (
        "contract/evm/v1",
        "9a90ea9f045b343ec8303414d810e399e0cb0552fd2518abc3bbd62dde4e0975ff4e844981ba4de84539e841bc0f7e1ad1f6c6ac91a9190b2767dd912356283c",
    ),
    (
        "image/png/v1",
        "115dd87223155e06fbefb748f34c5798705c9ea2ed005bc7ea4d12ed1d7f81291f1467f6abd4176083fc84ffda7bcc6a67895126bbda10dc84e100491e874dab",
    ),
    (
        "json/canonical/v1",
        "1204b8d2690771427cb632369bdbef584f24503bdf778ecddcafd0519012fb957f6a736fd1ad7312455deb4c7a8e158402a087a575c8150d9579f609bf2aa2bb",
    ),
    (
        "map/mmap/v1",
        "7fcabe3c56a74ce399ee472e50b5ff57bd1783dd26734dd59309d9fec15404b5f5ef51cac60dbec81d0cfbd857bf4e4f751de1e2e309a14804726662f33c3040",
    ),
    (
        "music/smf/v1",
        "90f5818727b4dae045835e4f19bb7173b2682b3a58f1374b29448157ac48ef1e1a8f09adbde5f9bda20c75b808cf7a1c6afac1e465f2625a04d66e7202812cee",
    ),
    (
        "scene/glb/v1",
        "6d758210911d96d724170d65339ce153f1beff08d973fd33b0a9e816a4ce40c4b805b62dd106f0e949e42558b4e810feb95b38d9fd5f0d5e2f38f8e1df7baa7d",
    ),
    (
        "simulation/trace/v1",
        "37ac660569b5eadddb30a4113e35caa031a3a81bf27b6c9a5b084b0b8fa76ccd2fd68dff36fc8a85c9027477900735914228f90d0d9d2827ec5bd5ce08076bb2",
    ),
];

fn actual() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = registry::transformer_names()
        .into_iter()
        .map(|(name, _, _)| {
            let m = registry::transformer_by_name(name).expect("just enumerated").manifest();
            (name.to_string(), faster_hex::hex_string(transformer_id(&m).as_byte_slice()))
        })
        .collect();
    rows.sort();
    rows
}

/// **The pin.** Every registered transformer's id is the one this build was reviewed with.
#[test]
fn the_transformer_ids_are_the_ones_this_build_was_pinned_with() {
    let got = actual();
    let want: Vec<(String, String)> = PINNED.iter().map(|(n, i)| (n.to_string(), i.to_string())).collect();
    if got != want {
        let mut report = String::new();
        report.push_str(&format!("\n  source_tree_sha256\n    pinned {SOURCE_TREE}\n    actual {SOURCE_TREE_SHA256_HEX}\n"));
        report.push_str("\n  the pin, as it should now read:\n");
        for (name, id) in &got {
            report.push_str(&format!("    (\"{name}\", \"{id}\"),\n"));
        }
        panic!(
            "ADR-0078 Decision 3: the transformer id set moved.\n\
             Every id is a function of `source_tree_sha256`, which covers EVERY byte under `misaka-palw-derive/src/` \
             — a comment or a test is enough. Nothing else in the tree pins these, and the corpus goldens do NOT \
             catch the move (they pin dsl_hash and artifact_hash, neither of which depends on transformer_id).\n\n\
             Decide before you re-pin: a derivation already published on a chain names an OLD id, and a build that \
             cannot resolve it cannot verify it (ADR-0078 Decision 5). Before a relaunch this is nearly free; after \
             one it is a deploy-order decision. See this file's header.{report}"
        );
    }
    assert_eq!(SOURCE_TREE_SHA256_HEX, SOURCE_TREE, "the source-tree hash moved but every id matched: that cannot happen");
}

/// The pin covers the whole registry — a tenth transformer must be pinned, not merely appear.
#[test]
fn the_pin_covers_every_registered_transformer() {
    assert_eq!(
        actual().len(),
        PINNED.len(),
        "this build registers {} transformers and the pin lists {}: a transformer that is not pinned is one whose id \
         can move without anybody noticing, which is the whole thing this file exists to prevent",
        actual().len(),
        PINNED.len()
    );
}

/// The manifest bounds SA-2 declares are in the id's preimage, so a loosened bound cannot keep the
/// pinned id. Stated here as well as in `ids.rs` because this is the file a reader lands on when
/// the pin fires, and "did somebody widen a ceiling?" is the first question worth asking.
#[test]
fn every_pinned_transformer_still_declares_the_three_sa2_ceilings() {
    for (name, _) in PINNED {
        let m = registry::transformer_by_name(name).unwrap_or_else(|| panic!("{name} is pinned but not registered")).manifest();
        assert!(misaka_palw_derive::check_declared_bounds(&m).is_ok(), "{name} ships a zero ceiling (ADR-0078 SA-2)");
    }
}
