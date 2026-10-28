//! RFC-0019 — the annual model catalog over the pure rules, the identity / fork id and the budget engine's tiered roll.
//! **Unverified source: written, not built and not run — no result here is a PASS.** Every value is a TEST value.
//!
//! | test | RFC-0019 requirement (§11) |
//! |---|---|
//! | `the_root_is_the_canonical_encoding_and_matches_the_release_tool` | A-1 canonical encoding; the release tool's vector |
//! | `catalogs_chain_by_previous_root_and_only_add` | A-2 chain + add-only (ADR-0175), kernel version, bounds |
//! | `a_release_extends_the_previous_release_as_a_prefix` | A-3 never overwrite / remove / reorder across releases |
//! | `the_active_catalog_is_the_highest_activated_edition` | A-4 `annual_catalog_active_at` |
//! | `the_tier_is_catalog_membership_and_never_registry_state` | A-5 Active ≠ 1.0; `model_tier_permille` |
//! | `old_and_new_builds_share_the_identity_until_the_edition_activates` | B-1 identity normalisation; schedule id reports |
//! | `the_fork_id_warns_before_an_edition_and_refuses_a_build_without_it_after` | B-2 fork id (warn pre, refuse post) |
//! | `a_red_block_merged_across_the_activation_is_read_against_one_epoch_snapshot` | C-1 accepted-DAA boundary rule |
//! | `splitting_capital_or_registering_more_models_never_raises_rights_and_0_3_applies_once` | C-2 no rights from splitting / no 0.09 |
//! | `the_catalog_status_is_a_pure_function_of_params_and_tip_across_restart_and_reorg` | D-1 restart / IBD / reorg consistency |
//! | `a_catalog_naming_absent_models_never_gates_blocks_or_liveness` | D-2 BASE-0 / heartbeat liveness when catalog models are missing |
//! | `the_lifecycle_names_are_operational_and_only_a_released_edition_ships` | §4 lifecycle; a node derives only SCHEDULED / ACTIVE |
//!
//! Gate C for every catalog edition and tier: `palw_rfc0018_v1/tests.rs`
//! `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt` (an edition-0 and an edition-1 listing, RFC-0019 §6.5).

use std::collections::BTreeMap;

use super::*;
use crate::config::params::TESTNET_PARAMS;
use crate::fork_id_v1::{ForkIdVerdict, evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use crate::palw_bond_budget_v1::*;
use crate::palw_model_tier_v1::{
    PALW_MODEL_TIER_APPROVED_PERMILLE_V1, PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1,
    PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1, PalwModelTierFenceV1, PalwModelTierV1, PalwTierSourceV1,
    palw_model_tier_coefficient_permille_v1, palw_model_tier_of_v1, palw_model_tier_weight_v1,
};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::tx::TransactionOutpoint;

/// The cross-implementation vectors: `python3 -I` over `scripts/release/misaka-release-manifest.py`'s `catalog_root` (the same bytes,
/// the same keyed blake2b-512) printed these for the two catalogs built below.
const R0: &str =
    "3da5390791b755610bd820466a28ae8f32cc9f6633660201b6b2bc7b73cf63dd9f675623db33c6b6c1a349afd2f82c48d9e99d7d0403be538ebb016052517db6";
const R1: &str =
    "6cb437ad38163cdbc973c26aff67784ba1bb8627c9c76004a3c16da40cb3d9c75377a887a810d344fbc3b7a1ef1793fb1573ff7bdeca1ebef9397f1417c1586d";
const EMPTY: &str =
    "c94db3542b1e0be58ad1da59b39fabf37b650f6c3e22348e06cb2bd6b1216143be3519a80bbd68578d62daf2d203b7a3db96e6d10c95a324cee8ef9d47f72b6a";

const H_EDITION: u64 = 5_000;
const KAS: u64 = 100_000_000;

fn b(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn m(class: u8, root: u8) -> ApprovedModelV1 {
    ApprovedModelV1::of(b(class), b(root))
}

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(crate::Hash64::from_bytes([n; 64]), 0))
}

