//! **AUDIT B1x — is the 2026-09-23 class-identity split enforced at the boundaries it exists for?**
//!
//! Read-only audit artefact. Nothing here is a fixture any shipped code reads. It complements
//! `audit_b1_class_identity.rs` (which showed the digest-for-root substitution still assembles) by
//! measuring (a) how far the three types reach, (b) whether the three genesis root SLOTS are
//! mutually interchangeable, and (c) which of the three t12 roots any CI check can re-derive.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::class_manifest_const_v1 as manifest;
use kaspa_consensus_core::config::params::*;
use kaspa_consensus_core::config::premine::*;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_class_identity_v1::{PalwArtifactDigestV1, PalwClassIdV1, PalwInventoryRootV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

fn t12() -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, 12)
}

fn t12_bonds() -> Vec<kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1> {
    PALW_RC_GENESIS_BONDS
        .iter()
        .map(|c| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(premine_outpoint(c.premine_index)),
            pubkey: c.bond_pubkey.to_vec(),
            operator_pubkey: c.operator_pubkey.to_vec(),
            payout_payload: Hash64::from_bytes(c.payout_payload),
        })
        .collect()
}

/// The t12 card, with each of the three root slots under the caller's control.
fn card(base0: Hash64, qwen36: Hash64, dense: Hash64) -> Result<Params, String> {
    let base = palw_t12_base_params();
    let utxos = genesis_premine_utxos_for(base.net);
    palw_v2_params_with_class_rows_v1(
        base,
        base0,
        qwen36,
        Some(dense),
        t12_bonds(),
        utxos,
        PalwGenesisClassRowsV1::HeldLadder { dense_n_ctx: PALW_T12_DENSE_N_CTX, hybrid_n_ctx: PALW_T12_HYBRID_N_CTX },
    )
    .map_err(|e| format!("{e:?}"))
}

fn registered_roots(p: &Params) -> Vec<(Hash64, Hash64)> {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => Some((*class_id, *artifact_root)),
            _ => None,
        })
        .collect()
}

/// **The laundering, as a TOTAL public function.** The module's guarantee is stated as "no
/// conversion between the types"; what the source actually forbids is `impl From<Hash64>` and
/// `Deref`. `into_hash64` plus any constructor is the same conversion, spelled in two calls.
fn launder(d: PalwArtifactDigestV1) -> PalwInventoryRootV1 {
    PalwInventoryRootV1::as_registered_on_chain(d.into_hash64())
}

#[test]
fn a_digest_converts_to_an_inventory_root_in_one_line_of_safe_public_api() {
    let digest = PalwArtifactDigestV1::measured_over_the_file(manifest::artifact_digest_of(manifest::QWEN25_A16_2M_MANIFEST_V1));
    let truth =
        PalwInventoryRootV1::rooted_over_the_inventory(manifest::inventory_root_of_class(manifest::QWEN25_A16_2M_MANIFEST_V1, 1));
    let laundered: PalwInventoryRootV1 = launder(digest);
    assert_ne!(laundered, truth, "the laundered value is the digest, not the root");
    assert_eq!(laundered.into_hash64(), digest.into_hash64(), "and it is bit-for-bit the digest");
    println!("digest                       {digest}");
    println!("launder(digest) : InventoryRoot {laundered}");
    println!("the real inventory root      {truth}");

    // The module's own forbidden list, read out of its source, does not mention the two names the
    // conversion above is built from.
    let whole = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/palw_class_identity_v1.rs"),
    )
    .expect("the module is readable");
    // Same cut the module's own guarantee test makes: its needles appear in its own test text.
    let src = &whole[..whole.find("#[cfg(test)]").expect("the module has tests")];
    for needle in ["impl From<Hash64>", "impl std::ops::Deref", "impl Deref"] {
        assert!(!src.contains(needle), "the module forbids {needle}");
    }
    assert!(src.contains("pub const fn into_hash64"), "and offers into_hash64 on all three");
    assert_eq!(src.matches("pub const fn into_hash64").count(), 3, "one escape per type");
    assert!(src.contains("pub const fn as_registered_on_chain"), "and a Hash64 -> InventoryRoot constructor");
}

