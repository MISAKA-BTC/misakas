//! **The nine `transformer_id`s, pinned — because otherwise they move in silence.**
//!
//! ADR-0078 Decision 3 makes a transformer's manifest carry "the build's source-tree hash", so
//! that `transformer_id` names the code. `build.rs` computes that hash over **every non-dot file
//! under `src/`, bytes and all** — `#[cfg(test)]` modules, doc comments and message strings
//! included (see `src/source_tree.rs`, which is the one spelling of the walk). Every manifest
//! quotes it. So:
//!
//! > **A single byte changed anywhere under `misaka/palw/derive/src/` moves ALL NINE
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

#![allow(warnings)]
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
///
/// **Moved 2026-10-28, from `4ada6cd1…`.** rustfmt of `src/` moved the source-tree hash and
/// therefore every transformer id. The corpus goldens (`dsl_hash`, `artifact_hash`) did not move,
/// and no manifest field but the tree did.
const SOURCE_TREE: &str = "f4fff57719e5be81ef4f198a7429f1d78b87d56cdd5c316c0ed0246e2078a950";

/// `(transformer name, transformer_id)` for every transformer this build registers.
const PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "817a2d0ae3c5f6c4f037f544f1cbc5332997a4427043e8eb4a63b3a0a1c63b7a91e7d19ab588ecca01bd36ff94acd0c2ceb95e07aeda6c82d0fd17a7072dab3e",
    ),
    (
        "code/evm/v1",
        "90fb0270f3066fd8c2b8dc44ba0e1a810982344882ea5e52d4082fc0d1a2a97ffbe42194a84d72e60f265b8f984a29c3c07129ef496c9f0933d572eebd3e18ae",
    ),
    (
        "contract/evm/v1",
        "a17d720ee90f0c7b7716b6c0639520dfdc4c4185b2268d2d453070fe2a6d576bd2b432441ecd2c8be96063265cfe66576ba997df58fa798b741ebecc86c55845",
    ),
    (
        "image/png/v1",
        "ca87e714454208da53af9fff9ea9bc855df3649fa7f0bcfa74c8d8b13d585c4c1bc9dcc3d8e594f7a40f364208bdb1a205c8632a41eb1622ad96f233ea7edf00",
    ),
    (
        "json/canonical/v1",
        "c6aeaf81e7b1a6b833688f2af65ebd478156ff83a7ceec3401b83a7c54fa6a80d4b90c392a8ec798c33a0b502aacc2fae0e621a0a222cafa81f0fe88f98cde8b",
    ),
    (
        "map/mmap/v1",
        "f84a4d287165948826bfb14e8ec4d982f2a5572fb5e75de0d23f9a3cc240ef0603e47abe29c2148ac2256baffb8d655e72738274e417a0173b0de1257d7b0355",
    ),
    (
        "music/smf/v1",
        "199290f09692c7831e6b2e17a0b97ba355d3dcf96c904b90ede6f23e83232eeab2d46afa6025506aea054584c8d2931091133d1371b563bdd27acdd52d61fb46",
    ),
    (
        "scene/glb/v1",
        "bab816eb1d1552c8d9c7e50b3d3e42c9f7c92dbb2aae1dbcd023ae85deaa9fbf5bb161cefed48e0eaffd842026dbb865ad26084ce9f5a44675075e2fa2c342e9",
    ),
    (
        "simulation/trace/v1",
        "cbd711cbe07a7877843a1c6652a24df14c62aad7e0a1c26d7742173d6a2c642751cb6c9f0f480dfc6fecd0bee2303e754c8c7c06df293303f7f554b9d49340c0",
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
             Every id is a function of `source_tree_sha256`, which covers EVERY byte under `misaka/palw/derive/src/` \
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