/// A base network with the tier fence at `fence_at` carrying `chain` (identity, schedule and fork-id tests read Params only; the
/// fence's own validation — which refuses any armed height — is not what they test).
fn armed(chain: PalwAnnualCatalogChainV1, fence_at: u64) -> Params {
    let mut p = TESTNET_PARAMS;
    let mut fence = PalwModelTierFenceV1::at(ForkActivation::new(fence_at));
    fence.catalogs = chain;
    p.palw_model_tier_v1 = Some(fence);
    p
}

fn old_chain() -> PalwAnnualCatalogChainV1 {
    PalwAnnualCatalogChainV1::genesis_only(vec![m(1, 2)])
}

fn new_chain() -> PalwAnnualCatalogChainV1 {
    old_chain().with_edition(H_EDITION, vec![m(4, 5)])
}

#[test]
fn the_root_is_the_canonical_encoding_and_matches_the_release_tool() {
    let e0 = AnnualModelCatalogV1::genesis(vec![ApprovedModelV1 {
        class_id: b(1),
        artifact_root: b(2),
        kernel_version: 1,
        verification_plan_id: b(3),
    }]);
    assert_eq!(catalog_bytes_v1(&e0).len(), 2 + 4 + 8 + 64 + 4 + (64 + 64 + 2 + 64), "the documented layout");
    assert_eq!(&catalog_bytes_v1(&e0)[..2], &1u16.to_le_bytes(), "the encoding version leads");
    assert_eq!(e0.root().to_string(), R0, "the release tool's vector (edition 0)");
    let chain = PalwAnnualCatalogChainV1 { editions: vec![e0.clone()] }.with_edition(1_000, vec![m(4, 5)]);
    chain.validate().unwrap();
    assert_eq!(chain.editions[1].previous_catalog_root, e0.root(), "edition 1 chains to edition 0's root");
    assert_eq!(chain.editions[1].root().to_string(), R1, "the release tool's vector (edition 1)");
    assert_eq!(AnnualModelCatalogV1::genesis(Vec::new()).root().to_string(), EMPTY, "the empty genesis catalog");
    // One model set, one encoding: the constructors sort, and the root does not depend on the order models were handed in.
    let ab = AnnualModelCatalogV1::genesis(vec![m(7, 8), m(3, 4)]);
    let ba = AnnualModelCatalogV1::genesis(vec![m(3, 4), m(7, 8)]);
    assert_eq!(ab.root(), ba.root());
    // Every field is bound: changing any one moves the root.
    let base = AnnualModelCatalogV1::genesis(vec![m(3, 4)]);
    let mut moved = base.clone();
    moved.models[0].kernel_version = 1;
    assert_ne!(moved.root(), base.root(), "the kernel version is bound");
    let mut moved = base.clone();
    moved.models[0].verification_plan_id = b(9);
    assert_ne!(moved.root(), base.root(), "the plan is bound");
    let mut moved = base.clone();
    moved.activation_daa = 1;
    assert_ne!(moved.root(), base.root(), "the height is bound");
}

