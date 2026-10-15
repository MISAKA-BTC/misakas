# ADR-0154 — testnet-12's first post-launch flag day: thirteen fences at DAA 750

* Status: **Accepted and armed.** The fences activated on public testnet-12 at DAA 750 on 2026-09-27
  (about 07:29 JST), with release `c3dbaee3c` and params fingerprint `dbbc9104…`.
* Date: 2026-09-26 (decided), 2026-09-27 (armed)
* Decided by: the operator. On 2026-09-26 the decision was to put every CRITICAL and HIGH post-launch
  fix into one fence, with V02 taken as option (a). At 17:05 that day the height moved from DAA 500 to
  DAA 750.
* Amends: [ADR-0152](0152-account-stake-staged-reserve-and-vested-rewards.md) (SW-8's seed and anchor;
  L-3's lock life; A-6's lock budget; B's maturity). It was written after the fact, to close the gap
  of fences with no ADR ([INVENTORY.md](INVENTORY.md), gaps).

## Context

After the launch of 2026-09-25, the pre-freeze audit and the live chain found consensus-level
CRITICAL and HIGH issues:

- the panel seed could be re-rolled for free by re-signing the anchor;
- an unbonded heartbeat miner could absorb public attempts and double-spend within the merge depth;
- a pruning-proof IBD could be taken over;
- round-lane blocks could be applied child-before-parent;
- seat locks filled the work budget, so claims voided with their escrow burned;
- an operator at its exposure ceiling could not anchor, which voided claims in a loop;
- a readiness dip voided claims and reset probation.

The operator chose one flag day for all of them rather than a series of fences, at a height below
bond maturity (DAA 1,000). Two fences at one height would be invisible to the fork id.

## Decision

`PALW_T12_POST_LAUNCH_FENCES_V1`, armed at `PALW_T12_POST_LAUNCH_FENCE_DAA` = 750. The list is shipped
and never grows again. Each entry is hashed Some-only with the `never()` collapse.

| # | Fence | Decision | Spec |
| --: | --- | --- | --- |
| D1 | `palw_registry_resilience` | A no-capable-panel claim retries until its bind window closes. Probation resets only on failed probes from two distinct bonds (V03, V05) | 07 §7.4, 03 §3.3 |
| D2 | `palw_panel_seed_execution` | The panel seed is the anchor attempt's execution commitment (CRITICAL) | 08 §8.1 |
| D3 | `palw_heartbeat_transparent_same_chain` | Heartbeat transparency stops at the merging block's own chain (CRITICAL) | 13 §13.4 |
| D4 | `palw_reorg_strict_economic_win` | A deep reorg needs a strict economic win. A tie within two DAA ticks is decided by GHOSTDAG's order | 13 §13.2 |
| D5 | `palw_bond_maturity_early` | The 1,000-DAA maturity applies to non-genesis bonds from 750 | 10 §10.1 |
| D6 | `palw_model_sink_bound` | A model sink output is valid only when bound to an object | 15 §15.4 |
| D7 | `palw_operator_anchor` | Only the genesis operator bonds' attempts anchor panels (the stopgap for the panel-seed CRITICAL) | 07 §7.3 |
| D8 | `palw_final_lock_full_collateral` | A resolved lock is carried by the whole collateral, with a four-floor accuser reserve (V02 a) | 10 §10.4 |
| D9 | `palw_final_lock_life` | A resolved `Valid` lock lives F + 1,000, and stops being slashable when it stops being committed | 10 §10.3 |
| D10 | `palw_anchor_at_ceiling` | An operator's own attempt at its exposure ceiling still anchors | 07 §7.3 |
| D11 | `palw_slashing_evidence_utxo_genuine` | A DNS slash's UTXO side effect needs genuine evidence (MSK-26A) | 09 §9.4 |
| D12 | `palw_pruning_proof_strict_economic_win` | A pruning-proof or IBD staging commit needs a strict economic win (hf-pptake2) | 13 §13.2 |
| D13 | `palw_lane_accept_parents_first` | Tied round lanes are applied parents first, and a round block carries no EVM payload | 12 §12.3 |

## Consequences

- From DAA 750, nodes older than `c3dbaee3c` (`0e8ec984e`, `8a0810992`) are refused at the handshake.
  A node that crossed 750 on an old build must re-sync. The fence schedule went from `1000` to
  `750, 1000`, with schedule id `7c652212…`.
- Seat throughput recovered (D8, D9). Locks dated before 750 kept licence + 3,000, which led to the
  second flag day ([ADR-0155](0155-testnet-12-flag-day-daa-1300.md)).
- Panel seeding trusts the operators while D7 is armed. External producers' claims bind at operator
  attempts.
- A fresh node syncing from genesis is exempt from D12, so first sync stays with the operators' public
  nodes.

## Links

- Spec: [16 network parameters and fences](../spec/palw/16-network-parameters-and-fences.md) §16.3, and the chapters above
- Design: [design/palw/lineage.md](../design/palw/lineage.md) (flag days), [collateral.md](../design/palw/collateral.md) §6
- Records: [launch note §00](../t12-launch-2026-09-25.md), [t12-panel-seed-2026-09-25.md](../t12-panel-seed-2026-09-25.md),
  `contrib/t12-deploy-kit/DAA750-ROLLOUT.md`
- Code: `consensus/core/src/config/params.rs` (`PALW_T12_POST_LAUNCH_FENCES_V1`, `PALW_T12_POST_LAUNCH_FENCE_DAA`,
  `palw_t12_arm_post_launch_fences_v1`)
