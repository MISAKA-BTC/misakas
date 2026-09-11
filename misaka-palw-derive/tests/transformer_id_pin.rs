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
const SOURCE_TREE: &str = "fa80f7680783b644eb90a2722f5ca0cc2f001788f4fe7a82e521cf1005585303";

/// `(transformer name, transformer_id)` for every transformer this build registers.
const PINNED: &[(&str, &str)] = &[
    (
        "cad/stl/v1",
        "902ca5144d15c1f09a143b0078424bde981c8b2dc5d677ab8818dedc86f4a1c383cff7d8ac0b1445899c2db7e7ef7e25aa749a8811cdc7d45e837b2f883c0935",
    ),
    (
        "code/evm/v1",
        "c691c66ee5ccb7f575772683dae599c69ff50d4084ba445a9f7b6ac2893f22d56e1b755c212545a0490badad0cb5835821f7c2e349702914168533fac6588403",
    ),
    (
        "contract/evm/v1",
        "a5b34c70a41b2fe6513a2b8f6b2b9abaed0f59b38ece3ce2eb39d4af4b71c74834efece6683e835b0abe35268fb76942c805c7d3e728d6acf450bbc39ca51572",
    ),
    (
        "image/png/v1",
        "afc299335937bb65552ce74984569684cc468b5799a5abecf535575177f5e5bbcf0c6b418aaec0f2deb8f02f30d01f6a3e4e262b440b2f7e8fbd0c5a6291d5f9",
    ),
    (
        "json/canonical/v1",
        "74d4a76930d7beec8848c74edf05f1e8f982e1b585576db7dc9fd6615cceb7c4d754130962966eab4283675271a4d4603814041c1732b88f99f229e67bfd8b37",
    ),
    (
        "map/mmap/v1",
        "9a0f3765c6ea48efa211985a671878e6b9f85d5c42d118719d92db546e17603e2451e67cd7e229a830f654cfb3557e2f3ef82f82c113815b1fa58d17f174a613",
    ),
    (
        "music/smf/v1",
        "4f47c5786c58affbf0a24e3203d50728385e26aece93737d56b79eebe95cbfff91d10ba03b3ee4cf22cdd836bbe295537fcf810210c8b4cd16a77afa1e5eb50e",
    ),
    (
        "scene/glb/v1",
        "62931736fd5b306f158a9b945757c7ce8076ce8a9115297b00cb91afa466a62edc2ff7b92d629a8a13a0dce06272ebd1233ea5ac1ec8079b99382944a67c1cb2",
    ),
    (
        "simulation/trace/v1",
        "c92cdad2eca5a3326537281ff48cdf40df139187329accbae7f4a7e96baf4e115f10efc8e4c071fc4498d1cd86d69f580649a71462def3ef3d7f09afae238734",
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