#[test]
fn catalogs_chain_by_previous_root_and_only_add() {
    let good = new_chain();
    good.validate().unwrap();
    // Not starting at a genesis edition 0 at DAA 0 with a zero previous root.
    assert!(PalwAnnualCatalogChainV1::default().validate().is_err(), "no genesis catalog");
    let mut g = good.clone();
    g.editions[0].activation_daa = 1;
    assert!(g.validate().is_err(), "the genesis catalog is at DAA 0");
    let mut g = good.clone();
    g.editions[0].previous_catalog_root = b(9);
    assert!(g.validate().is_err(), "the genesis catalog has no previous root");
    // A broken chain: wrong previous root, wrong number, gap.
    let mut g = good.clone();
    g.editions[1].previous_catalog_root = b(9);
    assert!(g.validate().is_err(), "edition 1 must chain to root(edition 0)");
    let mut g = good.clone();
    g.editions[1].edition = 2;
    assert!(g.validate().is_err(), "editions are numbered without gaps");
    // Heights: strictly increasing, real.
    let same = old_chain().with_edition(H_EDITION, vec![m(4, 5)]).with_edition(H_EDITION, vec![m(6, 7)]);
    assert!(same.validate().is_err(), "two editions at one height");
    let never = old_chain().with_edition(u64::MAX, vec![m(4, 5)]);
    assert!(never.validate().is_err(), "u64::MAX is not a height");
    // ADD-ONLY (ADR-0175): a class id once over the whole chain — re-listing it with its own root, re-rooting it, listing it twice in
    // one edition: all refused. Removal is impossible by construction (an edition only adds), and a rewritten earlier edition breaks
    // the chain of roots.
    for again in [m(1, 2), m(1, 9), m(4, 9)] {
        let g = good.clone().with_edition(H_EDITION + 1, vec![again]);
        assert!(g.validate().is_err(), "{again:?} re-listed");
    }
    let mut twice = old_chain().with_edition(H_EDITION, vec![m(4, 5), m(6, 7)]);
    twice.editions[1].models[1].class_id = b(4);
    assert!(twice.validate().is_err(), "a class twice in one edition (and out of order)");
    let mut rewritten = good.clone();
    rewritten.editions[0].models[0].artifact_root = b(9);
    assert!(rewritten.validate().is_err(), "re-rooting edition 0 breaks edition 1's previous root");
    // Shape: empty annual edition, zero ids, unsorted, unknown kernel version, bounds.
    assert!(old_chain().with_edition(H_EDITION, Vec::new()).validate().is_err(), "an empty annual edition");
    assert!(old_chain().with_edition(H_EDITION, vec![ApprovedModelV1::of(Hash64::default(), b(5))]).validate().is_err());
    assert!(old_chain().with_edition(H_EDITION, vec![ApprovedModelV1::of(b(5), Hash64::default())]).validate().is_err());
    let mut unsorted = old_chain().with_edition(H_EDITION, vec![m(4, 5), m(6, 7)]);
    unsorted.editions[1].models.swap(0, 1);
    assert!(unsorted.validate().is_err(), "models strictly ascending");
    let mut kernel = m(4, 5);
    kernel.kernel_version = PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1 + 1;
    assert!(old_chain().with_edition(H_EDITION, vec![kernel]).validate().is_err(), "a kernel this build does not implement");
    let many: Vec<ApprovedModelV1> = (0..=PALW_ANNUAL_CATALOG_MAX_MODELS_PER_EDITION_V1 as u64)
        .map(|i| ApprovedModelV1::of(Hash64::from_u64_word(1_000_000 + i), b(5)))
        .collect();
    assert!(old_chain().with_edition(H_EDITION, many).validate().is_err(), "the per-edition bound");
}

#[test]
fn a_release_extends_the_previous_release_as_a_prefix() {
    let old = old_chain();
    let new = new_chain();
    new.extends(&old).unwrap();
    new.extends(&new).unwrap();
    assert!(old.extends(&new).is_err(), "a release that drops an edition");
    let mut rewritten = new.clone();
    rewritten.editions[0].models.push(m(9, 9));
    assert!(rewritten.extends(&old).is_err(), "a release that rewrites the genesis catalog");
    let other = PalwAnnualCatalogChainV1::genesis_only(vec![m(1, 3)]).with_edition(H_EDITION, vec![m(4, 5)]);
    assert!(other.extends(&old).is_err(), "a release that re-roots an existing model");
}

