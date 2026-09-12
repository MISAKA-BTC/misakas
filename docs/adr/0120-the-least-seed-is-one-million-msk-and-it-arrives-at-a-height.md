# ADR-0120 — The least seed is one million MSK, and it arrives at a height

* Status: PROPOSED 2026-09-12 at the operator's request ("misaka position のモデル追加の際に流動性として
  ロックして引き出せないようにする misaka の数を 1M として　今の 100000 枚から引き上げて"). **IMPLEMENTED
  the same day** on `feat/adr-0120-seed-min-1m`, and **scheduled on testnet-11 at DAA 6,900** by the
  operator the same day, to ship with the 7,000 release at a height of its own:
  `PALW_RC_MODEL_SEED_V2_FENCE_DAA`. Every other preset keeps `palw_model_seed_v2 = None`.
  **6,900 is a flag day: every node must run a build carrying the fence before it.**
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2 (the least seed, 100,000 MSK) and [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)
  (the floor a pledge collects toward).
* Builds on: [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) (a market rule
  that changes what the fold writes arrives by activation, read through one resolved fence).

## 1. What is asked, and why it is a consensus rule

A line's market opens only once MSK is paid into the line's sink and locked there for good — the seed
(ADR-0090). ADR-0094 lets the seed arrive in instalments: every payment is collected in
`seed_pledged_sompi`, and the pair becomes a market the moment the collected total reaches the least
seed. The operator raises that least seed from 100,000 MSK to 1,000,000 MSK.

The floor is read by the fold (`model_seed_v1`): it decides whether a payment opens the pair
(`seed_v1` / `open_from_pledge_v1`) or is only collected (`pledge_v1`). That decision is written into
the market row, which is in the state root. Changing the constant in place would re-fold history
differently on every node that re-validates it — so, like ADR-0114, the new floor arrives at a height.

## 2. Decision

1. **`Params::palw_model_seed_v2`, a bare fence, read through `palw_model_seed_v2_fence`** — `Some` only
   where the market is armed too. Below it the floor is `PALW_MODEL_SEED_MIN_SOMPI_V1` (100,000 MSK),
   at and past it `PALW_MODEL_SEED_MIN_SOMPI_V2` (1,000,000 MSK); `palw_model_seed_min_sompi(active)` is
   the one spelling, and `Params::palw_model_seed_min_sompi_at(daa)` resolves it at a height.
2. **The fold reads the floor at the block's own DAA** through
   `PalwTransitionExtrasV1::model_seed_v2_active`, written explicitly by the virtual processor beside
   ADR-0114's leg. `false` by `Default`, so every caller that does not set it keeps the floor every
   existing row was opened under.
3. **Nothing already paid is lost or re-judged.** An open market stays open whatever the floor is now
   (`seed_remaining_sompi_under` owes nothing once `is_open`). A line whose pledges were short when the
   fence crossed keeps them — every sompi is already locked in its sink — and opens when the collected
   total reaches the new floor.
4. **The EVM window names the floor in force** (`constants()`'s third word, through
   `PalwEvmMarketFencesV1::seed_v2_active`). The writer keeps ADR-0094's rule — any non-zero seed is
   taken and the fold collects it — so no handler or address changes.
5. **The RPC serves the floor at the virtual's DAA** in `getPalwModelMarket`'s `seedMinSompi`, for an
   unseeded line as well (it answered 0 there before), so the CLI's `model-market`, the site and the
   Studio print what the fold will open the pair at.
6. **The fork id sees it.** 6,900 is a height no other fence uses: `fork_id_v1` digests fired heights,
   not fence sets, so a fence sharing 7,000 with held context and the audit's deep fixes would let a
   build without it peer through the gate and part silently there.

## 3. Consequences

* Opening a model's market costs ten times what it did; the seed is still wholly the reserve and still
  nobody's to withdraw. The curve starts at `seed / supply`, so the first price is ten times higher too.
* A line that has pledged between 100,000 and 1,000,000 MSK before 6,900 and has not opened by then
  waits for more pledges; nothing refunds it (a pledge was never refundable).
* The t11 fingerprint moves at the 7,000 release, which re-pins once for all of its fences.

## 4. Alternatives not taken

* **A policy floor in the tools only** (site, CLI, Studio refuse less than 1,000,000 MSK): the chain
  would still open any pair at 100,000 MSK paid by any other client, so the rule would be a suggestion.
* **Sharing 7,000**: one height for four fences, and a build missing one of them invisible to the gate
  (see Decision 6).
