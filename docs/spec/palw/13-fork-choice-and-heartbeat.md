# PALW spec — 13. Fork choice and heartbeat

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/liveness.md](../../design/palw/liveness.md). The code is the truth, and disagreements
> are listed in [divergences.md](divergences.md). The DNS stake reorg gate, the one coupling to DNS
> finality that stays (ADR-0126), is specified in `spec/dns-bft`.

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (heartbeat lane from genesis; the
DAA-750 fork-choice fences)
**Reconciled with code at:** `55a7be02f` (2026-09-27)

Which chain wins. The doctrine: **time is permissionless, weight is bonded, and finality is an
overlay.** A near-weightless heartbeat lane keeps the clock moving when nothing is produced. It must
not touch the economy, the difficulty, fork choice or the clock while the chain is producing.

## 13.1 Weight

- **PALW-FC-2 (the comparator).** Every chain-selection site MUST order PALW candidates by the one pure
  comparator `compare_palw_candidates_v1`. Its keys, in order:
  1. the safe frontier: the blue score of the deepest settled anchor;
  2. the safe weight;
  3. the live total;
  4. the candidate hash, as the last tie-break.

  Weight is a function of the state only, so equal DAGs give equal weights (02 PALW-ST-4).
- **PALW-FC-3 (what weighs).**
  - Settled PALW work weighs, and merged claims count (02 PALW-ST-15).
  - An attempt block's blue work is the constant `1 << PALW_ATTEMPT_BLUE_WORK_LOG` (`palw_attempt_work`).
  - A heartbeat's blue work is ε, and it adds no economic weight.
  - A class's share grows only on work that reached `Final` (`palw_share_growth_final`).
- **PALW-FC-4 (no hash lane).** No lane that `bits` prices is producible on a `ConsensusV2` network, and
  block production is PALW work (06 §6.5). The heartbeat is a clock, never a production path.

**Sources:** ADR-0042 D9, ADR-0039 W4′/W6′, ADR-0058, ADR-0066 D3 and ADR-0068 F2, ADR-0107 → ADR-0137.
**Code:** `core/palw_fork_choice.rs` (`compare_palw_candidates_v1`), `core/palw_chain_weight.rs`,
`core/palw_weight.rs`.

## 13.2 Reorg authority

- **PALW-FC-5 (deep reorg).** A reorg deeper than the finality depth MUST be allowed only if the
  challenger is `Greater` under the comparator (`decide_deep_reorg_v2`).
- **PALW-FC-6 (strict economic win, from DAA 750).** Past `palw_reorg_strict_economic_win`, a deep reorg
  MUST also strictly exceed the incumbent on at least one economic key (the first three keys of
  PALW-FC-2). An all-economic tie keeps the incumbent, whatever the hash, with one exception: a tie at
  most `PALW_REORG_SHALLOW_TIE_DAA_V1` = 2 DAA ticks deep on the incumbent's side is decided by
  GHOSTDAG's order (`palw_reorg_shallow_ghostdag_win_v1`), so honest slot races converge. A switch
  never lowers the sink's DAA. (`palw_reorg_strict_economic_win_v1`.)
- **PALW-FC-7 (pruning proof and IBD, from DAA 750).** Past `palw_pruning_proof_strict_economic_win`, a
  staging commit from a pruning proof or IBD MUST strictly win on economics, read at the incumbent's
  DAA. A node syncing from genesis has no incumbent and is exempt, so first sync should use trusted
  public nodes.
- **PALW-FC-8 (pruning-proof verification).** Pruning-proof PoW verification MUST be exhaustive, not
  sampled. Its per-header cost is amortised by keeping the model resident across jobs (ADR-0041 D1′).
- **PALW-FC-9 (dormant).** Frontier provenance at the deep-reorg gate (`palw_frontier_provenance`,
  ADR-0065 D2a) and the finality inactivity leak (`palw_inactivity_leak`, ADR-0060 D4 / 0066 D4) are
  dormant on testnet-12, and validation refuses them.
- **PALW-FC-10 (the DNS gate).** The DNS stake reorg gate may additionally refuse a reorg past a
  validator-confirmed anchor (ADR-0126, ADR-0128). It never enables one that PALW fork choice would
  refuse.

