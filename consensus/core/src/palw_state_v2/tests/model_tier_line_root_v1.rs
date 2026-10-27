//! audit(compute-weight) W-1 — a model's tier is its own `(id, root)`: a later registration (a model line with other weights) on an
//! approved graph is never 1.0. In `palw_state_v2/tests/` because the state's tables are private to it.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::network::{NetworkId, NetworkType};
use crate::palw_annual_catalog_v1::ApprovedModelV1;
use crate::palw_mode_v2::PalwConsensusMode;
use crate::palw_model_tier_v1::*;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2};
use crate::tx::TransactionOutpoint;

const FENCE: u64 = 10_000;

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(0x7135_0000 + n)
}

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(crate::Hash64::from_bytes([n; 64]), 0))
}

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn fence() -> PalwModelTierFenceV1 {
    PalwModelTierFenceV1::at(ForkActivation::new(FENCE))
}

// ---- audit(compute-weight) W-1: a model's tier is read off ITS OWN (id, root), never inherited from the graph it runs ----------------
//
// PRINCIPLES §9.1–9.2: 1.0 is `(model id, fixed artifact root) ∈ ApprovedModels`; every later registration is 0.3 from registration,
// forever. ADR-0175: a later registration with other weights on the same graph is a model line with its own id and its own root. The
// allocation keys a claim by the line that owns its root (`bond_budget_model_of` → `artifact_line_of_root`), and the tiered roll reads
// `palw_model_tier_of_model_v1(line)`. These two tests pin that a line's tier is its own: before the fix the line inherited the GRAPH's
// tier through `line.class_id` (its class's own root and registrant), so different weights — any weights, an all-zero artifact whose
// trace costs nothing to produce — collected 1.0 and sat outside the registered tier's cap κ.

fn tier_state_params(mirror: PalwModelTierMirrorV1) -> PalwStateParamsV2 {
    match &t12().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.clone().with_model_tier_v1(Some(mirror)),
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

fn tier_class(root: Hash64, registrant: Option<PalwBondKeyV2>) -> crate::palw_state_v2::PalwClassStateV2 {
    crate::palw_state_v2::PalwClassStateV2 {
        artifact_root: root,
        slash_value_per_pwu: 5,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(100),
        status: crate::palw_state_v2::PalwClassStatusV2::Active,
        registered_daa: 0,
        registrant_bond: registrant,
        fused_attention: false,
    }
}

fn tier_version(
    root: Hash64,
    status: crate::palw_model_lines_v1::PalwVersionStatusV1,
) -> crate::palw_model_lines_v1::PalwModelVersionV1 {
    crate::palw_model_lines_v1::PalwModelVersionV1 {
        root,
        parent: None,
        adopted_from: None,
        runtime_hash: None,
        dataset_commitment: None,
        training_config_hash: None,
        notes_hash: None,
        published_daa: 1,
        published_by: None,
        status,
        usage: Default::default(),
    }
}

/// **W-1a.** A genesis-assembly class (approved without a list) and a later registration on its graph — a non-founding line with its
/// own root (other weights). The class stays 1.0; the line is a later registration and is 0.3.
#[test]
fn audit_w1a_a_line_with_other_weights_on_an_approved_graph_is_registered_not_approved() {
    use crate::palw_model_lines_v1::{PalwArtifactOwnerV1, PalwVersionStatusV1, founding_line_v1};
    let (class, genesis_root, other_weights, line) = (h(40), h(41), h(42), h(43));
    let mut state = PalwChainStateV2::genesis();
    state.classes.insert(class, tier_class(genesis_root, None));
    state.model_lines.insert(line, founding_line_v1(class, Some(bond(7)), b"zero-weights".to_vec(), 1));
    state.model_versions.insert((line, 1), tier_version(other_weights, PalwVersionStatusV1::Current));
    state.artifact_owners.insert((class, other_weights), PalwArtifactOwnerV1 { class_id: class, line_id: line, version: 1 });
    let params = tier_state_params(fence().mirror());
    // A claim naming `other_weights` is admitted on the class (a root in force) and keyed to the LINE in the allocation.
    assert!(state.class_roots_in_force(&class, FENCE).contains(&other_weights));
    assert_eq!(state.artifact_line_of_root(&class, &other_weights, true), Some(line));
    assert_eq!(
        palw_model_tier_of_model_v1(&state, &params, &class, FENCE),
        PalwModelTierV1::Approved(PalwTierSourceV1::GenesisAssembly),
        "the approved model itself keeps 1.0"
    );
    assert_eq!(
        palw_model_tier_of_model_v1(&state, &params, &line, FENCE),
        PalwModelTierV1::Registered,
        "a later registration with its own root is 0.3 (§9.2) — never the graph's 1.0"
    );
}

/// **W-1b.** A registered class listed by an annual edition as `(id, R0)`, whose founding line has since made another root `R1`
/// current. A claim naming `R1` is keyed to the founding line, i.e. the class id: the class may keep 1.0 only while every root in force
/// on it is the listed one.
#[test]
fn audit_w1b_an_approved_id_with_an_unlisted_root_in_force_is_not_approved() {
    use crate::palw_model_lines_v1::{PalwArtifactOwnerV1, PalwVersionStatusV1, founding_line_v1};
    let (class, listed, unlisted) = (h(50), h(51), h(52));
    let fork_at = FENCE + 10;
    let mut f = fence();
    f.catalogs = f.catalogs.clone().with_edition(fork_at, vec![ApprovedModelV1::of(class, listed)]);
    f.validate().unwrap();
    let params = tier_state_params(f.mirror());
    let mut state = PalwChainStateV2::genesis();
    state.classes.insert(class, tier_class(listed, Some(bond(8))));
    // Control: only the listed root in force — approved by the edition.
    assert_eq!(
        palw_model_tier_of_model_v1(&state, &params, &class, fork_at),
        PalwModelTierV1::Approved(PalwTierSourceV1::AnnualFork { edition: 1, from_daa: fork_at })
    );
    // The founding line publishes and promotes other weights (`R1`) before the listing; `R0` is superseded.
    let mut founding = founding_line_v1(class, Some(bond(8)), Vec::new(), 0);
    founding.current = 2;
    founding.versions_published = 2;
    state.model_lines.insert(class, founding);
    state.model_versions.insert((class, 1), tier_version(listed, PalwVersionStatusV1::Superseded { until_daa: FENCE }));
    state.model_versions.insert((class, 2), tier_version(unlisted, PalwVersionStatusV1::Current));
    state.artifact_owners.insert((class, listed), PalwArtifactOwnerV1 { class_id: class, line_id: class, version: 1 });
    state.artifact_owners.insert((class, unlisted), PalwArtifactOwnerV1 { class_id: class, line_id: class, version: 2 });
    assert_eq!(state.class_roots_in_force(&class, fork_at), vec![unlisted], "claims now name R1 only");
    assert_eq!(state.artifact_line_of_root(&class, &unlisted, true), Some(class), "keyed to the class id");
    assert_eq!(
        palw_model_tier_of_model_v1(&state, &params, &class, fork_at),
        PalwModelTierV1::Registered,
        "the listing names (id, R0); weights R1 were never listed (§9.1, add-only ADR-0175)"
    );
}
