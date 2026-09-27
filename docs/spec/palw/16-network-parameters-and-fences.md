# PALW spec — 16. Network parameters and fences

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. The fence
> tables below are taken from `consensus/core/src/config/params.rs` at `55a7be02f` and from the
> [launch note](../../t12-launch-2026-09-25.md) §000–§00. Phase 2 writes the values table and checks
> every row against `palw_t12_shipped_params()`.

**Purpose.** This chapter is the reference for what is armed where. It covers each network's PALW
ruleset, every fence and its height, the values the other chapters leave to it, the identity a node
announces (params fingerprint, fork id, schedule id), and the rules for adding a fence without
splitting the network silently.

## 16.1 Networks

| Network | PALW | Assembly |
| --- | --- | --- |
| mainnet | **Disabled** (`PalwConsensusMode::Disabled`). No genesis bonds or roots | `MAINNET_PARAMS` |
| testnet-12 | R-core+ from genesis | `palw_t12_base_params` → `palw_t12_arm_every_rule_from_genesis` → `palw_t12_params_with_registry_v1` (arms `PALW_T12_POST_LAUNCH_FENCES_V1` at 750, then `_V2` at 1,300) → `palw_t12_shipped_params` |
| testnet-11 | Retired lineage (build `1f98d3bf4`) | `TESTNET11_PARAMS` |
| devnet / drill | Drill rulesets. A drill moves each post-launch list to a low height on a salted chain | `devnet_shipped_params`, `config/drill.rs` (`palw_drill_post_launch_fences_v2_at_v1`), `palw_t12_drill_params_v1` |

- [ ] **Mainnet.** State that PALW is off, and what arming it would take: one atomic activation
  bundle, one fork choice, one fingerprint, with every model arriving by registration, drill and
  binding. *Sources:* 0042 D1, 0036, 0075 §6–§7, `docs/mainnet-readiness.md`.

## 16.2 testnet-12: what is armed from genesis

