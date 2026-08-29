//! **The standard battery every lineage — and therefore every new LLM — runs before it exists.**
//!
//! Each profile module used to copy the same four or five tests by hand: the profile validates,
//! every reachable kernel is catalogued, the coverage gate certifies, the canonical job counts
//! inside the worst case, every reference points backwards. Copied prose drifts (the `(8, 2)`
//! literal that went stale the day the derivation moved to `(7, 2)` is on record in
//! `palw_qwen36_profile`), and a lineage that forgot one test shipped a class whose hole surfaced
//! at registration or — worse — at adjudication.
//!
//! [`check_lineage_v1`] is that battery as one call, over every entry a lineage supplies. The
//! SDK's own tests run it over the built-in lineages, so a new table row is covered the moment it
//! exists; a NEW lineage's first test should be this call, and the trait's documentation says so.
//! These are the ADJUDICABILITY invariants — what the admission gate will enforce on chain, minus
//! the per-network pieces (ladder depth and cost ceilings are the bundle's, checked by
//! [`crate::PalwClassSdk::preflight_admission`] at registration time; a class may conform and
//! still be inadmissible under a court that will not pay for it, which is a network's decision
//! and not a defect).

use kaspa_consensus_core::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1, verify_profile_coverage_v1};
use kaspa_consensus_core::palw_class_admission_v2::{derive_court_cost_v1, reachable_kernels_v1};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_SENTINEL_MIN, step_leaf_count, worst_case_step_leaf_count_v1};

use crate::lineage::PalwModelLineageV1;
use crate::sdk::PalwClassSdk;

/// Run the battery over one lineage. `Err` names the entry and the invariant, first failure wins —
/// the point is a red test that says which class broke, not a survey.
pub fn check_lineage_v1(lineage: &dyn PalwModelLineageV1, court: &PalwCourtParamsV2) -> Result<(), String> {
    let entries = lineage.classes(court);
    let mut seen_ids = std::collections::BTreeSet::new();
    for entry in &entries {
        let who = entry.model_id;
        if entry.lineage_id != lineage.lineage_id() {
            return Err(format!("{who}: stamped lineage {} inside lineage {}", entry.lineage_id, lineage.lineage_id()));
        }
        if !seen_ids.insert(entry.class_id()) {
            return Err(format!("{who}: shares a class id with another entry — two models must be two classes"));
        }
        // The graph a court can walk: every table inside the cap, every width non-zero.
        entry.profile.validate_shape().map_err(|e| format!("{who}: the shape does not validate: {e}"))?;
        // Every reference strictly earlier — the property that makes a step's input set
        // canonical, and the one a hand-written table gets wrong.
        for (name, table) in [
            ("pre", &entry.profile.pre_nodes),
            ("gdn", &entry.profile.gdn_nodes),
            ("attn", &entry.profile.attn_nodes),
            ("post", &entry.profile.post_nodes),
        ] {
            for (i, node) in table.iter().enumerate() {
                for r in &node.input_refs {
                    if *r < PALW_STEP_INPUT_SENTINEL_MIN && *r as usize >= i {
                        return Err(format!("{who}: {name} node {i} reads node {r}, which is not earlier"));
                    }
                }
            }
        }
        // ADR-0039's precondition: every kernel the graph reaches is one the adjudicator
        // re-executes — through the gate's own constructor, so "we checked coverage" and "a
        // certificate exists" stay one fact. Both halves: the id set AND the per-node shape
        // service (the strong gate audit H-02 found uncalled).
        let kernel_ids = reachable_kernels_v1(&entry.profile);
        verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: entry.class_id(), kernel_ids })
            .map_err(|e| format!("{who}: a reachable kernel is uncatalogued: {e:?}"))?;
        verify_profile_coverage_v1(&entry.profile).map_err(|e| format!("{who}: a node's shape is not servable: {e:?}"))?;
        // The canonical job: countable, non-empty, inside the class's own worst case and inside
        // the context the class declares (in the enumeration's own footprint form).
        let worst =
            worst_case_step_leaf_count_v1(&entry.profile).map_err(|e| format!("{who}: the step space does not enumerate: {e:?}"))?;
        let canonical = entry.canonical_context();
        let counted =
            step_leaf_count(&entry.profile, &canonical).map_err(|e| format!("{who}: the canonical job does not count: {e:?}"))?;
        if counted == 0 {
            return Err(format!("{who}: the canonical job commits no step leaves"));
        }
        if counted > worst {
            return Err(format!("{who}: the canonical job counts {counted} leaves against a worst case of {worst}"));
        }
        let footprint =
            (canonical.declared_prefill_tokens as u64).saturating_add(canonical.exact_decode_tokens.max(1) as u64).saturating_sub(1);
        if footprint > entry.profile.n_ctx as u64 {
            return Err(format!(
                "{who}: the canonical job touches {footprint} cached positions and the class registers n_ctx {}",
                entry.profile.n_ctx
            ));
        }
        // What prosecuting the class costs must at least DERIVE — whether a bundle pays for it is
        // that bundle's admission decision, not conformance's.
        derive_court_cost_v1(&entry.profile).map_err(|e| format!("{who}: the court cost does not derive: {e}"))?;
    }
    Ok(())
}

/// The cross-lineage half: class ids distinct across the WHOLE ledger, and every lineage passing
/// its own battery. This is the one call a build's test suite needs.
pub fn check_sdk_v1(sdk: &PalwClassSdk) -> Result<(), String> {
    for lineage in sdk.lineages() {
        check_lineage_v1(lineage.as_ref(), sdk.court())?;
    }
    let mut seen = std::collections::BTreeMap::new();
    for entry in sdk.ledger() {
        if let Some(other) = seen.insert(entry.class_id(), entry.model_id) {
            return Err(format!("{} and {other} derive one class id — two models must be two classes", entry.model_id));
        }
    }
    Ok(())
}
