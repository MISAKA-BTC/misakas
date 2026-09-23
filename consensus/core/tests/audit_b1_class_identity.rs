//! **AUDIT B1 — is the 2026-09-23 class-identity type split enforced, or cosmetic?**
//!
//! Read-only audit artefact. Nothing here is a fixture any shipped code reads.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::class_manifest_const_v1 as manifest;
use kaspa_consensus_core::config::params::*;
use kaspa_consensus_core::config::premine::*;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_class_identity_v1::{PalwArtifactDigestV1, PalwInventoryRootV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

fn t12() -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, 12)
}

fn t12_bonds() -> Vec<kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1> {
    PALW_T12_GENESIS_BONDS
        .iter()
        .map(|c| kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(premine_outpoint_for(t12(), c.premine_index)),
            pubkey: c.bond_pubkey.to_vec(),
            operator_pubkey: c.operator_pubkey.to_vec(),
            payout_payload: Hash64::from_bytes(c.payout_payload),
        })
        .collect()
}

/// Assemble the t12 card with `dense_root` in the 2M dense row's `artifact_root` slot.
fn t12_card_with_dense_root(dense_root: Hash64) -> Params {
    let base = palw_t12_base_params();
    let utxos = genesis_premine_utxos_for(base.net);
    let mut rows = PALW_T12_GENESIS_HELD_ROWS;
    for row in rows.iter_mut().filter(|row| row.n_ctx == PALW_T12_DENSE_N_CTX) {
        row.artifact_root = dense_root;
    }
    palw_v2_params_with_class_rows_v1(base, PALW_RC_GENESIS_ARTIFACT_ROOT, t12_bonds(), utxos, PalwGenesisClassRowsV1::Held(&rows))
        .expect("the card assembles")
}

fn dense_registered_root(p: &Params) -> Hash64 {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            n_ctx: PALW_T12_DENSE_N_CTX,
            ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
        },
    )
    .expect("2M graph-v7 projects");
    let want = profile.shape_profile_id();
    bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } if *class_id == want => Some(*artifact_root),
            _ => None,
        })
        .expect("the dense row is registered")
}

/// **THE FINDING.** The substitution the module says is "unrepresentable" is one `into_hash64()`
/// away at the exact boundary that shipped it twice: the genesis card's `artifact_root` slot.
#[test]
fn the_forbidden_substitution_still_assembles_a_genesis_card() {
    // The digest, held in the type that is supposed to make this impossible.
    let digest: PalwArtifactDigestV1 =
        PalwArtifactDigestV1::measured_over_the_file(manifest::artifact_digest_of(manifest::QWEN25_A16_2M_MANIFEST_V1));
    let root: PalwInventoryRootV1 =
        PalwInventoryRootV1::rooted_over_the_inventory(manifest::inventory_root_of_class(manifest::QWEN25_A16_2M_MANIFEST_V1, 1));
    assert_ne!(digest.into_hash64(), root.into_hash64(), "the two values in the committed manifest are different");

    // The honest card.
    let honest = t12_card_with_dense_root(root.into_hash64());
    assert_eq!(dense_registered_root(&honest), PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT);

    // **The t11/t12 outage, reproduced with the post-split API.** One method call laundered the
    // digest into the root slot; nothing refused it.
    let wedged = t12_card_with_dense_root(digest.into_hash64());
    assert_eq!(
        dense_registered_root(&wedged),
        digest.into_hash64(),
        "the genesis card registered the FILE DIGEST as the class's inventory root"
    );
    // And the resulting network validates and is a distinct, shippable chain.
    wedged.validate_palw_v2().expect("the wedged ruleset validates — nothing objects");
    assert_ne!(
        wedged.consensus_params_id(),
        honest.consensus_params_id(),
        "a different chain — the one whose dense tier produces zero blocks"
    );
    println!("digest (wedged root) = {}", digest);
    println!("true inventory root  = {}", root);
    println!("honest params id     = {}", honest.consensus_params_id());
    println!("wedged params id     = {}", wedged.consensus_params_id());
}

/// The genesis gate HAS an `ArtifactRootMismatch` error. It cannot fire on the card path, because
/// the catalog entry and the `ClassRegistered` object are minted from the same argument.
#[test]
fn the_genesis_gates_artifact_root_check_compares_a_value_with_itself() {
    let arbitrary = Hash64::from_u64_word(0xDEAD_BEEF);
    let card = t12_card_with_dense_root(arbitrary);
    let PalwConsensusMode::ConsensusV2(bundle) = &card.palw_consensus_mode else { panic!("ConsensusV2") };
    assert_eq!(dense_registered_root(&card), arbitrary, "any 64 bytes at all become the registered root");
    // The catalog commitment moved with it, which is why the gate's comparison is vacuous.
    let honest_catalog = {
        let h = t12_card_with_dense_root(PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT);
        let PalwConsensusMode::ConsensusV2(b) = &h.palw_consensus_mode else { panic!("ConsensusV2") };
        b.class_catalog_root
    };
    assert_ne!(bundle.class_catalog_root, honest_catalog, "the catalog is a function of the same argument the object is");
    card.validate_palw_v2().expect("a card registering 0xdeadbeef as an inventory root validates");
    println!("catalog root with 0xdeadbeef = {}", bundle.class_catalog_root);
}