/// **The three genesis root SLOTS are positional `Hash64`s and are mutually interchangeable.**
///
/// This is a different defect from "a digest can be passed as a root": here each of the three
/// registered classes' roots can be given ANOTHER CLASS's root and the card still assembles,
/// validates, and ships as a distinct chain.
#[test]
fn the_three_genesis_root_slots_accept_each_others_values() {
    let b0 = PALW_RC_GENESIS_ARTIFACT_ROOT;
    let q36 = PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT;
    let dense = PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT;

    let honest = card(b0, q36, dense).expect("the shipped card assembles");
    honest.validate_palw_v2().expect("the shipped card validates");
    println!("honest        params id {}", honest.consensus_params_id());

    // 1. base0 <-> qwen36 swapped.
    match card(q36, b0, dense) {
        Ok(p) => {
            let v = p.validate_palw_v2();
            println!("b0<->q36 swap params id {}  validate {:?}", p.consensus_params_id(), v.as_ref().err());
            assert!(v.is_ok(), "a card with the floor's and the hybrid's roots exchanged validates");
            assert_ne!(p.consensus_params_id(), honest.consensus_params_id());
        }
        Err(why) => println!("b0<->q36 swap REFUSED at assembly: {why}"),
    }

    // 2. the dense slot given the hybrid's root (one artifact, two classes claiming it).
    match card(b0, q36, q36) {
        Ok(p) => {
            let v = p.validate_palw_v2();
            let roots = registered_roots(&p);
            let dup = roots.iter().filter(|(_, r)| *r == q36).count();
            println!("dense:=q36    params id {}  validate {:?}  classes sharing that root: {dup}", p.consensus_params_id(), v.as_ref().err());
            assert!(v.is_ok(), "two registered classes may pin the same artifact root at genesis");
            assert_eq!(dup, 2, "the hybrid and the dense row now name one root");
        }
        Err(why) => println!("dense:=q36 REFUSED at assembly: {why}"),
    }

    // 3. all three the same value.
    match card(b0, b0, b0) {
        Ok(p) => {
            let v = p.validate_palw_v2();
            let roots = registered_roots(&p);
            println!("all three b0  params id {}  validate {:?}  distinct roots {}", p.consensus_params_id(), v.as_ref().err(), {
                let mut s: Vec<Hash64> = roots.iter().map(|(_, r)| *r).collect();
                s.sort();
                s.dedup();
                s.len()
            });
        }
        Err(why) => println!("all three b0 REFUSED at assembly: {why}"),
    }
}

/// **Consensus never compares a registered root against any measurement — it compares it with
/// itself.** `class_roots_in_force` returns the class's own registration plus its lines' versions,
/// and `check_palw_attempt_admission_v2` asks only for membership in that set.
#[test]
fn the_only_consensus_check_on_a_root_is_membership_in_the_set_it_came_from() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/palw_admission_v2.rs"),
    )
    .expect("readable");
    assert!(
        src.contains("if !state.class_roots_in_force(&attempt.class_id, daa).contains(&attempt.artifact_root)"),
        "the admission gate is a set-membership test over values the chain itself stored"
    );
    let st = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/palw_state_v2.rs"),
    )
    .expect("readable");
    let at = st.find("pub fn class_roots_in_force").expect("present");
    let body = &st[at..at + 900];
    assert!(body.contains("roots.push(class.artifact_root)"), "it returns the registration's own value");
    // Nothing in the whole consensus crate derives an inventory root.
    for forbidden in ["a16_inventory_root", "inventory_root_streamed", "a16_inventory_v1"] {
        let hits = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
            .expect("src")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("rs"))
            .filter(|e| std::fs::read_to_string(e.path()).unwrap_or_default().contains(forbidden))
            .count();
        println!("consensus-core src files mentioning `{forbidden}`: {hits}");
    }
}