**Sources:** ADR-0042 D9; ADR-0127 D1/D3; ADR-0154 (D4, D12); ADR-0041; ADR-0065 D2a; ADR-0060 D4.
**Code:** `core/palw_fork_authority_v2.rs` (`decide_deep_reorg_v2`, `palw_reorg_strict_economic_win_v1`,
`PALW_REORG_SHALLOW_TIE_DAA_V1`), `cons/pipeline/virtual_processor/processor.rs`,
`protocol/flows/src/ibd/flow.rs`.

## 13.3 The heartbeat lane

- **PALW-FC-11 (the lane).** Heartbeats use their own algorithm id (`algo_id` 8) with a fixed target
  that never touches `bits`. The slot rule is one block deep. At most four heartbeats may sit in one
  mergeset or chain. A heartbeat is admissible unconditionally: no bonded signature, and no rule keyed
  on whether the chain is producing.
- **PALW-FC-12 (the cursor).** A heartbeat consumes a clock slot. A block that does not advance the
  clock may not postpone it (06 §6.4).
- **PALW-FC-13 (non-interference).** While the chain is producing, the heartbeat MUST NOT touch the
  economy, the difficulty, fork choice or the clock. It carries no claim, no reward and no economic
  weight, and it is not counted by the difficulty window. When the chain stops, the heartbeat alone
  keeps the DAA moving (10 PALW-CO-44).
- **PALW-FC-14 (when to mine).** When to mine heartbeats is node policy: the heartbeat miner steps
  aside for a draw that has landed. *(node policy)*
- **PALW-FC-1 (H-1, heartbeat carriers).** Past `palw_rcore_plus`, a heartbeat block MUST be able to
  carry every conviction-bearing, DA and reporter object, and the fold applies them at step 3. That
  covers `ObjectiveOffence` (kinds 3 and 4), `ExecutorEquivocation`, `DefaultAccused`,
  `DefaultAccusedHeld`, `MaterialDisclosedV2`, `ReporterCommitted`, `ReporterRevealed`, `CourtOpened`,
  and court moves where the phase allows. The heartbeat miner MUST include them, and relay MUST NOT
  drop a heartbeat for carrying them. *Sources:* ADR-0152 H-1 (archive 0152/03).

**Sources:** ADR-0060 (the doctrine), ADR-0066 D1–D2, ADR-0068 F3a/F5, ADR-0140 D1–D5, ADR-0142.
**Code:** `core/palw_heartbeat_v1.rs`, `core/palw_heartbeat_carriers_v1.rs`, `core/palw_clock_cursor_v1.rs`;
node side `kaspad/src/palw_heartbeat_miner.rs`.

## 13.4 Heartbeat transparency

- **PALW-FC-15 (transparency).** Past `palw_heartbeat_transparent` (testnet-12 from genesis), GHOSTDAG
  colours each mergeset candidate by one of three rules, chosen by the candidate's own header
  (`LaneColoring`):
  - **Classic:** the k-cluster rule, used below the fence.
  - **Weighted:** used for a non-heartbeat block. Heartbeats are invisible to it: they are neither
    counted in its anticone nor enlarge its count.
  - **Heartbeat:** used for a heartbeat. It is counted against every blue as before, but it never
    enlarges a non-heartbeat block's recorded count.

  The exemption stops at the merge-depth window. A heartbeat therefore never turns a bonded block red.
- **PALW-FC-16 (same chain, from DAA 750).** Past `palw_heartbeat_transparent_same_chain`, the
  transparency MUST stop at the merging block's own chain. This closes the double spend by which an
  unbonded heartbeat miner could absorb public attempts within the merge depth.

**Sources:** ADR-0105 D1; ADR-0154 (D3). **Code:** `cons/processes/ghostdag/protocol.rs` (`LaneColoring`),
`config/params.rs` (`palw_heartbeat_transparent_fence`, `palw_heartbeat_transparent_same_chain_fence`).

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | heartbeat lane, attempt work and transparency from genesis. DAA 750: `palw_reorg_strict_economic_win`, `palw_pruning_proof_strict_economic_win`, `palw_heartbeat_transparent_same_chain`. Dormant: `palw_inactivity_leak`, `palw_frontier_provenance` |