#[test]
fn the_active_catalog_is_the_highest_activated_edition() {
    let chain = new_chain().with_edition(2 * H_EDITION, vec![m(6, 7)]);
    chain.validate().unwrap();
    for (daa, edition) in [(0, 0), (H_EDITION - 1, 0), (H_EDITION, 1), (2 * H_EDITION - 1, 1), (2 * H_EDITION, 2), (u64::MAX, 2)] {
        assert_eq!(annual_catalog_active_at(&chain, daa).unwrap().edition, edition, "at {daa}");
        assert_eq!(chain.root_at(daa), chain.editions[edition as usize].root(), "the root in force at {daa}");
    }
    assert_eq!(chain.next_after(0).map(|c| c.activation_daa), Some(H_EDITION));
    assert_eq!(chain.next_after(H_EDITION).map(|c| c.activation_daa), Some(2 * H_EDITION));
    assert_eq!(chain.next_after(2 * H_EDITION), None);
    assert_eq!(chain.scheduled_activations(), vec![H_EDITION, 2 * H_EDITION], "edition 0 is not a fence");
    // ApprovedModels(h) is the union of editions 0..=active(h).
    assert!(chain.listed_at(&b(1), &b(2), 0).is_some());
    assert!(chain.listed_at(&b(4), &b(5), H_EDITION - 1).is_none());
    assert_eq!(chain.listed_at(&b(4), &b(5), H_EDITION).map(|c| c.edition), Some(1));
    assert_eq!(chain.listed_at(&b(6), &b(7), u64::MAX).map(|c| c.edition), Some(2));
    assert!(chain.listed_at(&b(4), &b(9), u64::MAX).is_none(), "the root must match");
}

/// PRINCIPLES §9.2: registry state (`Active` included) never makes a model 1.0 — the tier API takes no lifecycle argument; only
/// membership of the catalog active at the height does, and a catalog model is 1.0 whatever its lifecycle.
#[test]
fn the_tier_is_catalog_membership_and_never_registry_state() {
    let fence_at = 1_000;
    let mut fence = PalwModelTierFenceV1::at(ForkActivation::new(fence_at));
    fence.catalogs = new_chain();
    fence.validate().unwrap();
    let mirror = fence.mirror();
    let registered = PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1;
    // A registered class that the registry walked to `Active` (any number of years): still the registered tier.
    for daa in [fence_at, H_EDITION, H_EDITION + 365 * 720, u64::MAX] {
        assert_eq!(model_tier_permille(Some(&mirror), &b(8), &b(9), false, daa), registered, "unlisted at {daa}");
    }
    // The genesis catalog's model: 1.0 from the fence; the edition-1 model: 0.3 until H, 1.0 from H.
    assert_eq!(model_tier_permille(Some(&mirror), &b(1), &b(2), false, fence_at), PALW_MODEL_TIER_APPROVED_PERMILLE_V1);
    assert_eq!(model_tier_permille(Some(&mirror), &b(4), &b(5), false, H_EDITION - 1), registered);
    assert_eq!(model_tier_permille(Some(&mirror), &b(4), &b(5), false, H_EDITION), PALW_MODEL_TIER_APPROVED_PERMILLE_V1);
    assert_eq!(
        palw_model_tier_of_v1(Some(&mirror), &b(4), &b(5), false, H_EDITION),
        PalwModelTierV1::Approved(PalwTierSourceV1::AnnualFork { edition: 1, from_daa: H_EDITION })
    );
    // A-2: below the tier fence every model weighs 1.0, listed or not; and without the fence at all.
    assert_eq!(model_tier_permille(Some(&mirror), &b(8), &b(9), false, fence_at - 1), PALW_MODEL_TIER_APPROVED_PERMILLE_V1);
    assert_eq!(model_tier_permille(None, &b(8), &b(9), false, u64::MAX), PALW_MODEL_TIER_APPROVED_PERMILLE_V1);
    // A same-id class with another root (impossible on chain by ADR-0175, refused here anyway) is not the catalog model.
    assert_eq!(model_tier_permille(Some(&mirror), &b(4), &b(6), false, H_EDITION), registered);
}

