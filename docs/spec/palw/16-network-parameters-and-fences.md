# PALW spec — 16. Network parameters and fences

> **Normative.** This chapter states what is armed where, with which values, and how a node proves
> which ruleset it runs. Reasoning and history: [design/palw/lineage.md](../../design/palw/lineage.md).
> The code is the truth, and disagreements are listed in [divergences.md](divergences.md).

**Reconciled with code at:** `55a7be02f` (2026-09-27)

## 16.1 Networks

- **PALW-NP-6.** Each network's PALW ruleset MUST be exactly what its assembly function returns:

  | Network | PALW | Assembly (`consensus/core/src/config/params.rs`) |
  | --- | --- | --- |
  | mainnet | **Disabled** (`PalwConsensusMode::Disabled`); `PALW_MAINNET_GENESIS_BONDS` is empty and the roots are zero | `MAINNET_PARAMS`, `mainnet_shipped_params`. When a genesis card is set, `mainnet_card_base_v1` states what it arms |
  | testnet-12 | `ConsensusV2`, R-core+ from genesis | `palw_t12_base_params` → `palw_t12_arm_every_rule_from_genesis` → `palw_t12_params_with_registry_v1` (arms `PALW_T12_POST_LAUNCH_FENCES_V1` at 750, then `_V2` at 1,300) → `palw_t12_shipped_params` |
  | testnet-11 | `ConsensusV2`, the RC bundle. Retired lineage; runs only on build `1f98d3bf4` | `palw_rc_shipped_params` (§16.9) |
  | devnet / drills | drill rulesets. A drill moves each post-launch list to a low height on a salted chain (`--palw-drill-fence-at`, `--palw-drill-fence2-at`) | `devnet_shipped_params`, `palw_t12_drill_params_v1`, `config/drill.rs` |

- **PALW-NP-7.** Arming PALW on mainnet MUST be one atomic activation bundle with one fork choice and
  one fingerprint (ADR-0042 D1). Every model arrives by registration, drill and binding. Genesis is
  floor-only (ADR-0075 §6–§7, ADR-0076 §4). The mainnet values of the testnet-12-only caps (the Eq cap,
  X10) are separate decisions. Readiness: [mainnet-readiness.md](../../mainnet-readiness.md).

## 16.2 testnet-12: what is armed from genesis

- **PALW-NP-8.** testnet-12 MUST arm at DAA 0 every consensus rule the binary knows, except:
  - `palw_bond_maturity` (DAA 1,000);
  - `palw_inactivity_leak`, `palw_frontier_provenance`, `palw_beacon_fold`, `palw_shard_licensing`,
    `palw_fp_decode_rules` and `palw_fp_decode_constraint`, which stay dormant because validation
    refuses them or this build cannot carry them;
  - the post-launch lists (§16.3, §16.4).

  `palw_t12_arm_every_rule_from_genesis` gives each exception's reason in its doc comments.
- **PALW-NP-9.** The genesis registry MUST hold:
  - the eight operator bonds of `PALW_T12_GENESIS_BONDS` (939,063.21 MSK each);
  - the genesis classes: the floor class, the Qwen2.5 A16 dense rows (8k and 2M, the held rows of
    `PALW_T12_GENESIS_HELD_ROWS`) and the hybrid row at n_ctx 512.

  The measured-row list `palw_class_verify_rows` is empty at genesis, so the 2M row is closed until
  ADR-0153's flag day.

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

**Sources:** ADR-0152 §6 (archive 0152/08).

## 16.3 testnet-12: the DAA-750 set (13 fences; ADR-0154; release `c3dbaee3c`)

- **PALW-NP-10.** `PALW_T12_POST_LAUNCH_FENCES_V1` MUST be armed at `PALW_T12_POST_LAUNCH_FENCE_DAA` =
  750 (`palw_t12_arm_post_launch_fences_v1`). The list is shipped and MUST NOT grow.