/// **The split does not reach consensus.** Every boundary that carries one of the three quantities
/// still carries `Hash64`.
#[test]
fn no_consensus_boundary_names_the_typed_identities() {
    // Read the crate's own sources directly.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut typed_hits: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).expect("readable") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            if p.file_name().and_then(|x| x.to_str()) == Some("palw_class_identity_v1.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap_or_default();
            for t in ["PalwArtifactDigestV1", "PalwInventoryRootV1", "PalwClassIdV1"] {
                if src.contains(t) {
                    typed_hits.push(format!("{} :: {t}", p.display()));
                }
            }
        }
    }
    println!("typed-identity uses in kaspa-consensus-core/src (excluding the module itself): {}", typed_hits.len());
    for h in &typed_hits {
        println!("  {h}");
    }
    assert!(
        typed_hits.is_empty(),
        "the three types are used nowhere in the consensus crate's own sources — the split is confined to the SDK"
    );
    // And the genesis-card entrypoint takes three interchangeable `Hash64` positions.
    let src = std::fs::read_to_string(root.join("config/params.rs")).expect("params.rs");
    for decl in ["base0_artifact_root: crate::Hash64,", "qwen36_artifact_root: crate::Hash64,", "qwen25_a16_artifact_root: Option<crate::Hash64>,"] {
        assert!(src.contains(decl), "the card still declares `{decl}`");
    }
}

/// **The hybrid row, same question.** `misaka_palw_base0::inventory::qwen36_registers_inventory_root_v1`
/// decides which of the two Qwen3.6 root FORMS a registration must pin, by one predicate over the
/// profile. That crate cannot be reached from here (it depends on this one), so the predicate is
/// re-evaluated over the profile t12's own genesis carriage carries.
#[test]
fn which_root_form_does_the_t12_hybrid_row_owe() {
    use kaspa_consensus_core::palw_step::kernel_semantics_id_v1;
    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("ConsensusV2") };

    let by_token = kernel_semantics_id_v1(kaspa_consensus_core::palw_step_refute::KDESC_A16_REQUANTIZE_BY_TOKEN);
    // testnet-11's hybrid row, for contrast: the SAME constant, under the graph-v3 row.
    let t11 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    if let PalwConsensusMode::ConsensusV2(b11) = &t11.palw_consensus_mode {
        for o in &b11.genesis_objects {
            let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, admission, .. } = o else { continue };
            if *artifact_root != PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT {
                continue;
            }
            let owes_inventory =
                admission.as_ref().map(|c| c.profile.pre_nodes.iter().any(|n| n.kernel_semantics_id == by_token));
            println!("t11 hybrid class {class_id}\n  same root, owes_inventory_root = {owes_inventory:?}");
        }
    }
    for o in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, admission, .. } = o else { continue };
        let Some(c) = admission.as_ref() else {
            println!("class {class_id} root {artifact_root}  (no carriage — the floor)");
            continue;
        };
        // The exact expression from `qwen36_registers_inventory_root_v1`.
        let registers_inventory_root = c.profile.pre_nodes.iter().any(|n| n.kernel_semantics_id == by_token);
        println!(
            "class {}\n  root  {}\n  n_ctx {}  layers {}  gdn_heads {}\n  qwen36_registers_inventory_root_v1 = {}",
            class_id,
            artifact_root,
            c.profile.n_ctx,
            c.profile.layer_count,
            c.profile.gdn_heads,
            registers_inventory_root
        );
        if *artifact_root == PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT {
            println!("  ^ this row pins PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT, documented as the COMPUTED root");
        }
    }
}

/// What one claim of each t12 class puts at risk, in the unit the fold reserves in
/// (`palw_exposure_pwu_v3` = `palw_exposure_unit_pwu_v1`), so a court defect has a price.
#[test]
fn what_one_claim_of_each_t12_class_reserves() {
    use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
    use kaspa_consensus_core::palw_fp_devnet_v3::palw_exposure_unit_pwu_v1;
    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("ConsensusV2") };

    // The floor's basis: declared leaves and derived work per draw.
    let floor_profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .expect("the floor projects");
    let (fp, fd) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let floor_job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor_profile, fp, fd);
    let floor_desc = PalwCanonicalClassDescriptorV1::of(&floor_profile, Hash64::default()).expect("descriptor");
    let floor_draw = palw_canonical_draw_work_v1(&floor_desc, &floor_job, true).expect("floor draw").provisional_scalar_v1();
    let mut floor_declared = 0u64;

    for o in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, .. } = o {
            if *class_id == bundle.base_class_id {
                if let kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } = pwu_rule {
                    floor_declared = *pwu_per_inference;
                }
            }
        }
    }
    println!("floor: declared_leaves {floor_declared}  derived_per_draw {floor_draw} MAC-eq");

    for o in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, slash_value_per_pwu, admission, .. } = o else { continue };
        let declared = match pwu_rule {
            kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
            _ => continue,
        };
        let draw = match admission.as_ref() {
            Some(c) => {
                let d = PalwCanonicalClassDescriptorV1::of(&c.profile, Hash64::default()).expect("descriptor");
                palw_canonical_draw_work_v1(&d, &c.canonical, true).expect("draw").provisional_scalar_v1()
            }
            None => floor_draw,
        };
        let u3 = palw_exposure_unit_pwu_v1(draw, floor_declared, floor_draw);
        let reserved = u3 as u128 * *slash_value_per_pwu as u128;
        println!(
            "class {}\n  declared(U1) {declared} leaves | derived(U2) {draw} MAC-eq | exposure(U3) {u3} pwu\n  reserved = U3 x {} sompi/pwu = {reserved} sompi = {:.8} MSK",
            &class_id.to_string()[..16],
            slash_value_per_pwu,
            reserved as f64 / 1e8
        );
    }
    println!("escrow per claim (720 permille of 444_562_014_000) = {} sompi = {:.5} MSK", 444_562_014_000u64 * 720 / 1000, (444_562_014_000u64 * 720 / 1000) as f64 / 1e8);
}