/// RFC-0019 §7.2: a build carrying the NEXT edition and one that does not share `consensus_identity_id` until the edition activates
/// (the rolling upgrade); the raw params id and the schedule id see the difference (reported, never gated). A genesis catalog that
/// differs is a rule in force at genesis and separates identities. A tier fence scheduled in the future is invisible to the identity.
#[test]
fn old_and_new_builds_share_the_identity_until_the_edition_activates() {
    let old = armed(old_chain(), 0);
    let new = armed(new_chain(), 0);
    assert_eq!(
        old.consensus_identity_id(),
        new.consensus_identity_id(),
        "the next edition leaves the identity: peers through the rollout"
    );
    assert_ne!(old.consensus_params_id(), new.consensus_params_id(), "the raw id sees the edition");
    assert_ne!(old.consensus_schedule_id(), new.consensus_schedule_id(), "the schedule id reports it (a warning, not a gate)");
    // Two new builds that disagree about the next edition's CONTENT agree on identity (both are 'not yet') and differ in schedule.
    let rival = armed(old_chain().with_edition(H_EDITION, vec![m(6, 7)]), 0);
    assert_eq!(rival.consensus_identity_id(), new.consensus_identity_id());
    assert_ne!(rival.consensus_schedule_id(), new.consensus_schedule_id(), "a rival catalog root is reported before activation");
    // The genesis catalog is a rule at genesis: a different one is a different network.
    let other_genesis = armed(PalwAnnualCatalogChainV1::genesis_only(vec![m(1, 3)]), 0);
    assert_ne!(other_genesis.consensus_identity_id(), old.consensus_identity_id());
    // A tier fence scheduled at a future height leaves the identity whole (A-2): same as no fence at all.
    let scheduled = armed(new_chain(), 1_000);
    assert_eq!(scheduled.consensus_identity_id(), TESTNET_PARAMS.consensus_identity_id());
    // Shipped presets carry nothing (the live testnet-12 identity cannot move from this module).
    assert!(TESTNET_PARAMS.palw_model_tier_v1.is_none());
    assert!(TESTNET_PARAMS.palw_annual_catalog_activations_v1().is_empty());
}

/// RFC-0019 §7.3: the edition's height is on the fence schedule AND gates the fork id. A node still on the old binary is a warning
/// below the height (the two agree about every block) and a named refusal from it; a fresh node of the new build that is still
/// syncing pre-H history is kept by an upgraded node past H.
#[test]
fn the_fork_id_warns_before_an_edition_and_refuses_a_build_without_it_after() {
    let old = armed(old_chain(), 0);
    let new = armed(new_chain(), 0);
    assert!(new.fence_schedule_v1().contains(&H_EDITION), "the edition is a scheduled height");
    assert!(fork_id_gate_fences_v1(&new).contains(&H_EDITION), "and a gate fence");
    assert!(!fork_id_gate_fences_v1(&old).contains(&H_EDITION));
    for local in [0, 1, H_EDITION - 1] {
        let peer = fork_id_v1(&old, local);
        let verdict = evaluate_fork_id_v1(&new, local, &peer.fired.as_bytes(), peer.next);
        assert!(!verdict.refuses(), "below H a build without the edition is a warning, not a refusal: {verdict:?}");
    }
    for local in [H_EDITION, H_EDITION + 1, H_EDITION + 10_000] {
        let peer = fork_id_v1(&old, local);
        let verdict = evaluate_fork_id_v1(&new, local, &peer.fired.as_bytes(), peer.next);
        assert!(verdict.refuses(), "from H a build without the edition is refused by name: {verdict:?}");
    }
    // A fresh upgraded node syncing below H, judged by an upgraded node past H: the same build seen from earlier — kept.
    let fresh = fork_id_v1(&new, 10);
    assert_eq!(evaluate_fork_id_v1(&new, H_EDITION + 5, &fresh.fired.as_bytes(), fresh.next), ForkIdVerdict::Agree);
    let peer = fork_id_v1(&new, H_EDITION + 5);
    assert_eq!(evaluate_fork_id_v1(&new, H_EDITION + 5, &peer.fired.as_bytes(), peer.next), ForkIdVerdict::Agree);
}