| # | Fence | Rule from DAA 750 | Chapter |
| --: | --- | --- | --- |
| 1 | `palw_registry_resilience` | A claim waiting to bind retries until its bind deadline instead of voiding when ready seats cannot fill the panel. A class returns to Probation only when probes from two distinct bonds fail | 07 §7.4, 03 §3.3 |
| 2 | `palw_panel_seed_execution` | The panel seed is the anchor attempt's execution commitment | 08 §8.1 |
| 3 | `palw_heartbeat_transparent_same_chain` | Heartbeat transparency stops at the merging block's own chain | 13 §13.4 |
| 4 | `palw_reorg_strict_economic_win` | A deep reorg needs a strict economic win. A tie keeps the incumbent unless it is at most two DAA ticks deep | 13 §13.2 |
| 5 | `palw_bond_maturity_early` | The 1,000-DAA maturity window applies to non-genesis bonds before 1,000 | 10 §10.1 |
| 6 | `palw_model_sink_bound` | A model sink output is valid only when bound | 15 §15.4 |
| 7 | `palw_operator_anchor` | Only the genesis operator bonds' attempts anchor panels (stopgap) | 07 §7.3 |
| 8 | `palw_final_lock_full_collateral` | A resolved lock is carried by the whole collateral, with a four-floor accuser reserve (V02 a) | 10 §10.4 |
| 9 | `palw_final_lock_life` | A resolved `Valid` seat lock lives until F + 1,000, for claims licensed at or after 750 | 10 §10.3 |
| 10 | `palw_anchor_at_ceiling` | An operator's own attempt at its exposure ceiling still anchors | 07 §7.3 |
| 11 | `palw_slashing_evidence_utxo_genuine` | A DNS slash's UTXO side effect requires genuine evidence | 09 §9.4 |
| 12 | `palw_pruning_proof_strict_economic_win` | A pruning-proof or IBD staging commit needs a strict economic win | 13 §13.2 |
| 13 | `palw_lane_accept_parents_first` | A tied round lane is applied parents first, and a round block carries no EVM payload | 12 §12.3 |

## 16.4 testnet-12: the DAA-1,300 set (2 fences; ADR-0155; release `587cab2b0`)

- **PALW-NP-11.** `PALW_T12_POST_LAUNCH_FENCES_V2` MUST be armed at `PALW_T12_POST_LAUNCH_FENCE_V2_DAA` =
  1,300 (`palw_t12_arm_post_launch_fences_v2`), after the DAA-750 list and before `validate_palw_v2`.
  A fix that goes behind the next flag day is added to this list, and to nothing else.

| # | Fence | Rule from DAA 1,300 | Chapter |
| --: | --- | --- | --- |
| 1 | `palw_floor_refusal_retry` | A draw refused for eligibility at every seed re-anchors at the next slot instead of voiding | 07 §7.4 |
| 2 | `palw_final_lock_life_retro` | At the crossing block, Final seat locks are re-dated to `min(expiry, max(F + 1,000, H))`. After it, locks are dated F + 1,000 | 10 §10.3 |

## 16.5 testnet-12: identity by release

| Release | Change | Params fingerprint | Fence schedule | Schedule id |
| --- | --- | --- | --- | --- |
| `0e8ec984e` (launch, 2026-09-25) | genesis | `b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f` | 1000 | `93da24cc60f7a77e…` |
| `8a0810992` (2026-09-26) | node only: hdrcrash, pptake, palw19, slash refusal, the DAA-198 split fix | same | 1000 | same |
| `c3dbaee3c` (2026-09-27) | the DAA-750 set | `dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9` | 750, 1000 | `7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397` |
| `587cab2b0` (2026-09-27) | the DAA-1,300 set | `24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff` | 750, 1000, 1300 | `d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a` |

The genesis, the identity and the premine txid are unchanged across these releases
([launch note](../../t12-launch-2026-09-25.md) §000–§0).

## 16.6 Values (testnet-12)

