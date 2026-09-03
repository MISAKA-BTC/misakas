//! **The eight `transformer_id`s, pinned — because otherwise they move in silence.**
//!
//! ADR-0078 Decision 3 makes a transformer's manifest carry "the build's source-tree hash", so
//! that `transformer_id` names the code. `build.rs` computes that hash over **every non-dot file
//! under `src/`, bytes and all** — `#[cfg(test)]` modules, doc comments and message strings
//! included (see `src/source_tree.rs`, which is the one spelling of the walk). Every manifest
//! quotes it. So:
//!
//! > **A single byte changed anywhere under `misaka-palw-derive/src/` moves ALL EIGHT
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
const SOURCE_TREE: &str = "d2419027673f94ebbc46ad99b847297dc3c6d71a2dadbc4a902fd4453a7ff658";

/// `(transformer name, transformer_id)` for every transformer this build registers.
const PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "cde32f30e56e4368a406e78ab66cb3b78ce770504a6372cc0ce95e8827360404fd107679c1f39e0b88db195e3df1dc8044c83e6b20a79e4d6af6c83b9ddf571e",
    ),
    (
        "code/evm/v1",
        "bdfb5c0860867eebb669499acd983c5493bf9158edae4780fc7d2c4e54c8e23506d53d7129887f823b939363a834ca91a00786888aec545f0dbce6bd7f8d54ed",
    ),
    (
        "contract/evm/v1",
        "52a1037e97920c1ca14ef7b373c93e9905c0cd54b32d33021e593198af7c8c7daca5ac1196933fc04f9933e4b5d43ad585efe120da690aad201f137427e782fc",
    ),
    (
        "image/png/v1",
        "d2723e17a048943921620d062d085643bd2f8e8f89761e4789354321bc1e8728816778df395aec5795f8d864c468ab6931588116896cbf4dae928ad198b0b0f1",
    ),
    (
        "map/mmap/v1",
        "25c87c8355ff6423ef0e03b8132014fe404977ee2d280f3cedb35d4fba32492231d8d38d1fb5c7474efa5d180e46ff6fd0e8fbcf60a89df7c9b29cb36ec30b1a",
    ),
    (
        "music/smf/v1",
        "24be30948de1e5861b37c84d80b26bf615c9877493afd3bd6d3d000fe952ab932a70948313bbd45a2ecd5a90618e33fc35ebd009f8150e851841fa31eaf7e80c",
    ),
    (
        "scene/glb/v1",
        "17f041cda4da4743926275b2d5dfc43af1065561685e6553bec2db06e7f03f1aec0d0b5c02ac52eab5783b80cb2a6e65103462598d111aae10b0e80672312725",
    ),
    (
        "simulation/trace/v1",
        "439fe3bbaa615d0ed95fdf259bc0463f4ea9325d39d9ccc05f40d27b69b56317785f92b3b573d8c2b729b98b39653567932dc860b40c8478d9629f3b536c6db9",
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

/// The pin covers the whole registry — a ninth transformer must be pinned, not merely appear.
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