/// RFC-0019 §6.4: a RED block mined below the activation and merged above it is accepted ONCE, at its accepting chain block's DAA,
/// against the single allocation snapshot of that block's epoch — the snapshot the roll took at the epoch's first chain block.
#[test]
fn a_red_block_merged_across_the_activation_is_read_against_one_epoch_snapshot() {
    let fence_at = 0;
    let mut fence = PalwModelTierFenceV1::at(ForkActivation::new(fence_at));
    // Epochs of 100 DAA: [200, 300) contains H = 250.
    let h = 250;
    fence.catalogs = old_chain().with_edition(h, vec![m(4, 5)]);
    fence.validate().unwrap();
    let mirror = fence.mirror();
    let tier_at = |daa| palw_model_tier_of_v1(Some(&mirror), &b(4), &b(5), false, daa);
    // The RED block's own header DAA (240) is never the reference; its accepting chain block's DAA is.
    let red_header_daa = 240;
    for (accepting, roll, approved) in [(260, 200, false), (299, 200, false), (300, 300, true), (305, 300, true)] {
        assert!(accepting > red_header_daa);
        let reference = palw_catalog_tier_reference_daa_v1(accepting, Some(roll));
        assert_eq!(reference, roll, "the epoch's one snapshot");
        assert_eq!(tier_at(reference).is_approved(), approved, "accepted at {accepting} (epoch roll {roll})");
    }
    // A roll that itself lands past H (a chain block jumped over both the epoch boundary and H) reads the new edition: still one read.
    assert!(tier_at(palw_catalog_tier_reference_daa_v1(270, Some(260))).is_approved());
    // No allocation epoch: the accepted DAA itself (no tier is applied anywhere then).
    assert_eq!(palw_catalog_tier_reference_daa_v1(260, None), 260);
    // A roll reported above the claim's accepted DAA cannot be its epoch's: the accepted DAA stands (never a later snapshot).
    assert_eq!(palw_catalog_tier_reference_daa_v1(260, Some(300)), 260);
}

fn allocation_world(bonds: &[(u8, Hash64, u64)]) -> (PalwModelAllocationPolicyV1, PalwBondBudgetStateV1) {
    let ap = PalwModelAllocationPolicyV1::decided_unapproved_v3();
    let mut s = PalwBondBudgetStateV1::new(0, &PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1());
    let mut j = Vec::new();
    for (n, model, amount) in bonds {
        s.assign(&ap, bond(*n), &[(*model, *amount)], 1, *amount, 0, |_| true, &mut j).unwrap();
    }
    (ap, s)
}

/// Rolls epochs `0..epochs` reading the tier through the catalog at each roll's DAA (the fold's reading), with the cap κ.
fn roll_through_catalog(
    ap: &PalwModelAllocationPolicyV1,
    s: &mut PalwBondBudgetStateV1,
    mirror: &crate::palw_model_tier_v1::PalwModelTierMirrorV1,
    roots: &BTreeMap<Hash64, Hash64>,
    epochs: std::ops::Range<u64>,
) {
    let empty = BTreeMap::new();
    let mut j = Vec::new();
    for epoch in epochs {
        let roll_daa = epoch * ap.epoch_daa;
        s.roll_epoch_v3_tiered(
            ap,
            epoch,
            roll_daa,
            |_| (2_000 * KAS, true),
            &empty,
            |_, c| c,
            |model: &Hash64| {
                let root = roots.get(model).copied().unwrap_or_default();
                let tier = palw_model_tier_of_v1(Some(mirror), model, &root, false, roll_daa);
                (!tier.is_approved(), palw_model_tier_coefficient_permille_v1(Some(mirror), tier, roll_daa))
            },
            Some(mirror.registered_cap_permille),
            &mut j,
        );
    }
}