| Value | testnet-12 | Where it comes from | Used in |
| --- | --- | --- | --- |
| cadence | 120 s per PALW block | the schedule | 06 |
| `window_bind` / `window_receipt` | 600 / 600 DAA | `PALW_RC_WINDOWS_V1` (`palw_fp_devnet_v3.rs`) | 07 |
| `window_challenge` / `window_challenge_at` | 1,200 / 120 DAA (past `palw_short_challenge_window`) | `PALW_RC_WINDOWS_V1`, `PALW_SHORT_CHALLENGE_WINDOW_DAA_V1` | 07, 08 |
| `window_court` / `claim_retirement` | 3,000 / 3,000 DAA | `PALW_RC_WINDOWS_V1` | 07, 09, 10 |
| `anchor_delay` / `max_beacon_gap` | 20 / 400 DAA | `PALW_RC_WINDOWS_V1` | 07 |
| `court_turn_deadline` | 42 DAA | `PALW_RC_WINDOWS_V1` | 09 |
| `fp_abandon_hold` | 600 DAA | `PALW_RC_WINDOWS_V1` | 07, 10 |
| withdrawal delay | 7,500 DAA (12,900 including the DA lattice) | `PALW_RC_WINDOWS_V1`, `palw_v2_bond_withdrawal_delay_at_v1` | 10 |
| settled-anchor depth | 30 | `PALW_T12_SETTLED_ANCHOR_DEPTH` | 07 |
| bond maturity window | 1,000 DAA | `PALW_T12_BOND_MATURITY_WINDOW_DAA` | 10 |
| producer and registration floor / seat floor | 13,000 / 130,000 MSK | `min_collateral_sompi`; `palw_panel_economy_v1.rs` | 10 |
| work ceiling | 500 ‰ of posted collateral | `fp_max_exposure_ratio_permille` | 10 |
| draw weight cap / eligible-stake floor | 1,000,000 MSK / 875 ‰ | `PALW_DRAW_WEIGHT_CAP_MSK_V1`, `PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1` | 08 |
| readiness-V2 horizon | 24 spans | `palw_readiness_v2_max_age_spans` | 08 |
| execution-quantum maturity | 120 DAA | `palw_exec_quantum_maturity_daa` | 12 |
| resolved `Valid` lock life (from 750) | 1,000 DAA | `PALW_FINAL_LOCK_LIFE_DAA_V1` | 10 |
| C7 window | ≥ 1,000 spans | `PALW_RCORE_C7_WINDOW_SPANS_V1` | 10 |
| Final basis | `basis_k ≥ 2` | `PALW_RCORE_FINAL_BASIS_K_V1` | 08 |
| n_ctx (dense held / narrow dense / hybrid) | A16 held max / 8,192 / 512 | `PALW_T12_DENSE_N_CTX`, `PALW_T12_NARROW_DENSE_N_CTX`, `PALW_T12_HYBRID_N_CTX` | 03, 04 |
| genesis block subsidy | 444,562,014,000 sompi | `PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI` | 10 |

## 16.7 Identity and handshake

- **PALW-NP-12.** A node's params fingerprint (`consensus_params_id`) MUST hash every scheduled rule
  and, through the rule manifest, the rule itself, not only its height. Two builds with one fingerprint
  MUST give one verdict for every block. *Sources:* ADR-0150, ADR-0042. *Code:*
  `core/palw_rule_manifest_v1.rs`.
- **PALW-NP-13.** A node MUST announce a fork id (`fork_id_v1(params, daa_score)`) and a fence schedule.
  From the first fence height a peer has not crossed, the node MUST refuse that peer at the handshake
  ("Fork-id mismatch … this node has crossed fence 750"). *Code:* `core/fork_id_v1.rs`,
  `kaspad/src/daemon.rs` (the startup identity lines).

## 16.8 Adding a fence

- **PALW-NP-14.** A new testnet-12 rule MUST go behind a fence in the current post-launch list, as one
  `PalwPostLaunchFenceV1` entry that sets the field and its bundle mirror through the field's own
  `sync_*`.
- **PALW-NP-15.** Its height MUST NOT be a height another fence already uses (today 750, 1,000 and
  1,300). The fork id cannot tell apart two flag days at one height.
- **PALW-NP-16.** A fence MUST be hashed Some-only with the `never()` collapse, so a dormant field
  leaves every shipped id unchanged. `DnsParams` is hashed whole, so a field never leaves it.
- **PALW-NP-17 (release policy).** A release that arms a fence MUST ship only after a drill that crosses
  the fence with the shipping binary. The rollout procedure is in the runbooks.

## 16.9 testnet-11 (retired; recorded, not maintained)

testnet-11 (`palw_rc_shipped_params`, build `1f98d3bf4`) activates:

| DAA | What it activates |
| --: | --- |
| 7,100 | the held regime and the deep-audit fixes |
| 7,101 | the PALW upgrade bundle labelled `6001`, including ADR-0125's one-permit execution lane |
| 7,200 | ADR-0133 Verification V2 and readiness multiproofs |
| 7,300 | the execution-lane schedule span shortened from 5 DAA to 1 |
| 7,301 | the compute overlay retires |

`6000`, `6001`, `6100` and `6201` are rollout labels, not heights. The history is in
[history/testnet-11.md](../../history/testnet-11.md) and in the ADR index's activation section.

**Sources:** ADR-0035 (testnet-11 continued; class admission pinned), ADR-0036 (mainnet activation
model), ADR-0042 D1 (one bundle, one fingerprint), ADR-0150, ADR-0152 §6, ADR-0154, ADR-0155.
