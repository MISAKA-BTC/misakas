# Retention and foreign GC audit — 2026-11-02

Base: `65c52b77772fbc7948133f8c07778ed14e4788e7`. This isolated source change does not deploy a node or change a consensus fence. Remote hosts, databases, journals, keys and build targets were not accessed or modified by this audit.

## Implemented correction

The periodic foreign-copy sweep in `kaspad/src/palw_panel.rs::prune_foreign_retention` previously consulted the finite in-memory `foreign_pinned` set and, when active, material-income holds. It did not consult each retained file's accepted native claim DA obligation. A cold restart, duty-cache eviction or changed selected branch could therefore leave a Final claim's copy outside the pins, even while its stored retention had not expired. The 72-hour age sweep and the 4,096-file count sweep could delete that copy. The separate retention janitor already protects that obligation, but cannot protect against deletion by the panel's other sweep.

The foreign sweep now asks the live claim oracle for each retained filename and uses `palw_da_material_owed_v1` over the stored claim. Existing duty pins and material-income holds continue to protect files. Owed files remain exempt from both age and count eviction. The change removes the separate income-only directory walk and reads obligations in the sweep itself. It does not add a retained claim map or alter signatures, claim IDs, court windows or admission rules.

## Existing rule and readers audited

| Seam | Source / finding |
|---|---|
| Acceptance | `consensus/core/src/palw_model_panel_one_v1.rs::model_panel_one_effective_attempt_retention_v1`: `max(raw signed deadline, accepted DAA + required span at acceptance)`, activated by original acceptance only. |
| Reservation and stored state | `palw_state_v2.rs::apply_attempt` passes the same effective deadline to court admission, detection-budget lookup and `PalwClaimStateV2.trace_retention_daa`. |
| Producer/rider | `kaspad/src/palw_producer.rs::accepted_rider_retention_v1` reads accepted lead state. Existing test rejects an unaccepted lead and checks the exact rider window. Raw Attempt signature/horizon are unchanged. |
| DA discharge | `palw_da_rcore_v1.rs::palw_da_material_owed_v1` keeps non-voided material through the stored deadline inclusively. |
| Node retention janitor | `kaspad/src/palw_retention.rs::pass_with` reads the stored claim, DA obligation and monetary material hold before age/space deletion. |
| Panel foreign GC | Corrected here to read that same stored DA obligation, independently of finite cached duties. |
| Restart and revert | Existing core retention test round-trips `PalwStateCarriageV2` and applies `revert_delta_v2`. Additional actual-20,000 test covers both with a 10/11/21-DAA generation-to-acceptance delay. |
| Provider fetch | Both panel evidence-provider fetch paths construct `ClaimRoots.retention_deadline` from stored claim state. `EvidenceManifestV1::verify_claim_binding` requires the offered retention to be **at least** that deadline. |
| Monetary provider GC | `misaka/palw/remote/src/monetary_material_gc.rs` uses finalized exact release tuples and crash intent replay, with unknown/failed reads retaining material. This change does not replace it with a raw DAA clock. |

## Provider operational difference that remains

An old provider manifest promising only the raw generation deadline can be shorter than the accepted obligation. For example raw `20604`, stored `20608` is correctly refused with `RetentionTooShort`; this is a lower-bound check, not strict equality. It must not be relaxed to accept a short provider promise. Publication and any separately signed provider/storage terms must actually cover the accepted deadline; an unsigned metadata change cannot extend a signed provider term. Native material-income holds explicitly do not extend signed provider terms (`consensus/core/src/palw_bond_budget_fold_v1/material.rs`).

The generic remote `RetentionMonitor` (`misaka/palw/remote/src/transport.rs::tick`) watches the offered manifest's deadline and expires its watch after that deadline. It neither discovers a later accepted deadline nor deletes the provider's material. A watcher using a pre-acceptance manifest must be supplied with a sufficient refreshed manifest/watch; it cannot establish ongoing accepted-claim coverage from the old raw promise. No live provider inventory, manifest refresh, signed consent change or provider holding duration was verified here. There is no evidence in this audit that the four hosts use this generic monitor.

## Activation and deployment identity

The existing testnet candidate remains at **DAA 20,000**, with ModelOne at **18,640**. The withdrawn **18,850** candidate is not restored. Existing candidate record:

- Params: `253d22bbc32fbcf3f23e9333524435478a5a7b87ac9a360de6513dde4cdc5248`
- Schedule: `ec5f56fa48cdeafb3a6fc953c02a2dcd53f0f54025fe7243a87972884a47eb5e`
- Identity: `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5`

Source: [20,000 candidate record](../model-panel-one-2026-10-31/retention-20000-candidate.json). These are the existing recorded identities, not newly measured deployed fingerprints. No parameter, schedule, release manifest or signature pin changed in this patch. Fresh deployment comparison and confirmation that 20,000 remains future belong to the parent deployment task.

## Validation and limits

Executed: `python3 docs/evidence/palw/oom-retention-2026-11-02/foreign-gc-harness.py`, **2/2 passed**, plus `rustfmt` and `git diff --check`. The [harness](foreign-gc-harness.py) extracts the actual production sweep and the actual two filesystem tests; [output](foreign-gc-harness.log) records the production function SHA-256. It checks age/count eviction, empty cached pins, both `.material` and `.answer`, raw/effective boundary separation, inclusive deadline and changed selected-branch obligation. Hash64 is replaced with u64; this does not test hash parsing, consensus sessions or a running node. The branch change in the GC test is an injected obligation, not a full consensus reorg.

Added but **not executed here** (central Cargo resource budget reserved by parent):

- Core `actual_20000_boundary_keeps_signed_identity_after_long_merge_delay`: actual fold refusal at 19,999; acceptance at 20,000/20,001/20,011; unchanged signed envelope/claim ID/ticket; cold carriage and delta revert.
- Core `future_20000_fence_preserves_all_29_captured_historical_retention_results`: the actual historical 29-claim fixture's original and merge acceptance positions continue to return their raw deadlines. This is retention-policy comparison, not full historical chain replay.
- Node `foreign_gc_rereads_accepted_retention_after_restart_and_branch_change` and existing retention tests with the normal Hash64 type and Node dependencies.

Suggested central filters: `cargo test --locked -p kaspa-consensus-core --lib g14_model_panel_one_retention_v1` and `cargo test --locked -p kaspad --lib foreign_retention`, plus the new `foreign_gc_rereads_accepted_retention` and existing `accepted_effective_obligation` filters. Use the repository's required test-stack settings. Full fleet restart/reorg, live claim acceptance, chosen-validator approval, Court supply and finality remain deployment validation; this local audit does not assert them.