/// PRINCIPLES §9.4: the tier is applied ONCE, in `A_m`; a catalog listing changes that one factor (300 → 1000) and nothing else;
/// splitting a registered model's capital over more models (or more bonds) never raises the tier's weight, and κ bounds the tier.
#[test]
fn splitting_capital_or_registering_more_models_never_raises_rights_and_0_3_applies_once() {
    let approved = Hash64::from_u64_word(0xA0);
    let single = Hash64::from_u64_word(0xB0);
    let halves = [Hash64::from_u64_word(0xB1), Hash64::from_u64_word(0xB2)];
    let roots: BTreeMap<Hash64, Hash64> = [(approved, b(0xA0)), (single, b(0xB0)), (halves[0], b(0xB1)), (halves[1], b(0xB2))].into();
    let mut fence = PalwModelTierFenceV1::at(ForkActivation::new(0));
    fence.catalogs = PalwAnnualCatalogChainV1::genesis_only(vec![ApprovedModelV1::of(approved, b(0xA0))]);
    fence.validate().unwrap();
    let mirror = fence.mirror();
    // One registered model with 1,000 BILI vs the same 1,000 BILI split over two registered models on two bonds.
    let (ap, mut one) = allocation_world(&[(0, approved, 1_000 * KAS), (1, single, 1_000 * KAS)]);
    let (_, mut two) = allocation_world(&[(0, approved, 1_000 * KAS), (1, halves[0], 500 * KAS), (2, halves[1], 500 * KAS)]);
    roll_through_catalog(&ap, &mut one, &mirror, &roots, 0..3);
    roll_through_catalog(&ap, &mut two, &mirror, &roots, 0..3);
    let w_single = one.model_row(&single).unwrap().weight;
    let w_split = two.model_row(&halves[0]).unwrap().weight.saturating_add(two.model_row(&halves[1]).unwrap().weight);
    assert!(w_split <= w_single, "α = 1.5: splitting never raises the tier's weight");
    // Once, never 0.09: the registered weight is exactly c · the approved weight of equal capital.
    let w_approved = one.model_row(&approved).unwrap().weight;
    assert_eq!(w_single, palw_model_tier_weight_v1(w_approved, PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1));
    assert_ne!(
        w_single,
        palw_model_tier_weight_v1(palw_model_tier_weight_v1(w_approved, PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1), 300)
    );
    // The registered tier's total share stays within κ however many models it splits into.
    let mut j = Vec::new();
    let total = 1_000_000 * KAS;
    two.accrue(total, &mut j);
    let tier_total: u128 = halves.iter().map(|h| two.model_available(h) as u128).sum();
    assert!(tier_total * 1_000 / total as u128 <= PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1 as u128);
    // Listing the single model in an annual edition changes that one factor at the first roll at or past its height — and only it.
    let h = 3 * ap.epoch_daa;
    let mut listed_fence = fence.clone();
    listed_fence.catalogs = listed_fence.catalogs.clone().with_edition(h, vec![ApprovedModelV1::of(single, b(0xB0))]);
    listed_fence.validate().unwrap();
    let listed_mirror = listed_fence.mirror();
    // The same epoch rolled twice from one parent book — once without the listing, once with it — so the 30-day averages agree.
    let mut unlisted = one.clone();
    roll_through_catalog(&ap, &mut unlisted, &mirror, &roots, 3..4);
    roll_through_catalog(&ap, &mut one, &listed_mirror, &roots, 3..4);
    let after = one.model_row(&single).unwrap().weight;
    let without = unlisted.model_row(&single).unwrap().weight;
    assert_eq!(after, one.model_row(&approved).unwrap().weight, "listed: the same weight as an equally capitalised approved model");
    assert_eq!(without, palw_model_tier_weight_v1(after, PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1), "one factor, applied once");
    assert_eq!(unlisted.model_row(&approved), one.model_row(&approved), "the listing moves no other model's row");
}

/// Restart / IBD / reorg: the catalog lives in Params (rebuilt identically at every start), the mirror is a clone of it, and every
/// read is a pure function of the tip DAA — so a reorg back below H reads SCHEDULED again and a fresh IBD node reads what the others do.
#[test]
fn the_catalog_status_is_a_pure_function_of_params_and_tip_across_restart_and_reorg() {
    let p = armed(new_chain(), 0);
    let restarted = p.clone();
    assert_eq!(p.palw_model_tier_v1.as_ref().unwrap().mirror(), restarted.palw_model_tier_v1.as_ref().unwrap().mirror());
    let below = palw_annual_catalog_status_v1(&p, H_EDITION - 1);
    assert_eq!(below.current_edition, 0);
    assert_eq!(below.next_activation_daa, Some(H_EDITION));
    assert_eq!(below.next_root, Some(new_chain().editions[1].root()));
    assert_eq!(below.editions[1].state, PalwCatalogBinaryStateV1::Scheduled);
    let at = palw_annual_catalog_status_v1(&p, H_EDITION);
    assert_eq!(at.current_edition, 1);
    assert_eq!(at.current_root, new_chain().editions[1].root());
    assert_eq!(at.next_activation_daa, None);
    assert_eq!(at.editions[1].state, PalwCatalogBinaryStateV1::Active);
    // A reorg back below H: the same reading as before the switch.
    assert_eq!(palw_annual_catalog_status_v1(&p, H_EDITION - 1), below);
    // A restart or a fresh IBD node: the same reading at every height.
    for daa in [0, H_EDITION - 1, H_EDITION, H_EDITION * 3] {
        assert_eq!(palw_annual_catalog_status_v1(&restarted, daa), palw_annual_catalog_status_v1(&p, daa));
    }
    // No fence: nothing scheduled, nothing reported as a catalog.
    let none = palw_annual_catalog_status_v1(&TESTNET_PARAMS, 123);
    assert!(!none.scheduled && none.editions.is_empty() && none.next_activation_daa.is_none());
}