/// **How many of testnet-12's three registered roots can any CI run re-derive?**
///
/// One. The floor derives from a seed (`misaka-palw-base0` `the_pinned_rc_artifact_root_is_the_one_the_floor_derives`,
/// not `#[ignore]`d). The dense 2M root is a committed JSON measurement whose only re-derivation is
/// `#[ignore]`d and needs a 2.87 GB file absent from the repo. The hybrid root is a hand-pinned byte
/// array with no derivation anywhere in the tree.
#[test]
fn only_one_of_the_three_t12_roots_is_derivable_by_ci() {
    let p = Params::from(t12());
    let roots = registered_roots(&p);
    assert_eq!(roots.len(), 3, "t12 registers three classes");

    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifests = std::fs::read_dir(repo.join("consensus/core/src/config/class-manifests"))
        .map(|d| d.filter_map(|e| e.ok()).count())
        .unwrap_or(0);
    println!("registered classes: {}   committed .palwmanifest files: {manifests}", roots.len());
    assert_eq!(manifests, 1, "one manifest for three registered roots");

    // The floor: derivable, and the test that does it is live.
    let rc = std::fs::read_to_string(repo.join("misaka-palw-base0/src/rc.rs")).expect("rc.rs");
    let at = rc.find("fn the_pinned_rc_artifact_root_is_the_one_the_floor_derives").expect("present");
    let before = &rc[at.saturating_sub(200)..at];
    println!("floor derivation test ignored? {}", before.contains("#[ignore]"));
    assert!(!before.contains("#[ignore]"), "the floor's root is re-derived on every CI run");

    // The dense 2M root: the only re-derivation is ignored and needs an out-of-repo file.
    let probe = std::fs::read_to_string(repo.join("misaka-palw-base0/tests/a16_root_probe.rs")).expect("probe");
    let at = probe.find("fn print_a16_2m_root_forms").expect("present");
    let before = &probe[at.saturating_sub(120)..at];
    assert!(before.contains("#[ignore]"), "the 2M re-derivation is ignored");
    assert!(probe.contains("PALW_A16_2M_PATH"), "and it needs a file no CI run has");
    println!("2M re-derivation: #[ignore], requires env PALW_A16_2M_PATH");

    // The hybrid root: no derivation at all.
    let q36 = PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT;
    println!("hybrid root {q36} — hand-pinned byte array, re-pinned 2026-08-27, no derivation in-tree");

    // And only the dense row carries a digest/root confusion guard.
    let reg = std::fs::read_to_string(repo.join("consensus/core/tests/t12_regenesis.rs")).expect("t12_regenesis");
    assert!(reg.contains("the card would be registering a flat artifact digest again"), "the dense row has the guard");
    println!("assert_ne! digest-vs-root guards in t12_regenesis.rs: {}", reg.matches("artifact_digest_of").count());
}

/// **The split reaches no consensus source file, in either consensus crate.**
#[test]
fn the_typed_identities_reach_neither_consensus_crate() {
    let core_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let node_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (label, root) in [("kaspa-consensus-core/src", core_src), ("kaspa-consensus/src", node_src)] {
        let mut hits = 0usize;
        let mut files = 0usize;
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                    continue;
                }
                files += 1;
                if p.file_name().and_then(|x| x.to_str()) == Some("palw_class_identity_v1.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap_or_default();
                for t in ["PalwArtifactDigestV1", "PalwInventoryRootV1", "PalwClassIdV1"] {
                    hits += src.matches(t).count();
                }
            }
        }
        println!("{label}: {files} .rs files, {hits} typed-identity mentions");
        counts.push((label.to_string(), hits));
    }
    for (label, hits) in &counts {
        assert_eq!(*hits, 0, "{label} carries {hits} typed-identity mentions");
    }
    // For contrast, the type IS used — in the operator SDK, which is not consensus.
    let sdk = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../misaka-palw-sdk/src");
    let mut sdk_hits = 0usize;
    let mut stack = vec![sdk];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                let src = std::fs::read_to_string(&p).unwrap_or_default();
                for t in ["PalwArtifactDigestV1", "PalwInventoryRootV1", "PalwClassIdV1"] {
                    sdk_hits += src.matches(t).count();
                }
            }
        }
    }
    println!("misaka-palw-sdk/src: {sdk_hits} typed-identity mentions (an operator tool, not consensus)");
    assert!(sdk_hits > 0, "the split lives entirely here");

    // And the class-id type is not the type the genesis card's class ids are, either.
    let c = PalwClassIdV1::of_this_graph(manifest::class_id_of_class(manifest::QWEN25_A16_2M_MANIFEST_V1, 1));
    let p = Params::from(t12());
    let roots = registered_roots(&p);
    assert!(roots.iter().any(|(id, _)| *id == c.into_hash64()), "the chain stores it as a bare Hash64");
}

/// **The three types carry no borsh implementation — so they CANNOT sit at a consensus boundary
/// as written.** Every consensus object is borsh-serialised into the state root and the ruleset id.
/// A newtype with no `BorshSerialize` cannot be a field of one, which is a structural reason the
/// split was always going to stop at the SDK rather than an oversight at one call site.
#[test]
fn the_typed_identities_cannot_be_borsh_fields() {
    fn assert_borsh<T: borsh::BorshSerialize + borsh::BorshDeserialize>() {}
    // The object that carries `artifact_root` is borsh on both sides.
    assert_borsh::<PalwConsensusObjectV2>();
    assert_borsh::<Hash64>();

    let whole = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/palw_class_identity_v1.rs"),
    )
    .expect("readable");
    let src = &whole[..whole.find("#[cfg(test)]").expect("has tests")];
    assert!(!src.contains("Borsh"), "the three types derive no borsh codec");
    assert!(!src.contains("serde"), "nor serde");
    println!("derives on the three types: {:?}", src.matches("#[derive(").count());
    for line in src.lines().filter(|l| l.trim_start().starts_with("#[derive(")) {
        println!("  {}", line.trim());
    }
}