- [ ] Every consensus rule this binary knows is armed at DAA 0, except:
  - `palw_bond_maturity`, at DAA 1,000.
  - Six rules that stay dormant because validation refuses them or this build cannot carry them:
    `palw_inactivity_leak`, `palw_frontier_provenance`, `palw_beacon_fold`, `palw_shard_licensing`,
    `palw_fp_decode_rules`, `palw_fp_decode_constraint`.

  *Code:* `palw_t12_arm_every_rule_from_genesis` (its doc comments give each exception's reason).
- [ ] The genesis registry: the eight operator bonds (`PALW_T12_GENESIS_BONDS`), the genesis classes
  and held rows (`PALW_T12_GENESIS_HELD_ROWS`; the Qwen2.5 A16 8k and 2M roots, and the hybrid at
  n_ctx 512).

## 16.2a testnet-12: the R-core+ fences (ADR-0152)

- **PALW-NP-1.** `palw_offence_attribution` MUST be armed at DAA 0 only, and only with
  `palw_objective_offence`, `palw_audit_2026_09_23`, `palw_verification_v2`, `palw_economic_safety`
  and `palw_prefill_draw` at 0.
- **PALW-NP-2.** `palw_rcore_plus` MUST be armed at DAA 0 only (genesis only). `validate_palw_rcore_plus_v1`
  refuses it unless all 14 prerequisites are armed at or below it:
  - `palw_offence_attribution`, `palw_admission_independence`, `palw_audit_2026_09_23`,
    `palw_audit_2026_09_11`, `palw_economic_safety`, `palw_objective_offence`, `palw_panel_economy`;
  - `palw_panel_exposure_floor` (its `.activation`), `palw_unavailable_abstains`, `palw_clock_floor`,
    `palw_clock_cursor`, `palw_verification_v2`, `palw_da_court`, `palw_operator_id_unique`.

  It also refuses when:
  - `palw_settled_anchor_depth` is missing;
  - `palw_shard_licensing` is armed beside it;
  - the signature-context root is not `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V5`;
  - the panel quorum is not `PALW_PANEL_COLLUDING_QUORUM_V1`;
  - the bundle's mirrors differ from the params. The mirrors are `rcore_plus_from_daa`,
    `withdrawal_delay_daa` (the delay including the DA lattice) and `rcore_conservative_classes`, set by
    `sync_palw_rcore_plus`.
- **PALW-NP-3.** `palw_rcore_conservative_classes` (C7's list) MUST be non-empty only with
  `palw_rcore_plus`, and MUST be a subset of `PALW_T12_GENESIS_HELD_ROWS`. On testnet-12 it holds the
  2M class. It is hashed only when non-empty.
- **PALW-NP-4.** `palw_rcore_attributed_charging` (X10) is not declared. When it is added, it is a
  Some-only fence armed at or above `palw_rcore_plus` by a public flag day.
- **PALW-NP-5.** A node MUST refuse to start a network that arms `palw_rcore_plus` from a build whose
  `PALW_RCORE_VESTING_ROWS_LANDED_V1` is false (`palw_rcore_build_can_run_v1`). It MUST also refuse
  testnet-11 parameters or a testnet-11 datadir (state version 22 against 20).

**Sources:** ADR-0152 §6 (archive 0152/08). **Code:** `config/params.rs` (`validate_palw_rcore_plus_v1`,
`sync_palw_rcore_plus`, `palw_t12_arm_every_rule_from_genesis`).

## 16.3 testnet-12: the DAA-750 set (13 fences, release `c3dbaee3c`)

`PALW_T12_POST_LAUNCH_FENCES_V1`. It was armed at `PALW_T12_POST_LAUNCH_FENCE_DAA` = 750 and is
shipped, so it never grows.

| # | Fence | Rule from DAA 750 | Chapter |
| --: | --- | --- | --- |
| 1 | `palw_registry_resilience` | A claim waiting to bind retries until its bind deadline instead of voiding when ready seats cannot fill the panel. A class returns to Probation only when probes from two or more distinct bonds fail | 07 §7.4, 03 §3.3 |
| 2 | `palw_panel_seed_execution` | The panel seed is the anchor attempt's execution commitment | 08 §8.1 |
| 3 | `palw_heartbeat_transparent_same_chain` | Heartbeat transparency stops at the merging block's own chain | 13 §13.4 |
| 4 | `palw_reorg_strict_economic_win` | A deep reorg needs a strict economic win. A tie keeps the incumbent unless it is at most two DAA ticks deep | 13 §13.2 |
| 5 | `palw_bond_maturity_early` | The 1,000-DAA maturity window applies to non-genesis bonds before 1,000 | 10 §10.1 |
| 6 | `palw_model_sink_bound` | A model sink output is valid only when bound | 15 §15.4 |
| 7 | `palw_operator_anchor` | Only the genesis operator bonds' attempts anchor panels (stopgap) | 07 §7.3 |
| 8 | `palw_final_lock_full_collateral` | A resolved lock is carried by the whole collateral, with a four-floor accuser reserve (V02 a) | 10 §10.3 |
| 9 | `palw_final_lock_life` | A resolved `Valid` seat lock lives until F + 1,000, for claims licensed at or after 750 | 10 §10.3 |
| 10 | `palw_anchor_at_ceiling` | An operator's own attempt at its exposure ceiling still anchors | 07 §7.3 |
| 11 | `palw_slashing_evidence_utxo_genuine` | A DNS slash's UTXO side effect requires genuine evidence | 09 §9.4 |
| 12 | `palw_pruning_proof_strict_economic_win` | A pruning-proof or IBD staging commit needs a strict economic win | 13 §13.2 |
| 13 | `palw_lane_accept_parents_first` | A tied round lane is applied parents first, and a round block carries no EVM payload | 12 §12.3 |

## 16.4 testnet-12: the DAA-1,300 set (2 fences, release `587cab2b0`)

`PALW_T12_POST_LAUNCH_FENCES_V2`, armed at `PALW_T12_POST_LAUNCH_FENCE_V2_DAA` = 1,300. The next
flag day's fixes are added here.

| # | Fence | Rule from DAA 1,300 | Chapter |
| --: | --- | --- | --- |
| 1 | `palw_floor_refusal_retry` | A draw refused for eligibility re-anchors at the next slot instead of voiding | 07 §7.4 |
| 2 | `palw_final_lock_life_retro` | At the crossing block, Final seat locks are re-dated to `min(expiry, max(F + 1,000, H))`. After it, locks are dated F + 1,000 | 10 §10.3 |

## 16.5 testnet-12: identity by release

| Release | Change | Params fingerprint | Fence schedule |
| --- | --- | --- | --- |
| `0e8ec984e` (launch, 2026-09-25) | genesis | `b8564b88…` | 1000 |
| `8a0810992` (2026-09-26) | node-only update (hdrcrash, pptake, palw19, slash refusal, split fix) | `b8564b88…` | 1000 |
| `c3dbaee3c` (2026-09-27) | the DAA-750 set | `dbbc9104…` | 750, 1000 |
| `587cab2b0` (2026-09-27) | the DAA-1,300 set | `24e1aec3…` | 750, 1000, 1300 |

- [ ] Add the full fingerprints and schedule ids from the launch note, and keep this table current.

## 16.6 Values (Phase 2)

- [ ] One row per value, with its constant and the chapter that uses it. At least: cadence (120 s),
  settled-anchor depth (`PALW_T12_SETTLED_ANCHOR_DEPTH` = 30), bond maturity window (1,000 DAA), draw
  weight cap (`PALW_DRAW_WEIGHT_CAP_MSK_V1` = 1,000,000 MSK), eligible-stake floor (875 ‰), readiness
  horizon (24), λ (5), execution-quantum maturity (120 DAA), genesis block subsidy
  (`PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI`), n_ctx per row (`PALW_T12_DENSE_N_CTX`,
  `PALW_T12_NARROW_DENSE_N_CTX` = 8,192, `PALW_T12_HYBRID_N_CTX` = 512), the seat and producer floors,
  and the challenge window with the finality depth derived from it.

## 16.7 Identity and handshake

- [ ] The params fingerprint hashes every scheduled rule, not only its height. The rule manifest.
  *Sources:* 0150, 0042. *Code:* `core/palw_rule_manifest_v1.rs`.
- [ ] The fork id and the handshake: a node that has crossed a fence refuses peers that have not
  ("Fork-id mismatch … this node has crossed fence 750"). The schedule id. *Code:*
  `core/fork_id_v1.rs`, `kaspad/src/daemon.rs` (the start-up identity lines).

## 16.8 Adding a fence

- [ ] A new testnet-12 fix goes behind a fence in the current post-launch list, as one entry that sets
  the field and its mirror through the field's own `sync_*`.
- [ ] Its height MUST be one that no other fence uses (not 750, 1,000 or 1,300). The fork id cannot
  tell apart two flag days at one height.
- [ ] It is hashed Some-only, with the `never()` collapse, so a dormant list leaves every shipped id
  unchanged.
- [ ] `DnsParams` is hashed whole, so a field never leaves it.
- [ ] A release that arms a fence ships only after a drill that crosses the fence with the shipped
  binary. *(release policy; the runbook has the procedure)*

## 16.9 testnet-11 (retired)

- [ ] The activation map (DAA 7,100 held regime and deep audit; 7,101 the `6001`-labelled bundle
  including the one-permit execution lane; 7,200 Verification V2 and readiness multiproofs; 7,300 span
  5 → 1 DAA; 7,301 the compute overlay retires), moved here from `adr/README.md` together with its
  "Activation axis" section. Recorded, not maintained.
