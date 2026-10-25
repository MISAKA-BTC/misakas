# ADR-0061: Zero-seat genesis, and collateral sized by arithmetic instead of history

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted; implemented 2026-08-30 (`palw-genesis-10b-cap` branch, same re-mint train
  as ADR-0059/0060)
- Depends on: ADR-0060 (the heartbeat lane is what makes both decisions safe), the
  post-genesis bond carrier ("a stranger can register their own bond"), ADR-0059 (the 10B cap
  the collateral re-size returns money under)
- Supersedes: the born-licensable rule (`PanelCannotBeSeated`) of the RC genesis gate, and the
  0.1B-per-seat genesis collateral both ADR-0059 and ADR-0060 §8 carried forward

> **Amended (index reconciliation, 2026-09-02).** The audit amendment below ("the capability is
> real and its bootstrap is not yet") was answered by [ADR-0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
> and [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md): the heartbeat lane is back
> as `algo_id = 8`, armed from genesis on testnet-11 and devnet, and the Phase 1 drill proved the
> bootstrap this ADR draws — a zero-bond devnet born over heartbeats, a bond registered on it, the
> bonded lane starting after. "testnet-11 still ships its six seats" is Relaunch 3: since Relaunch 4
> the RC registry holds eight cards so that [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
> Decision 1 can be armed (`seat_count + 3`); devnet ships six public-seed bonds
> ([ADR-0075](0075-certification-is-a-consensus-object.md) §7's rehearsal chain). Decision 2's
> 10,000 BILI per seat stands. Map: [`README.md`](README.md).

## The two decisions

**1. A genesis may seat zero bonds.** `verify_palw_genesis_v2` no longer refuses a registry
smaller than `seat_count + 1` distinct operators — down to and including the empty registry.
The refusal's stated premise was "`BondRegistered` may not ride a transaction … a registry too
small has no later repair", and both halves are dead: bonds register on the running chain as
ordinary transactions, and the heartbeat lane (ADR-0060 D1) produces the blocks such a
registration rides even when no bonded producer exists. The bootstrap of a zero-seat network
is therefore fully permissionless:

```
genesis (0 bonds) → heartbeat blocks (bondless, fee-only)
                  → bond registrations ride them
                  → bonded producers light the PALW lanes
                  → sixth distinct operator arrives → licensing begins
```

**Audit amendment (2026-08-30, same day).** The bootstrap sentence above depends on ADR-0060
Decision 1, which the mainnet audit shipped OFF (see ADR-0060 §12). The GATE change stands — a
zero-seat registry is a valid genesis, and `an_empty_bond_registry_still_yields_a_consensus_v2_network`
pins that it stays a ConsensusV2 network rather than silently degrading to a hash chain, which is
a defect this ADR introduced and the audit caught. What waits is the first BLOCK: until the
heartbeat lane is redesigned, a zero-seat network has no permissionless producer, so the
capability is real and its bootstrap is not yet.

Until the sixth operator, claims void at `BindTimeout` and their escrow burns — LOUDLY (the
runtime warns per voided block), which is what distinguishes today's bootstrap phase from the
silent-forever failure the old gate was written against. The gate keeps every other check:
collateral coverage (C-08), the bind-window sustain rule, the catalog root, the class list.

**2. Genesis collateral is 10,000 BILI per seat** (was 0.1B — the old vault denomination, kept
"because it was there"). The binding constraint is the DERIVED requirement —
`palw_v2_collateral_for_claim_lifetime_v1` over the dearest registered class, measured at
**3,223.07 BILI** on the shipped three-class card — and the C-08 gate only demands the output
COVER the declaration. 10,000 is a ~3.1× margin over that structural minimum; the margin
absorbs DAA advancing slower than one per block (parallel production against one bond), while
the derivation itself already covers the whole claim-lifetime exposure horizon, `+1` included.

Consequences of the re-size, under the ADR-0059 cap arithmetic (the main wallet pays for every
carve, so the cap never moves):

| | before | after |
|---|---:|---:|
| collateral per seat | 100,000,000 BILI | 10,000 BILI |
| locked across 6 seats | 600,000,000 BILI | 60,000 BILI |
| t11 main wallet (spendable) | 8,852,999,400 BILI | 9,452,939,400 BILI |
| genesis total | 10B exactly | 10B exactly |

A slash of one seat now burns at most 10,000 BILI of operator money instead of 100M — the
penalty finally matches the protocol's own accounting instead of a historical denomination.

## What deliberately does not move

* **The declared collateral** in every `BondRegistered` (the derived 3,223.07 BILI) — so
  `palw_ruleset_id` is byte-identical. What moves is the genesis UTXO set, its commitment, the
  t11 genesis hash and the t11 fingerprint (`17bdff18…`), all riding the re-mint already in
  progress. No other preset's fingerprint moves.
* **The seating arithmetic.** `derive_panel_v2` still needs `seat_count + 1` distinct
  operators to license; zero-seat changes when they arrive, not how many are needed.
* **The shipped t11 card.** testnet-11 still ships its six seats — zero-seat is a capability
  (the mint tool now assembles any registry size, including empty), exercised by the next
  network that wants it, mainnet included.
* **No version bump.** The gate change moves no consensus bytes on any running network: two
  builds on a seated network behave identically, and an old build meeting a zero-seat genesis
  refuses to assemble it at boot — loud, fail-safe, and impossible to mistake for a fork.

## Why this closes ADR-0060 §8

Both items were listed there as "deliberately not decided", pending exactly the operator
decision this ADR records. With them decided, a genesis is at last only what a genesis must
be: the supply (one 10B main wallet under ADR-0059), the community's allocations, and — where
the operator wants a running start — a registry it could equally have grown on-chain.


## Audit note on the collateral figure

The 10,000 BILI carve did not, as first written, buy a ≈3× runtime margin: every exposure ceiling
reads the DECLARED collateral, and that was pinned to the derived structural minimum, so the
surplus in the outpoint bought exactly one extra concurrent claim. The declaration is now
`max(derived, held)` and the margin is real. Two consequences follow and are deliberate: the
derived minimum itself rose (the redraw's extra bind+receipt pair belongs in
`MAX_CLAIM_EXPOSURE_DAA`), and the carve is now a CEILING on the dearest class a genesis may
register — 10,000 BILI admits `pwu_per_inference ≤ ~8.3M` against Qwen3.6's 2.69M, a 3.1× headroom,
and exceeding it aborts every binary inside `Params::from`. Raising the carve is a supply decision
under ADR-0059's cap.

## Mission alignment amendment — 2026-10-07

zero-seat genesisはpreset genesis seatsを置かない意味であり、固定Panelの検証を廃止するRFC15のPanel=0ではない。後者はRFC14の全gateとRFC15固有gateが成立するまで有効化しない。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
