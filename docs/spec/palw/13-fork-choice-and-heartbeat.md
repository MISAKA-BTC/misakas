# PALW spec — 13. Fork choice and heartbeat

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. The DNS stake
> reorg gate, the one coupling to DNS finality that stays (0126), is specified in `spec/dns-bft`.

**Purpose.** This chapter defines which chain wins. Weight comes from bonded, verified work. Time is
permissionless: a near-weightless heartbeat lane keeps the clock moving when nothing is produced, and
it must not touch the economy, the difficulty, the fork choice or the clock while the chain is
producing. A deep reorg needs a **strict economic win**, and a tie keeps the incumbent, except within
two DAA ticks, where GHOSTDAG's order decides. The same strictness applies to a pruning-proof or IBD
switch. This chapter also defines heartbeat transparency, and where it stops.

**Principles served:** none of P1–P7 directly. This chapter protects what the other chapters pay for:
a claim's weight counts only on the chain that wins.

## 13.1 Weight

- [ ] Chain weight: what counts (merged claims, settled work), and the two derived weights that replace
  hash work in fork choice. *Sources:* 0039 W4′/W6′, 0038 (amended), 0058, 0149 (the weight reads the
  derivation). *Code:* `core/palw_chain_weight.rs`, `core/palw_weight.rs`, `core/palw_fork_choice.rs`.
- [ ] Attempt blue work is a constant, and heartbeats add no economic weight. *Sources:* 0066 D3, 0068
  F2/F3a/F5.
- [ ] Share growth counts only Final work. *Sources:* 0107 → 0137. *Code:* fence
  `palw_share_growth_final`.

## 13.2 Reorg authority: the strict economic win

- [ ] **A deep reorg needs a strict economic win, and a tie keeps the incumbent.** The exception: a tie
  at most two DAA ticks deep on the incumbent's side is decided by GHOSTDAG's order, so honest slot
  races converge. A switch never lowers the sink's DAA. *Fence:* `palw_reorg_strict_economic_win`
  (DAA 750). *Sources:* the post-launch audit (lane `rcore/f1-strictwin-tie`), the launch note §00
  and §3. There is no ADR yet. *Code:* `core/palw_fork_authority_v2.rs`
  `palw_reorg_strict_economic_win_v1`, `cons/pipeline/virtual_processor/processor.rs`.
- [ ] **Pruning proof and IBD.** A staging commit from a pruning proof or IBD needs a strict economic
  win, read at the incumbent's DAA. A node syncing from genesis is exempt, so first sync stays with the
  operators' public nodes. *Fence:* `palw_pruning_proof_strict_economic_win` (DAA 750, hf-pptake2).
  *Code:* `core/palw_fork_authority_v2.rs`, `protocol/flows/src/ibd/flow.rs`.
- [ ] Pruning-proof verification is exhaustive and amortised, not sampled. *Sources:* 0041 (D1′).
- [ ] Frontier provenance at the deep-reorg gate is dormant on testnet-12 (`palw_frontier_provenance`,
  refused by validation). *Sources:* 0065 D2a.
- [ ] How PALW fork choice composes with the DNS stake reorg gate (a pointer). *Sources:* 0126, 0128.

## 13.3 The heartbeat lane

- [ ] The doctrine: time is permissionless, weight is bonded, and finality is an overlay. *Sources:*
  0060 (in 0066's form), 0140 (the heartbeat is the emergency generator: the five invariant claims).
- [ ] The lane's own algorithm (`algo_id = 8`), a fixed target, and at most four heartbeats per
  mergeset or chain. *Sources:* 0066 D1–D2, 0068 F3a/F5. *Code:* `core/palw_heartbeat_v1.rs`,
  `core/palw_heartbeat_carriers_v1.rs`, fence `palw_heartbeat`.
- [ ] A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has
  landed. *Sources:* 0105.
- [ ] The heartbeat consumes a clock slot (chapter 06 §6.4). *Sources:* 0142.

## 13.4 Heartbeat transparency

- [ ] What heartbeat transparency lets through (fence `palw_heartbeat_transparent`, testnet-12 only).
  *Sources:* 0105 (as armed on testnet-12). *Code:* `palw_heartbeat_transparent_fence`.
- [ ] **Transparency stops at the merging block's own chain.** It closes the double spend by an
  unbonded heartbeat miner absorbing public attempts within the merge depth. *Fence:*
  `palw_heartbeat_transparent_same_chain` (DAA 750, CRITICAL). *Code:*
  `cons/processes/ghostdag/protocol.rs`, `palw_heartbeat_transparent_same_chain_fence`.

## 13.5 What stays dormant

- [ ] The finality inactivity leak (`palw_inactivity_leak`): dormant on testnet-12 and refused by
  validation. State the rule it would add, and why it cannot be armed. *Sources:* 0060 D4, 0066 D4.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | heartbeat lane and transparency from genesis. DAA 750: `palw_reorg_strict_economic_win`, `palw_pruning_proof_strict_economic_win`, `palw_heartbeat_transparent_same_chain`. Dormant: `palw_inactivity_leak`, `palw_frontier_provenance` |

**Design:** `design/palw/liveness.md`.
