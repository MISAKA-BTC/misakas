# PALW spec — 15. Model lines and market

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. This chapter
> sits beside the reward path, not on it. The market rides the PALW fold, but under ADR-0144 P1 it is
> not a reward surface. Its EVM-side interfaces are specified in `spec/evm` (Phase 3).

**Purpose.** A model **line** keeps an owner, and the owner keeps publishing versions. Each line can
have a **store** run by consensus. The store opens only with a locked seed of real MSK. Whole
**memberships** are bought from the line's curve and sold back to it, and never move between holders.
Every join and leave burns 5 % and pays the owner 5 %. At `Final`, 5 % of the escrowed worker reward
of a claim that ran the line buys from its pair. No holder is ever paid. This chapter defines those
state transitions and the model-sink rule that keeps value from vanishing unrecorded.

**Principles served:** P1 (the membership sells product access, not remote inference), P7 (a line's
market never changes the unit price of work).

## 15.1 Lines and versions

- [ ] The class keeps its graph. A line keeps its owner, who publishes new weights as versions of the
  line. The roots in force for a line are in chapter 03 §3.5. *Sources:* 0088 (amends 0056 D6 and
  0087 D1/D4/D7). *Code:* `core/palw_model_lines_v1.rs`, fence `palw_model_lines`.

## 15.2 The store

- [ ] A position is bought from the curve and sold back to it. Positions are whole, 500,000 per line.
  *Sources:* 0087 (as amended), 0090. *Code:* `core/palw_model_market_v1.rs`, fence
  `palw_model_market`.
- [ ] A market opens only with a seed of at least the **least seed**: 1,000,000 MSK past
  `palw_model_seed_v2`, 100,000 below it. The seed becomes the reserve, fee-free and locked for good,
  so the reserve never falls under it. *Sources:* 0090, 0120.
- [ ] A seed is paid in as many transactions as it takes (`ModelSeed`, writer action 3). *Sources:* 0094.
- [ ] The owner's leg: past `palw_model_leg_v2` every join and leave pays 5 % burned and 5 % to the
  line's owner. *Sources:* 0114 (amends 0087's 1 %).
- [ ] The reward buys the pair, and no holder is paid: 5 % of the escrowed worker reward at `Final`.
  *Sources:* 0091, chapter 10 §10.8.

## 15.3 Memberships

- [ ] A position is a membership, not an income. It buys what the line declares, and no share or vote.
  *Sources:* 0095 (proposed; this section is written when it is built). *Code:*
  `core/palw_model_benefits_v1.rs`.
- [ ] A membership is proven by the chain, and a position never moves between holders. *Sources:* 0101
  D2/D3/D5 (D4/D6/D7 withdrawn by 0144).

## 15.4 The model sink

- [ ] **A model sink output is block-valid only when bound to an object.** An unbound one would be an
  unrecorded burn. *Fence:* `palw_model_sink_bound` (testnet-12, DAA 750). *Code:*
  `core/palw_model_market_v1.rs`, `cons/processes/transaction_validator/mod.rs`,
  `consensus/core/src/errors/tx.rs`.

## 15.5 The EVM window (pointer)

- [ ] The fold is the truth, and the EVM is its window and its hand: three read precompiles, the
  writer whose actions the fold applies after the block, and the per-class MRC-20 facade. *Sources:*
  0089. *Code:* `core/evm/model_market.rs`, `kaspa-evm/src/model_market.rs`, `contracts/misaka-model/`,
  fence `palw_model_evm`. Specified in `spec/evm`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | `palw_model_sink_bound` from DAA 750. The market fences (`palw_model_market`, `palw_model_lines`, `palw_model_evm`, `palw_model_leg_v2`, `palw_model_seed_v2`) are to be confirmed against `palw_t12_shipped_params` in Phase 2 |

**Design:** `design/palw/market.md`. The app is `web/misaka-options/`.