/// RFC-0019 §6.6: listing is not availability. A catalog may name a model no node runs or the chain does not have; that never
/// refuses a block, never stops BASE-0 / heartbeat (no function here returns a block verdict) and only moves the model's allocation
/// weight — a missing model has no capital, so no row and no share.
#[test]
fn a_catalog_naming_absent_models_never_gates_blocks_or_liveness() {
    let absent = ApprovedModelV1::of(Hash64::from_u64_word(0xDEAD), Hash64::from_u64_word(0xBEEF));
    let mut fence = PalwModelTierFenceV1::at(ForkActivation::new(0));
    fence.catalogs = PalwAnnualCatalogChainV1::genesis_only(vec![absent]).with_edition(H_EDITION, vec![m(4, 5)]);
    fence.validate().expect("validation never asks whether a listed model is registered, published or run");
    let mirror = fence.mirror();
    // A class the chain does not have reads as listed by the pure rule, but the allocation roll never sees it (no capital, no row).
    assert!(palw_model_tier_of_v1(Some(&mirror), &absent.class_id, &absent.artifact_root, false, 0).is_approved());
    let roots: BTreeMap<Hash64, Hash64> = BTreeMap::new();
    let other = Hash64::from_u64_word(0xC0);
    let (ap, mut s) = allocation_world(&[(0, other, 1_000 * KAS)]);
    // Three rolls, as the other allocation tests here: the assignment (DAA 1, epoch 0) is seasoned for a full epoch first.
    roll_through_catalog(&ap, &mut s, &mirror, &roots, 0..3);
    assert!(s.model_row(&absent.class_id).is_none(), "an absent catalog model takes no row and no share");
    assert!(s.model_row(&other).is_some(), "the running registered model keeps its (0.3, capped) allocation");
}

#[test]
fn the_lifecycle_names_are_operational_and_only_a_released_edition_ships() {
    use PalwCatalogLifecycleV1::*;
    for state in [Proposed, Tested, Prepared, LockedIn, Active, Aborted] {
        assert_eq!(PalwCatalogLifecycleV1::parse(state.name()), Some(state));
        assert_eq!(state.may_ship_in_a_binary(), matches!(state, Prepared | LockedIn | Active), "{}", state.name());
    }
    assert_eq!(PalwCatalogLifecycleV1::parse("VOTED"), None, "there is no vote state");
    // What a node derives is only scheduled / active — never the release-process state.
    let chain = new_chain();
    assert_eq!(palw_catalog_binary_state_v1(&chain.editions[1], H_EDITION - 1), PalwCatalogBinaryStateV1::Scheduled);
    assert_eq!(palw_catalog_binary_state_v1(&chain.editions[1], H_EDITION), PalwCatalogBinaryStateV1::Active);
    assert_eq!(
        palw_catalog_binary_state_v1(&chain.editions[0], 0),
        PalwCatalogBinaryStateV1::Active,
        "the genesis catalog is active from genesis"
    );
    assert_eq!(PalwCatalogBinaryStateV1::Scheduled.name(), "SCHEDULED");
}
