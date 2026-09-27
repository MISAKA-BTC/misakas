# PALW spec — 15. Model lines and market

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/market.md](../../design/palw/market.md). The code is the truth, and disagreements are
> listed in [divergences.md](divergences.md). The market rides the PALW fold beside the reward path.
> Under ADR-0144 P1 it is not a reward surface. Its EVM interfaces are specified in `spec/evm`
> (Phase 3).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (`palw_model_market`,
`palw_model_lines`, `palw_model_evm`, `palw_model_leg_v2` and `palw_model_seed_v2` are armed by the RC
base and moved to DAA 0 by testnet-12's arming walk; `palw_model_sink_bound` from DAA 750)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P1 (a membership sells product access, never remote inference); P7 (a line's
market never changes the unit price of work).

## 15.1 Lines and versions

- **PALW-MK-1 (the line).** A line is `(class, owner, name)`. A class's own line is its first, and its id
  is the class id. A class keeps its graph: it remains the unit of work, share, certification and
  court. New weights are a new **version** of a line, never a new class. A new graph is a new class.
- **PALW-MK-2 (versions).** A version is one object, signed by the line's developer
  (`ModelVersionPublished`), then promoted or withdrawn (`ModelVersionPromoted`, `ModelVersionWithdrawn`).
  A class's roots in force are the union of its lines' (03 PALW-CL-18). The fold counts usage per
  version.
- **PALW-MK-3 (roles).** A line has an owner, developers and maintainers (`ModelLineRolesSet`). The owner
  may hand the line over (`ModelLineOwnerTransferred`) or retire it (`ModelLineRetired`). Evaluations
  (`ModelEvaluationPosted`) are declarations from anyone, and each says who declared it. Proposals are
  recorded open research (`ModelProposalPosted`, `ModelProposalClosed`).

**Sources:** ADR-0088 D1–D12. **Code:** `core/palw_model_lines_v1.rs`; objects tags 26–35.

## 15.2 The store

- **PALW-MK-4 (positions).** A position MUST be a whole balance in the fold: not a coin, not a UTXO. A
  line has 500,000 positions. No transfer exists: positions are bought from the line's curve and sold
  back to it (`ModelBuy`, `ModelSell`), and never move between holders.
- **PALW-MK-5 (the curve).** The curve is constant product over the reserve. Its product never falls, so
  the reserve never falls below the seed.
- **PALW-MK-6 (the seed).** A market MUST open only when its seed reaches the **least seed**: 1,000,000
  MSK past `palw_model_seed_v2`, and 100,000 below it. The seed:
  - becomes the reserve, fee-free and locked for good, and nobody is ever paid it out;
  - is paid through `ModelSeed` in as many transactions as it takes, each payment locking on arrival;
  - may be paid by anyone, and the row names who started it;
  - opens the market with the whole supply on the payment that crosses the floor.
- **PALW-MK-7 (the legs).** Past `palw_model_leg_v2`, every join and leave MUST pay 5 % burned and 5 % to
  the line's owner, from the MSK leg. Below it: 5 % burned and 1 % to the owner. The seed pays no leg.
- **PALW-MK-8 (the reward buys the pair).** At a claim's `Final`, 5 % of its escrowed worker reward
  (the buyback slice `s`, `palw_model_buyback_slice_v1`) MUST buy positions from the pair of the line
  the claim ran. The chain keeps the positions it buys, for good, and never pays or moves them. Where
  the line has no open pair, the miner is paid in full and nothing is burned in its place. A voided
  claim buys nothing. The slice is a carve of the escrow, never an addition to emission.

**Sources:** ADR-0087 D1–D8 (as amended), ADR-0090 D1–D7, ADR-0094 D1–D6, ADR-0120, ADR-0114, ADR-0091
D1–D8. **Code:** `core/palw_model_market_v1.rs`; objects tags 24, 25, 36.

## 15.3 Memberships

- **PALW-MK-9 (a membership, never money).** A position buys no share, no vote and no income. It buys
  what its line declares for holders (`ModelLineBenefitsDeclared`): early versions, private betas, the
  front of a queue, experimental modes. The line controls the product, providers control the serving,
  and the chain proves the membership.
- **PALW-MK-10 (proof of membership).** Anyone MAY serve a line's holders. A client checks a provider's
  signed service descriptor against chain facts (`palw_benefit_server_v1`). Discovery is not chain
  state, and serving is not adjudicated. Public anyone-serves inference is not a PALW reward path
  (ADR-0144).

**Sources:** ADR-0095 (proposed; the benefit declarations are built), ADR-0101 D1–D3 and D5 (D4, D6 and
D7's public serving withdrawn by 0144), ADR-0100 D8. **Code:** `core/palw_model_benefits_v1.rs`,
`core/palw_service_descriptor_v1.rs`.

## 15.4 The model sink

- **PALW-MK-11.** From DAA 750 (`palw_model_sink_bound`), a transaction output to a model sink MUST be
  bound to a market object. An unbound sink output is block-invalid, because it would be an unrecorded
  burn. *Code:* `core/palw_model_market_v1.rs`, `cons/processes/transaction_validator/mod.rs`,
  `consensus/core/src/errors/tx.rs`.

**Sources:** ADR-0154 (D6).

## 15.5 The EVM window

- **PALW-MK-12.** The fold is the truth, and the EVM is its window and its hand:
  - The EVM reads the fold through system addresses and read precompiles, at the EVM block's selected
    parent.
  - The MRC-20 facade has no `Transfer`.
  - The `ModelWriter` performs buy, sell and seed actions, which the fold applies after the block and
    which settle one block later.
  - EVM holders and fold holders are separate namespaces.

  Specified in `spec/evm`.

**Sources:** ADR-0089 D1–D12. **Code:** `consensus/core/src/evm/model_market.rs`,
`kaspa-evm/src/model_market.rs`, `contracts/misaka-model/`; the app is `web/misaka-options/`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | the market, lines, EVM window, the 5 % leg and the 1,000,000 MSK least seed from genesis. `palw_model_sink_bound` from DAA 750 |
