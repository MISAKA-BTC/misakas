# ADR-0120 — The least seed is one million BILI, and it arrives at a height

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-12 at the operator's request ("misaka position のモデル追加の際に流動性として
  ロックして引き出せないようにする misaka の数を 1M として　今の 100000 枚から引き上げて"). **IMPLEMENTED
  the same day** on `feat/adr-0120-seed-min-1m`, and **scheduled on testnet-11 at DAA 6,900** by the
  operator the same day, to ship with the 7,000 release at a height of its own (unchanged on 2026-09-17,
  when the operator moved the 7,000 heights to 6,000: 6,900 now follows them):
  `PALW_RC_MODEL_SEED_V2_FENCE_DAA`. Every other preset keeps `palw_model_seed_v2 = None`.
  **6,900 is a flag day: every node must run a build carrying the fence before it.**
* Amends: [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2 (the least seed, 100,000 BILI) and [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)
  (the floor a pledge collects toward).
* Builds on: [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) (a market rule
  that changes what the fold writes arrives by activation, read through one resolved fence).

> **Amended (2026-10-01, implemented the same day behind the dormant fence `palw_model_virtual_v1`).** [0162](0162-the-pair-opens-on-a-virtual-reserve.md): past `palw_model_virtual_v1` the least seed is zero — no seed opens anything, every line's market opens at its creation on a virtual reserve — and `Params::palw_model_seed_min_sompi_at` answers 0 there whatever `palw_model_seed_v2` says.

## 1. What is asked, and why it is a consensus rule

A line's market opens only once BILI is paid into the line's sink and locked there for good — the seed
(ADR-0090). ADR-0094 lets the seed arrive in instalments: every payment is collected in
`seed_pledged_sompi`, and the pair becomes a market the moment the collected total reaches the least
seed. The operator raises that least seed from 100,000 BILI to 1,000,000 BILI.

The floor is read by the fold (`model_seed_v1`): it decides whether a payment opens the pair
(`seed_v1` / `open_from_pledge_v1`) or is only collected (`pledge_v1`). That decision is written into
the market row, which is in the state root. Changing the constant in place would re-fold history
differently on every node that re-validates it — so, like ADR-0114, the new floor arrives at a height.

## 2. Decision

1. **`Params::palw_model_seed_v2`, a bare fence, read through `palw_model_seed_v2_fence`** — `Some` only
   where the market is armed too. Below it the floor is `PALW_MODEL_SEED_MIN_SOMPI_V1` (100,000 BILI),
   at and past it `PALW_MODEL_SEED_MIN_SOMPI_V2` (1,000,000 BILI); `palw_model_seed_min_sompi(active)` is
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
   not fence sets, so a fence sharing 6,000 with held context and the audit's deep fixes would let a
   build without it peer through the gate and part silently there.

## 3. Consequences

* Opening a model's market costs ten times what it did; the seed is still wholly the reserve and still
  nobody's to withdraw. The curve starts at `seed / supply`, so the first price is ten times higher too.
* A line that has pledged between 100,000 and 1,000,000 BILI before 6,900 and has not opened by then
  waits for more pledges; nothing refunds it (a pledge was never refundable).
* The t11 fingerprint moves at the 7,000 release, which re-pins once for all of its fences.

## 4. Alternatives not taken

* **A policy floor in the tools only** (site, CLI, Studio refuse less than 1,000,000 BILI): the chain
  would still open any pair at 100,000 BILI paid by any other client, so the rule would be a suggestion.
* **Sharing 6,000**: one height for four fences, and a build missing one of them invisible to the gate
  (see Decision 6).
