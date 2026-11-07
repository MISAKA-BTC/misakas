# ADR-v2-0031 — Panel独立性・coverage・取得不能と不正の区別

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0006](../rfc-v2/0006-panels-and-probabilistic-verification.md)

## 1. この文書の役割

Panelの独立性・検出coverage・非応答を、客観的な不正裁定と混同しない。

## 2. 統合した判断の読み方

### 2.1

bondの成熟、producer／registrantからの除外、operator単位のdraw等を、それぞれのanchorとnetworkに結び付ける。[旧ADR-0065](../adr/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) [旧ADR-0147](../adr/0147-independence-is-drawn-not-declared.md)

### 2.2

coverageはsample数・不正位置・検査方式に依存する数値であり、署名者数だけを完全検証とみなさない。不正を見つけたseatの動作はその責任と対応させる。[旧ADR-0098](../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)

### 2.3

verifierの取得不能・沈黙・供給不足は、producerの計算不正とは別に終了／再抽選を扱う。非不正終了が採掘枠を早期回復させるかも別論点として残す。[旧ADR-0166](../adr/0166-verifier-unavailability-is-not-producer-fraud.md)

## 3. 維持する境界・後続改定

operator IDが別というだけで実質的な非共謀が証明されたとはしない。モデル未取得の扱いは[ADR-v2-0019](0019-model-tiers-and-annual-catalog.md)、未採択の追加条件は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)へ分ける。

## 4. 隣接する判断との分担

claimの抽選証拠は[ADR-v2-0032](0032-sampling-beacon-and-receipts.md)、不正発見後の裁定は[ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md](../adr/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) | L1–L448 | L21–L28 |
| [0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md](../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) | L1–L326 | L15–L37 |
| [0147-independence-is-drawn-not-declared.md](../adr/0147-independence-is-drawn-not-declared.md) | L1–L226 | L15–L18 |
| [0166-verifier-unavailability-is-not-producer-fraud.md](../adr/0166-verifier-unavailability-is-not-producer-fraud.md) | L1–L125 | L17–L21, L123–L123 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else"></a>

<details>
<summary>0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md — 原文 L1–L448</summary>

[原文を開く](../adr/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) · SHA-256: `f9d09d8c24d15d16c41783817a3d74487d85bfa2e66f602082b973f193192236`

<!-- ADR_SOURCE_BEGIN:0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md -->
~~~~~~~~markdown
# ADR-0065 — A bond must be earned, and a failure is not a verdict

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


*(Titled "…and a seat must be someone else" when drafted. D3 was withdrawn against the code — the
seat draw already dedups by operator, and "someone else" turned out to be uncheckable. The title now
names what survived.)*

Status: **D1, D2 and D4 LANDED (all dormant); D3 and D5 decided** (2026-08-30). **D1 made
ARMABLE 2026-08-31** — see the correction below. Closes the CRITICAL recorded in the two addenda to
`docs/palw-mainnet-audit-2026-08-30.md`. **Mainnet blocker.** Consensus-affecting, but **no
re-mint for the RULES**: both landed rules sit behind top-level `ForkActivation` fences, `None` on
every preset, so no shipped fingerprint moves and either can be armed by rolling deploy. The earlier
"fingerprint move and a re-mint on every network" in §Consequences was written before the placement
was worked out and is corrected there. **Growing the registry so D1 can be armed at all is a
separate change and it IS a re-mint** — testnet-11 Relaunch 4, below.

> **Arming changed after the Status line (index reconciliation, 2026-09-02).** "`None` on every
> preset" was the 2026-08-30/31 state. Decision 4 (`palw_unavailable_abstains`) is armed **from
> genesis** on testnet-11 since Relaunch 5 (`palw_rc_arm_phase1`, [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
> Phase 2); devnet and mainnet leave it `None`. Decision 1 (`palw_bond_maturity`) is armable — the RC
> registry holds eight cards — but is still `None` on every preset; arming it is the out-of-band
> window this ADR names. Decision 2a stays dormant. The "0.004 BILI" registration floor measured
> here is the post-genesis carrier's `min_collateral_sompi`; genesis seats are 10,000 BILI
> ([ADR-0061](0061-zero-seat-genesis-and-right-sized-collateral.md)). Map: [`README.md`](README.md).

## The single root

Three facts about a bond, all verified in the tree:

* registration gates on `min_collateral_sompi` alone — **400,000 sompi (0.004 BILI)**, refundable;
* **`write_bond(key, None)` has no callers** — a bond never leaves the registry, retired or not;
* **`registered_daa` is written and read by no consensus gate anywhere** — no maturity, no soak.

So a bond is cheap, permanent and instantly usable. Everything below is that one fact, seen from
two sides.

**Safety side.** A holder of one bond can fork from any point at or after registering it, fold
sybil `BondRegistered` objects into the fork's own blocks, seat panels from them, self-license and
grow `safe_frontier` privately. `palw_fork_choice`'s stated invariant — *a fork nobody could see
collects no receipts, so it has no frontier* — is already false.

**Liveness side, measured on testnet-11.** The shipped panel is `seat_count = 5, quorum = 3`, and
**one host runs three seats** — exactly quorum. 443 of ~1,265 claims (35 %) ended in
`ProducerDefaulted`, each voiding ≈2,756 BILI.

That host holds three seats *legitimately*: three bonds, three distinct `operator_id`s, three
distinct bond keys. The draw is working as designed (see D3). Cheap, permanent, instantly-usable
bonds plus a free identity namespace mean **quorum is something one party can simply hold**.

## What the live chain taught that the code review could not

`Unavailable` **is not an abstention — it is a conviction.** `palw_panel_v2`'s own header: *the two
quorums license OPPOSITE transitions.* A `Valid` quorum licenses `ReceiptLicensed`; an
`Unavailable` quorum licenses `ProducerDefaulted`.

And availability fails **per claim, not per seat**: when a seat cannot obtain a claim's material,
every seat in the same situation fails on the same claims. The three seats on one host vote
`Unavailable` together, reach quorum unaided, and convict a producer that is serving correctly.

The mechanism was measured, and the first diagnosis was wrong in a way worth keeping. It looked like
a configuration gap — the producer loaded three class artifacts, host C's seats held two. That gap
was real and was closed; **the convictions continued** at ~30 % from every remote seat while the
producer's co-located panel stayed at 0 %. The cause is the **material transport**: a seat with no
material issues a gossip pull (`request_palw_material`, one per 25 DAA), waits half the receipt
window, and signs `Unavailable`. The co-located panel never needs the pull, which is exactly why it
never accuses. Roughly a third of pulls do not deliver, and **neither side logs a send, an answer, or
a timeout**.

So the conviction rate is a measurement of relay loss wearing a fraud verdict's clothes — and the
config fix not stopping it is the evidence that this is structural rather than operational.

**The general shape: a verifier's own missing dependency is submitted as evidence against the
accused.** Correlated failure plus a positive-conviction verdict plus co-located quorum is enough to
convict without a single dishonest actor — which is exactly the property a court must not have.

## Decisions

**D1 — Seat maturity. LANDED, dormant.** A bond may be drawn for a panel only if it was registered
at or before `anchor_daa - bond_maturity_daa`. This is the rule `registered_daa` was evidently
recorded for; the field already exists, so it adds no state.

Three things the implementation had to decide, none of which this ADR anticipated:

* **The clock is the claim's ANCHOR, not the binding block.** A panel is accepted only if it equals
  the derived one exactly, so the draw must stay a pure function of the claim. Resolving the fence
  at the binding block's DAA would make the derived panel change from block to block around the
  fence height, and a `PanelBound` that missed the block it was assembled for would be refused as a
  mismatch rather than accepted late. The subtraction lives in one function
  (`palw_seat_maturity_floor_v1`) that both the assembler and the acceptance layer call — two
  subtractions that could differ by one is a node proposing panels its own peers refuse.
* **The parameter is `Option<{ForkActivation, u64}>` at TOP LEVEL of `Params`, and the window is
  never visited by `for_each_fence`.** The bundle was the obvious home and is the wrong one:
  `palw_ruleset_id_v2` is a bare borsh hash over the whole bundle and `for_each_fence` never
  descends into it, so a bundle field moves `consensus_identity_id` and every old/new pair fails
  the handshake — a deploy-day partition on testnet-11, the only shipped preset carrying a bundle.
  And within the top-level field, only the activation may be visited: `consensus_identity_id`
  normalises every visited value to `0` or `u64::MAX`, so a visited *duration* would make two
  builds shipping different windows fingerprint identically, peer, and then seat different panels.
  That is `inactivity_leak_daa`'s recorded failure, and the window is hashed raw into
  `consensus_params_id` instead.
* **The liveness trap below is now refused, not documented.** See the Consequences entry.

**D2 — ~~Frontier provenance.~~ Unimplementable as written; RESTATED AND LANDED as a
comparison-site rule (dormant).**

The sentence was: *a `safe_frontier` advance requires receipts whose panels were drawn from bonds
mature relative to the fork point, not merely relative to the branch's own tip; without this, D1 is
defeated by rooting the fork later.* Two things are wrong with it.

**Wrong 1 — the rule cannot live where it was put.** `safe_frontier` is written in the pure fold
(`apply_palw_transition_v4` step 5, `palw_state_v2.rs:3670/3679`; the only other writer,
`:5557-5558`, replays the fold's own recorded pair when walking a chain path). The fold's entire
input is `(parent, params, admission, ctx, objects, own work, merged work)` — one chain, no second
branch, no ancestor. And the result is hashed into `state_root`, which the next block's header pins
and every node re-checks. So a frontier whose value depended on a fork point would depend on which
competing branch a node happens to hold, and two nodes would compute different roots for the same
block. **D2 as written is a chain split, not merely awkward.**

**Wrong 2 — the causal claim is false.** `registered_daa` is written from `ctx.daa_score` at the
fold and the comparison DAA is the branch's own; both are branch-local, so the fork point never
enters either side. D1 is not defeated by *rooting the fork later* — it is defeated by *extending
the fork by `bond_maturity_daa`*, which is work the attacker has to do. That is the cost D1 was
supposed to impose, and it does impose it.

**The restatement, in two parts, because they cover different attacks.**

* **D2a — a comparison-site rule, at the one site that has a fork point.** `dns_reorg_outcome`
  (`processor.rs:8784`) holds both the candidate and `prev_sink` in one consensus instance and the
  file already has an O(log) selected-chain ancestor finder it uses on the DNS path
  (`chain_common_ancestor_within`, `:8424`). A deep reorg may be refused when the challenger's
  frontier advance rests on bonds registered after the common ancestor. **This does not cover
  IBD**: the IBD commit site compares two independent consensus instances
  (`protocol/flows/src/ibd/flow.rs:1912-1913`) with no shared reachability, so there is no ancestor
  to compute. Say so in whatever ships it; a gate that silently covers one of two entry points is
  worse than one that admits its scope.
* **D2b — 新しい single-chain anchor は不採用。** append-only bond registry の ancestor との差分で登録 provenance を判定でき、追加 state や異なる score 単位の比較は不要なため。D2a の判定は維持する。

**What landed for D2a**, behind a top-level bare `Option<ForkActivation>` (`None` on every preset):
at the deep-reorg gate, when the challenger's registry holds bonds the ancestor's does not and a
panel that bound on the challenger's branch seated a quorum of them, the reorg is refused
(`FrontierProvenanceViolation`). Panels are read off the DELTAS rather than the challenger's tip,
because `retire_claim` deletes a claim and its panel while `safe_frontier` never retreats — so the
frontier can stand on a panel the tip no longer holds.

The fast path is a proof rather than a shortcut: a panel holds at most `minted` new seats, so a
quorum of them needs `minted >= quorum`, and the threshold used is the strictly smaller
`seat_count - quorum` that the majority invariant implies. It therefore scans sometimes when it
need not and never skips when it must, checked exhaustively over every legal panel shape.

**Two abstentions and one refusal** (amended, audit 2026-09-02). No fork point within the horizon,
and a missing delta or an unreachable side of the fork, pass with a warning rather than refusing: a
veto that cannot name a fork point is the permanent-partition shape the gate beside it already
learned to escape.

The third case was grouped with those two and is a different fact. `load_tip` returning `Err` means
this node's own root-verified snapshot did not decode — a disk that flipped a byte, or a row left by
another schema — and answering "carry on" there is a fork-choice gate failing open on exactly the
state that says it cannot judge. That is the `.ok().flatten()` shape ADR-0042 Decision 5 forbids
("reading absent data as nothing is forbidden"), and it arrived here because one `Option` spelled
"no tip yet" and "unreadable tip" the same way. **An unreadable snapshot now refuses the reorg**
(`FrontierProvenanceViolation`); `Ok(None)` — a V2 chain before its first tip — stays an
abstention, or a young chain could never reorg at all.

**And the threshold is derived, not configured.** A value that sits beside a fence is normalised
out of `consensus_identity_id`, so two builds scheduling one height with different thresholds would
peer and then disagree — the hazard D1 carries permanently. A bare fence has nothing to coordinate.

Note also that `PalwCandidateOrderV1` is four scalars (`palw_fork_choice.rs:38-53`), so any
provenance a comparison-site rule wants has to widen that seam or be computed beside it.

**And with D1 armed, most of what D2 was reaching for is already paid for.** A fork that mints its
own sybil bonds cannot seat them until the fork itself has advanced `bond_maturity_daa` — that is
the cost, and it is imposed by a single-chain rule with no fork point in it. What D2 leaves over is
the bond **pre-registered on the honest chain** and held as a standing, retroactive option: it is
mature at the fork point, so it is mature relative to it, and D2 *as written* would not have caught
it either. That residual is a **pricing** question (D6 and the collateral floor), not a provenance
one — which is worth saying plainly, because the two were run together in the original draft and
the provenance half is the expensive one.

**D3 — operator dedup の追加案は不採用。** one-seat-per-operator と one-key-per-bond は既に実装済みであり、自由に生成できる別鍵から独立した人間であることは証明できないため。D1 の maturity、collateral、D4 の abstention は維持する。

**D4 — `Unavailable` must be evidence of a refusal, not of a failure. LANDED, dormant.** A seat that
cannot evaluate a claim for reasons on its own side must **abstain**, not convict.

**artifact root の提示だけを possession proof とする案は不採用。** root は公開 field であり、モデルを持たない seat も同じ値を提示できるため。

**And no positive proof of refusal is constructible in this tree.** The material pull carries no
seat identity, nonce or signature; the serve carries no producer signature; the serve path returns
a silent `None` on four separate conditions including a node-global 10-second per-claim throttle;
and the producer broadcasts its material exactly once, immediately before submitting its block —
the "periodically while its claim is unresolved" in the flow context has no implementation. There
is nothing for a rule to check.

**So what landed is the weakening, and it is the honest one.** Past
`Params::palw_unavailable_abstains`, an `Unavailable` counts toward **neither** quorum and is
**never charged as a dissent** — the treatment `Incapable` already gets. The claim then falls to
the redraw-then-`ReceiptTimeout` path, which voids the escrow and slashes nobody. The verdict is
KEPT rather than deleted: a seat must still have a way to say it got nothing, and pushing those
seats into silence would trade a fact the chain can see for one it provably cannot.

D4 does **not** reduce to "route more cases into `Incapable`". `palw_seat_may_plead_incapable_v2`
refuses that plea for the base class — the class producing most testnet-11 claims — and the refusal
poisons the whole receipt set rather than dropping the one receipt.

Two consequences of the landed shape worth stating plainly:

* **`ProducerDefaulted` becomes unlicensable past the fence**, because the `Unavailable` quorum is
  its only constructor. The acceptance cross-check refuses it automatically once the tally can no
  longer return `ProducerUnavailable`, and the transition arm refuses it too rather than trusting
  that filter. A producer that genuinely withholds still gets nothing — no `Final`, no payout, its
  own escrow destroyed at `ReceiptTimeout` — so withholding remains self-punishing. What is lost is
  the *slash*, and provable withholding belongs in the DA court (ADR-0062), where the demand is on
  chain and absence cannot be forged by presence.
* **The fence is top-level and not in the bundle**, and here that is load-bearing rather than tidy:
  this rule has to reach a network that is convicting honest producers *now*, by rolling deploy. A
  bundle placement would refuse every old/new handshake instead of peering with a warning.

**D5 — The registry IS append-only, and that is now a decision.** Verified: `write_bond`'s `None`
branch has no callers anywhere — every call passes `Some`. It is deliberate and it must stay. A
retirement is a *status transition*, not a deletion (`Active → Retiring { since_daa }`), and the
record is the only thing that proves the withdrawal delay has elapsed
(`palw_bond_collateral_is_locked_v2` reads `since_daa`). Removing the row would make a retired bond
indistinguishable from one that never existed, which is exactly what the retirement path must not
do.

So "permanent" in the attack means the *registration* is permanent — not that a retired bond keeps
its powers. `palw_bond_may_take_work_v2` requires `Active`, so a `Retiring` bond takes no seats and
no work. The attacker's advantage is that it need never retire, and what prices that is D1 and the
collateral floor, not deletion.

**D6 — Re-pricing alone is not a remedy.** Raising `min_collateral_sompi` does not close this:
permanence (D5) and the absence of maturity (D1) are what make one purchase a standing, retroactive
option. Re-price if desired, but never in place of D1–D4.

## Consequences

* **No re-mint, and no shipped fingerprint moves.** Both landed rules are top-level
  `Option<ForkActivation>` fences left `None` on every preset, so an old build and a new one keep
  one `consensus_identity_id` and stay peers; arming either is a rolling deploy. The earlier
  "fingerprint move and a re-mint on every network" at the top of this section assumed a bundle
  placement, which turned out to be the thing that would have partitioned testnet-11 on deploy day.
* **`min_slash_permille_of_escrow` is NOT what makes a default cost the producer its bond — the
  earlier note here was wrong and the harm is live.** That parameter is read in exactly one place,
  admission item 9's collateral-backs-the-escrow check. The `ProducerDefaulted` arm calls
  `void_and_slash`, which is `void_claim` *followed by* `slash_bond(claim.bond, claim.reserved)`;
  its own comment says "this void takes the stake, unlike the two timeouts". So each of
  testnet-11's 443 defaults debited the producer's bond on top of voiding the escrow, and the
  licensing arm's `slash_dissenting_seats` charged every un-fed seat that filed `Unavailable` on a
  claim the panel went on to license. **Read that bond's `collateral` and `slashed` before
  anything else**: the debit is capped per event, so a long enough run drives it under
  `min_collateral_sompi`, after which `palw_bond_may_take_work_v2` stops it taking work at all —
  the chain refusing its own only producer for an offence it did not commit.
* **D1's liveness price is now refused rather than remembered.** `derive_panel_v2` fails closed on a
  short draw (`InsufficientEligibleBonds`), so a claim that cannot seat a panel never binds and
  voids at `BindTimeout`. The shipped genesis registers exactly `PALW_V2_PANEL_SEATS + 1` bonds and
  the draw excludes the executor — **zero slack** — and every genesis bond carries
  `registered_daa = genesis.daa_score`. A fence armed before its own window has elapsed therefore
  makes all of them immature simultaneously: no panel anywhere, `safe_frontier` pinned at 0,
  pruning never starting, every block's worker carve burned. This ADR's answer was "arm it after
  the network has bonds to spare", which is something an operator has to remember;
  `Params::validate_palw_v2` now refuses the configuration outright, and refuses a zero window
  besides, because a gate that cannot refuse anything is the failure this whole audit line keeps
  finding.
* **D4 is the one that stops present harm**, and unlike D1 and D2 it is not defeated by a free
  identity namespace. It should ship first.

### What an adversarial review of the landed code established, and none of it is fixable by editing D4

* **Past the fence, seat accountability is gone entirely.** `slash_dissenting_seats` has two call
  sites; the `ProducerDefaulted` one becomes unreachable and the licensing one skips `Withheld`, and
  `slash_silent_seats` was already a no-op. So no seat's receipt behaviour is punishable by
  anything. That is the honest position — this chain cannot observe silence, and it cannot tell a
  lost fetch from a refusal — but it should be stated rather than discovered: **a seat that files
  nothing, files `Unavailable` forever, or lies `Valid` pays the same, which is zero.** Seat
  accountability needs receipts that ride the chain independently of a concluding object; until one
  exists there is nothing to charge on.
* **The redraw is weaker than D4's fallback assumes, for a reason D4 cannot reach.** Two defects,
  one fixed here and one structural. Fixed: the seat service keyed its "already answered" set by
  CLAIM, so every seat that answered a first panel silently skipped the second — the redraw dealt
  new seats and then muted any that had sat before. Structural: `derive_panel_v2` re-randomises only
  the ticket ORDER (by `anchor_block`), so on a registry holding exactly `seat_count + 1` bonds —
  the shipped genesis — the second panel is the identical seat SET. **A redraw only helps a network
  with spare bonds.** With none, D4's fallback is simply the timeout, which is still the right
  outcome (escrow void, nobody slashed) but is not the second opinion the redraw was built for.
* **D1's arming guard is a genesis-only check and cannot see the live registry.** It proves the
  bonds that exist at genesis are mature when the fence fires; it cannot prove the same of a
  REPLACEMENT. On the zero-slack shipped registry, if one non-executor bond leaves eligibility — the
  operator retires it, or the escrow slash drives it under `min_collateral_sompi` — the replacement
  is unseatable for a whole `bond_maturity_daa`, and a short draw is no panel: every claim voids at
  `BindTimeout` and the frontier stops for the window's duration. Without D1 the same replacement
  restores panels in the next block. **So D1 must not be armed on a network running at
  `seat_count + 1` bonds.** This was written as an operational precondition the config check cannot
  enforce. It is now BOTH enforced and satisfied — see the correction below.
* **Arming D1 is a coordinated change, even though the handshake permits a rolling one.** Two
  operators who schedule it at the same height with different windows keep one
  `consensus_identity_id` and are told only by a warning. Deliberate — it is the standing rule for a
  value inert until its fence fires — but it means the window has to be agreed out of band.

## Two operational rules that fall out, worth keeping even after the code is fixed

1. **A conviction verdict must log its reason.** The panel files `Unavailable` with the claim id and
   nothing else, which is why a third of the chain's claims were being convicted unnoticed. A verdict
   against another party should never be cheaper to emit than to explain.
2. **No single host may hold quorum-many seats**, independent of any rule change.


## Correction, 2026-08-31 — the rule was enforceable and unarmable at the same time

The caveat above says D1 "must not be armed on a network running at `seat_count + 1` bonds" and
files that under things a config check cannot enforce. Both halves were wrong in the same direction.

**It IS enforceable.** `validate_palw_v2` reads the genesis registry — the objects are in `Params`
— so the count is knowable at boot, before a peer is dialed. The guard now refuses an armed
`palw_bond_maturity` on a bundle registering fewer than
`palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1()` bonds, and `ConfigBuilder::build` panics on
a failing validate. An operator cannot arm the halt by remembering wrong.

**And enforcing it exposed the real defect: nothing this build shipped could satisfy it.**
`PALW_RC_GENESIS_BONDS` held exactly `seat_count + 1 = 6` cards, so the guard refused every shipped
preset. D1 was a rule that could only be obeyed by minting a new genesis — enforcement existed and
arming did not, which is the "gate that never fires" shape this ADR line keeps finding, wearing the
opposite mask: a gate that fires on everything.

**The bar, derived rather than chosen.** `seat_count + 3`:

| term | why |
|---|---|
| `seat_count` | the panel itself |
| `+1` | the executor, excluded by bond, operator and key |
| `+1` | one seat departing — the case D1's window makes expensive, since the replacement is immature for a whole window |
| `+1` | margin, so the first departure does not leave the network one retirement from a halt |

The strict minimum is `seat_count + 2`; the third is slack, and it is named as slack in the code. An
earlier derivation reached the same number by counting "the replacement's own immaturity" as a
fourth term, which double-counts: the replacement's immaturity *is* the departure's gap.

**What changed.** `PALW_RC_GENESIS_BONDS` grew from six cards to eight, with two real ML-DSA-87
key pairs generated for the purpose. This is a genesis change — two collateral outputs and two fee
floats enter the premine, two `BondRegistered` objects enter `genesis_objects` — so the premine
commitment, both genesis hashes and the testnet-11 `consensus_params_id` all move together:

* testnet-11 genesis `d2789338…` → `572f80c0…`, `utxo_commitment` `670b1125…` → `7f3142f2…`
* `consensus_params_id` `f3bf86b4…` → `4f89ec82…` — and then to `5ccdd684…` when this merged with
  the free-prompt quantum drop (1,000 → 100) on the same day. Recorded because the intermediate
  value never shipped: a fingerprint is a function of the whole ruleset, so two re-mints landing
  together produce a third figure rather than either of theirs.
* payload marker `11,3` → `11,4` ("Relaunch 4")

**testnet-11 must be re-minted and every host must wipe its datadir.** Nothing in this build accepts
the Relaunch-3 chain; an un-wiped node hits the startup genesis-mismatch guard rather than silently
resuming, which is the intended behaviour and the reason the marker is bumped.

**Two tests hold the claim**, because "armable" has two halves and the first alone is the failure
mode this ADR keeps cataloguing:

* `arming_bond_maturity_needs_a_registry_with_a_spare_seat` — the config gate accepts the fence on
  the shipped preset, refuses it on the same preset trimmed to `seat_count + 1`, and still refuses
  it one card short of the bar.
* `the_shipped_registry_draws_under_an_armed_maturity_fence_even_after_a_seat_leaves` — the shipped
  genesis objects are applied through the real transition, a claim is bound under a real card, and
  the panel draws with the floor on; then a `BondRetireRequested` removes a seat and it still draws.
  The same departure on a `seat_count + 1` registry returns `InsufficientEligibleBonds`. A fence
  that validates and then starves every draw would pass the first test and fail this one.

**What is still not enforced, stated plainly.** The guard reads the GENESIS registry, and that is
a real limitation rather than a theoretical one: a `BondRegistered` carrier IS admitted on a live
V2 chain once it locks the collateral it declares (`virtual_processor/processor.rs:4922`), so live
registries genuinely do grow. A network that grew past the bar post-genesis still cannot arm D1 if
its genesis was small; a network whose genesis was large can arm it after the live registry has
shrunk below the bar. `validate_palw_v2` has no chain access, so this is the limit of a config-time
check, not an oversight — but it means the guard is conservative in one direction and stale in the
other. Both failures land on the same runtime behaviour, which fails closed: a short draw is
`InsufficientEligibleBonds`, no panel, and the claim voids at `BindTimeout`. **The reason to ship
the registry at `seat_count + 3` anyway is that the genesis size is the only one the config gate
can see, and a halted chain produces no blocks — so no repair carrier can land on the network that
most needs one.**

**Mitigated, 2026-08-31: the stall now says what it is.** The gap above cannot be closed by a
check — `validate_palw_v2` has no chain access, and a runtime refusal would be a consensus rule
resolved from local state, which is a chain split. What was fixable is that the failure was
*silent*. When the live registry falls under the bar, `derive_panel_v2_with_maturity` returns
`InsufficientEligibleBonds`, `palw_v2_derived_panel_bindings` skips the claim with a bare
`else { continue }`, every claim voids at `BindTimeout` and `safe_frontier` stops — and no line
anywhere named the maturity fence. From the outside that is indistinguishable from a producer
outage, which is the wrong thing to go and investigate.

`VirtualStateProcessor::palw_warn_if_maturity_outruns_the_registry` now reports it. **Log only** —
no refusal, no fence, no return value, no fingerprint movement — with two severities, because the
two situations need different responses:

* **below `seat_count`** — no claim can seat a panel, whoever its executor is;
* **at exactly `seat_count`** — every claim from a still-eligible bond fails, and only a claim whose
  own bond has itself left eligibility can still bind;
* **below `palw_v2_maturity_armable_bonds_v1()`** — draws still work but the margin is gone, and the
  next retirement or slash stalls the chain for a full maturity window.

Three bands rather than two, because exactly three are provable and the middle one is where a
plausible message goes wrong. The draw's three exclusions collapse to one: `pubkey` uniqueness is
enforced at registration (`palw_state_v2.rs`, the `DuplicateBondKey` arm), so the key clause can
only match the executor's own bond, which the bond clause already matched. The exclusions therefore
remove exactly ONE operator when the executor's operator is among the eligible, and ZERO when it is
not — and it is not whenever the executor's own bond is immature, `Retiring`, or under the
collateral floor, each of which a claim can enter *after* it was created (claim creation checks
`Retiring` at admission and never re-checks, and imposes no maturity requirement at all).

So at exactly `seat_count` a panel is still drawable for such a claim, and an earlier draft of this
warning said "no panel can be drawn" over a state where one demonstrably was.
`a_claim_whose_own_operator_has_left_can_still_seat_a_panel` measures both edges of that band rather
than arguing them. The alarm threshold did not move — `seat_count + 1` is still the point where
claims from healthy bonds start failing — but a diagnostic may not claim more than its count knows.

The count comes from `palw_panel_v2::palw_seatable_operators_v1`, which applies **the same two
predicates the draw does** (`palw_bond_may_take_work_v2` and the maturity comparison) rather than
restating them — a diagnostic that could disagree with the rule it describes is the failure this
ADR line keeps cataloguing, one layer out.
`the_seatable_counter_agrees_with_the_draw_it_warns_about` holds the two together across every
maturity floor and panel size, so the counter cannot quietly stop matching. It counts distinct
OPERATORS, because the draw seats one bond per operator, and it deliberately does not apply the
draw's executor exclusions: the bar it is compared against already prices the executor as one of
its four terms, so excluding it twice would fire the warning a seat early.

It is rate-limited to once per `window_bind` — the interval over which a claim actually times out,
so the operator hears once per cohort of claims the shortfall costs rather than once per block —
and it returns before touching the header store or the registry whenever the fence is dormant,
which is every shipped preset.

## Mission alignment amendment — 2026-10-07

maturityと公開bond条件はSybil/DoS対策として維持できるが、独立operator名や複数bondの存在をhonest-majority証明にしない。Unavailableやproducer defaultの旧問題は客観的な証拠・期限で区別する。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md -->

</details>

<a id="source-0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else"></a>

<details>
<summary>0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md — 原文 L1–L326</summary>

[原文を開く](../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) · SHA-256: `0c581d5fd5a5f4410b24213989c82255f51cf02507369e4662e5310610adb322`

<!-- ADR_SOURCE_BEGIN:0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md -->
~~~~~~~~markdown
# ADR-0098 — The panel's coverage is a number, and a seat that found a lie files nothing else

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-10. **Decisions 1–3 IMPLEMENTED the same day** (§9): Decision 1 is a
  report over the draw the seats run; Decisions 2 and 3 change what a seat FILES, which is the
  seat's own duty — no consensus object, acceptance rule, fence, parameter or fingerprint moves,
  and a fleet takes them by an ordinary rolling rebuild. Decisions 4–6 are stated and not built.
* Builds on: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decision 8
  (a seat replays `k` intervals drawn from the beacon; "a row unequal: the seat files nothing and
  opens a court at that leaf"; a sampled verdict convicts nobody),
  [0082](0082-the-close-is-flat-in-the-context.md) Decision 9 (the seat recomputes the state it
  resumes from), [0085](0085-the-close-is-assembled-from-what-was-served.md) Decision 4 (a seat's
  fault is prosecuted by the challenger's half), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  Decision 6 (a leaf once named), ADR-0065 Decision 4 (`Unavailable` abstains),
  [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (a receipt is 4,772 bytes on the
  wire), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) Decision 4,
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the missing
  responder), [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  Decision 5 (whose first row — a seat that holds a shard, measured first — this is).
* Answers: ADR-0081 Decision 8's question and its U-03 — "what fraction of a job's segments the
  panel collectively covers per claim and what a producer's expected cost is of a single forged
  link" — which ADR-0082 withdrew with ADR-0081 §3 before anyone took the number. The question was
  never about segments; it is about ADR-0077 Decision 8's live draw, and it is answered there.
* Amends nothing in consensus. **Restores** ADR-0077 Decision 8's "the seat files nothing" in the
  node, where three paths had lost it (§1.3).
* Supersedes nothing.

## 0. The sentence this ADR is

**On the 300-token claim testnet-11 licensed on 2026-09-05, the panel's sampling catches a
one-token lie 6.51 % of the time with five replaying seats and 3.96 % with three; a producer can
file 35 such claims before the odds that one is caught reach 90 %. And until this ADR, a seat
that did catch one could go on to certify the claim, or to accuse its producer of withholding the
data it had just replayed.** The number is now measured with the draw the seats run. A seat that
finds a lie files nothing else about the claim, and every fault it proves is recorded for the
court. A network whose seats cannot hold the model (ADR-0097) learns what the number costs it. A
panel stratified by shard keeps the coverage and multiplies the seats. Its licensing object fits
one transaction only up to eight shards. And it still needs a court that a seat holding one shard
can open.

## 1. What was measured

Read on `feat/adr-0097-model-fit` at `65ee18cd` (itself on `feat/adr-0096-everyday-lane`'s
`421cc92e`) on 2026-09-10. Every figure is what `misaka-palw-base0 --bin palw-seat-coverage`
printed on that tree (recorded in `docs/palw-seat-coverage-2026-09-10.md`); the binary is the
authority (ADR-0092 §5).

### 1.1 The draw, as it ships

* A free-prompt seat draws **`k = 4`** intervals (`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`,
  `palw_fp_interval_v1.rs`) with `palw_fp_interval_draw_v1(network, beacon, claim, seat_index, k,
  N)`: distinct within a seat, a keyed BLAKE2b over the four inputs reduced `u64 % N`, the seat
  index keyed in so seats draw independently. The beacon is the panel's anchor — the block at
  `bind_base_daa + anchor_delay`, which did not exist when the claim was committed.
* **`N`** is `max(1, ⌈(D − 1) / checkpoint_interval⌉)` for a claim of `D` decode tokens
  (`base0_fp_interval_count_for_v1`). The dense A16 and BASE-0 families checkpoint every decode
  call, so **`N = max(1, D − 1)`**; the hybrid checkpoints at `n_ctx`, so its claims have
  **`N = 1`** and every seat replays the whole job.
* The panel is **five seats with a quorum of three** (`PALW_V2_PANEL_SEATS` / `PALW_V2_PANEL_QUORUM`,
  both presets). The licensing object is carried as soon as three `Valid` receipts pool, and a
  seat's duty exists only while the claim is `PanelBound` — so **between three and five seats
  replay a claim before it is licensed**. Every table gives both.
* Before replaying interval `j` a seat recomputes the state that interval resumes from — the cache
  over every earlier position, from the prompt and the committed answer — and compares its root
  with the committed one (ADR-0082 Decision 9). So a lie that alters the cache is caught by a
  seat that draws ANY interval at or after it. A lie the cache never records — the selected token,
  or the last layer's work after its K/V rows — is caught only by the row replay of its own
  interval. That lie is also the one that changes the answer, so it is the forger's best, and its
  number is the floor under every other.

### 1.2 The numbers

**The forger's best lie** — one token, found only by a seat that draws its interval. Closed form
`1 − ((N − k)/N)^s`, and the draw itself over 5,000 beacons, which agrees on the mean and on every
interval within six binomial deviations (`palw_seat_coverage_v1`'s tests):

| D | N | s = 3 | s = 5 | forged claims before 90 % odds of one catch (s = 5) |
|---|---|---|---|---|
| 5 | 4 | 100 % | 100 % | 1 |
| 9 | 8 | 87.50 % | 96.88 % | 1 |
| 17 | 16 | 57.81 % | 76.27 % | 2 |
| 65 | 64 | 17.60 % | 27.58 % | 8 |
| 256 | 255 | 4.63 % | 7.60 % | 30 |
| **300** | **299** | **3.96 %** | **6.51 %** | **35** |
| 512 | 511 | 2.33 % | 3.85 % | 59 |
| 1024 | 1023 | 1.17 % | 1.94 % | 118 |

**A lie the cache records**, by where it sits (N = 299, s = 5): 100 % up to interval 149, 99.70 %
at 224, 88.07 % at 269, 49.53 % at 289, and 6.51 % at the last interval, where it is the row
replay's number. The forger's best cache-altering lie is therefore at the end of the answer.

**What a higher number costs** — intervals each seat must replay:

| N | s | 50 % | 90 % | 99 % |
|---|---|---|---|---|
| 299 | 5 | 39 | 111 | 180 |
| 299 | 3 | 62 | 161 | 235 |
| 1023 | 5 | 133 | 378 | 616 |

Ninety percent on the 300-token claim needs each of five seats to replay 111 of its 299
intervals — 37 % of the job each, plus the recompute to its latest draw. Sampling buys a weak
per-claim catch cheaply; a strong one costs most of an inference per seat.

**The capture arm's leaf sample.** A seat checking a served capture opens leaf 0 and three leaves
drawn uniformly (`fp_capture_samples_clear`). On the dense graph-v5 row that is a 1 in 565,750
chance per seat of landing on one forged leaf for an 8/8 job, and 1 in 13,919,200 for 51/300.

**A panel of seats that each hold a shard** (§3 Decision 5): stratified by shard, with a quorum of
three per shard, a one-leaf lie is seen only by its own shard's seats, so the coverage is the
s = 5 column at every shard count. The licensing object carries `3 × shards` receipts and fits one
standard transaction (120,000 bytes) up to **eight shards** — 114,597 bytes at eight, 128,913 at
nine. For the Kimi K3 stand-in of ADR-0097 (2.8 T parameters, one byte a weight as a floor) eight
shards is 326 GiB a seat.

### 1.3 What the measurement found in the seat

The question "does a seat that finds the lie make it count" had to be answered before the number
meant anything, and the code said no, in three ways.

1. **A seat that found a fault went on.** `interval_seat_outcome_v1` records a `Fault` /
   `FaultInRange` (`note_seat_fault_v1`) and returns `None`, and the verdict block fell through,
   in the same round, to the whole-capture arms. There the FPC1 arm could file **`Valid`**: its
   leaf sample misses the lie with near certainty (above). Otherwise the half-window tail filed
   **`Unavailable`**, a signed accusation of withholding against a producer that had served the
   data the seat just replayed. On a ruleset without ADR-0065 Decision 4 — the shipped devnet
   (`DEVNET_PARAMS`; `palw_rc_arm_phase1` arms it for testnet-11 and the mainnet card only) —
   `slash_dissenting_seats` charges that `Unavailable` `claim.reserved` when the other seats
   license the forged claim (`a_seat_that_reports_against_the_quorum_pays_what_it_tried_to_take`).
   The honest finder paid; the forger's claim was licensed. The seat also re-replayed the claim
   every round until its deadline.
2. **A state root that does not recompute was logged and dropped.** On a checkpoint mismatch the
   interval arm returned `None` without recording anything, so the challenger's half never
   prosecuted it. By §1.1 this is the path that catches every cache-altering lie before the drawn
   interval, which makes it the seat's strongest detector.
3. **A leaf of the claim's own capture that does not recompute was a `false`.** The capture arm's
   sampler returned the same `false` for "proved a fault" as for "could not check", so a proven
   fault was dropped and the seat drifted to the tail.

Two facts decide the fix and one more decides the design:

* **Silence is never charged.** `slash_silent_seats` has been a no-op since audit M2-7, because the
  chain cannot observe silence. So "files nothing" costs the finder nothing on any ruleset.
* **A seat whose interval openings have not arrived also falls through** to the capture arm in the
  same round. For a claim whose capture fits the 16 MiB material cap, the four-leaf check can then
  be the seat's whole verification, before its interval replay ever runs. This is not a fault
  once found, and closing it trades liveness; see Decision 6.
* **Every conviction needs a party that can run the whole job.** The challenger's half re-executes
  the claim (`execute_free_prompt` / `execute`), and only if it does not reproduce does it open a
  court. It opens that court over the ruleset's whole `StepLeaves` space, and answers every rung
  of the ladder from its own execution. A seat can NAME the leaf (ADR-0086 Decision 6) but cannot
  open a court at it. For a network whose seats hold shards, nobody could convict.

## 2. The requirement

> **R-cov — the panel's coverage of one claim is a stated number, measured with the draw the seats
> run; a seat that finds a lie files nothing else about that claim and hands it to the court; and
> a network whose seats hold shards knows, before it mints, what the number costs it.**

## 3. Decisions

**Decision 1 — the coverage is a number, and it is generated.** `palw_seat_coverage_v1`
(consensus-core, a report) holds the closed form for the row replay and for the cache-altering
lie, the measurement that runs `palw_fp_interval_draw_v1` itself, the inverse questions (the draw
or the panel a target needs), the capture arm's sample as the same arithmetic, and the stratified
panel's cost. `palw-seat-coverage` prints §1.2. Two things are said with the number every time,
because either alone misleads:

* **It is the half that costs a seat `k` intervals.** ADR-0077 Decision 8 is kept: a sampled
  verdict convicts nobody, and a claim stays disputable for its whole challenge window. Where a
  bonded watchdog (`--palw-challenge`) re-runs every licensed claim, a forged leaf is found with
  certainty, at one inference per claim.
* **It is the forger's best lie.** Any lie that alters the cache is caught with a higher
  probability, as §1.2 shows by position.

**Decision 2 — a seat that found a fault in a claim files nothing else about it.** The fault
ledger (`PalwSeatFaultLedgerV1`) is consulted at three points of the verdict block: before its
first arm, between the free-prompt interval arm and the capture arm, and before the half-window
tail. A claim in the ledger gets no receipt from that seat, in that round or any later one — not
a `Valid`, not an `Unavailable` — and it is not replayed again. The fault reaches the court
through the challenger's half, which prosecutes every claim in the ledger (ADR-0085 Decision 4).
This is ADR-0077 Decision 8's own sentence, put back where it had been lost.

**Decision 3 — every fault a seat proves is recorded.** A checkpoint state root that does not
recompute is recorded as a fault with no leaf address, `(0, 0)`, and the court's bisection finds
the leaf. The capture arm's sampler answers `Cleared`, `NotCleared` or `FaultAt(leaf)`, and the
last is recorded at its leaf. The ledger keeps the most specific address it is given: a named
leaf over a block over an unaddressed fault. A later, vaguer note never erases a better one.

**Decision 4 — the expected cost of a lie is the number times the claim's reservation.** A
conviction slashes exactly `claim.reserved` (`void_and_slash`). So a producer's expected cost for
one forged leaf, when detection leads to a conviction, is the coverage times `claim.reserved`:
6.51 % of it on the 300-token claim with five replaying seats. The chain deters a false answer
worth less than that. It does not deter one worth more, except through the watchdog. The coverage
rule a network wants — a draw `k`, a panel size — is a mint-time choice priced by §1.2's second
table. This ADR prices it and does not choose it: a coverage rule chosen before the measurement
was ADR-0081's warning, and one chosen by this ADR would be a guess with a table beside it.

**Decision 5 — what a network whose seats hold shards must have, named.** Not built, and each item
is measured before it is designed:

* **A stratified panel.** Seats are drawn per shard, `r` per shard, with a quorum per shard. The
  coverage of a one-leaf lie is then independent of the shard count (§1.2), so today's number is
  kept at `r = 5`. The panel is `5 × shards` seats, and its licensing object carries `3 × shards`
  receipts. That fits one standard transaction up to eight shards. Past eight, the licensing
  object must be split across transactions or aggregated. ML-DSA-87 signatures do not aggregate,
  so this is a design question with its own measurement.
* **A court a shard can open.** Opened at a leaf a seat NAMES, and adjudicated at the terminal step
  from what the seat already holds: its interval opening (the committed rows and the state the
  interval resumes from, bound to the claim's roots) and the artifact opening of that one node's
  weights. The terminal step never needed the whole model; only the ladder above it did, because a
  challenger answers each rung from its own whole execution. This court is a consensus object
  behind its own fence, and the sibling of ADR-0093's responder. Its first measurement is the
  close for one node of the target model at a ladder that holds it; ADR-0097's generator already
  prices it.
* **The seat's replay budget.** A seat's work is `k` interval replays plus the recompute to its
  latest draw, on its shard. It must fit `window_receipt` at the certification drill's measured
  rate (ADR-0082 Decision 9), which no model of this size has yet been measured at.

**Decision 6 — the pending fall-through is the operator's.** Closing it means a seat whose interval
openings are pending never certifies through the capture arm. That would make the interval draw
the seat's only check on small-capture claims, and a producer that serves captures but not interval
openings would get no receipts from seats that follow the rule. It is named, with its number (§1.2's
capture-arm row), and left open.

## 4. What this costs

* **Chain:** nothing. No object, field, fence or fingerprint.
* **A seat:** on a claim it found a lie in, it stops replaying and stops filing. On an honest claim
  nothing changes. On a forged claim the licensing quorum loses one seat, and the other four can
  still reach three. That is the point: licensing is not conviction, and the court decides.
* **devnet:** an honest finder is no longer charged `claim.reserved` for the `Unavailable` it
  should never have filed.

## 5. Invariants the tests hold

```
1  The closed form is the exact rational rounded down, never up (a floor).
2  The draw the seats run, over deterministic beacons, agrees with the closed form on the mean
   and on every interval within six binomial deviations — uniformity is what a forger choosing
   where to lie would exploit, and it is measured, not assumed.
3  A cache-altering lie is caught through any later draw; at the last interval its number is the
   row replay's; before it, never less.
4  A job no longer than the draw is replayed whole by every seat (the hybrid's N = 1).
5  A stratified panel's coverage does not depend on the shard count, and its licensing object
   fits one standard transaction for at most eight shards (pinned as the limitation).
6  The fault ledger: a fault once found stays found; a named leaf outranks its block; an
   unaddressed fault never erases an address; past its bound a NEW claim is refused.
7  The verdict block consults the ledger at its three points, the capture arm records the fault
   it proves, and the root that does not recompute is recorded — a source pin whose negative
   control (one gate removed) was run and observed red.
```

1–5 are `consensus/core/src/palw_seat_coverage_v1.rs`'s tests; 6 is `kaspad/src/palw_fp_seat.rs`'s
`a_fault_once_found_stays_found_and_the_most_specific_address_wins`; 7 is `kaspad/src/palw_panel.rs`'s
`a_seat_that_found_a_fault_consults_the_ledger_before_every_verdict`.

## 6. Order of work

1. Decision 1 — **done** (§9).
2. Decisions 2 and 3 — **done** (§9).
3. The fleet takes Decisions 2 and 3 by a rolling rebuild; nothing is scheduled. That is the
   operator's.
4. Decision 5's court a shard can open — its own ADR, measured first.
5. Decision 5's licensing past eight shards — its own ADR, measured first.
6. Decision 6 — the operator's.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0077 Decision 8 — "a row unequal: the seat files nothing" | kept, and restored in the code (Decisions 2–3) |
| ADR-0077 Decision 8 — "a sampled verdict convicts nobody" | kept, and priced: it is why Decision 1 states what the number is and what it is not |
| ADR-0081 Decision 8 / U-03 — the coverage number | answered for the live draw (Decision 1); the withdrawal of ADR-0081 §3 stands |
| ADR-0085 Decision 4 — the challenger's half prosecutes a seat's fault | kept; it now sees the state-root and capture-arm faults too (Decision 3) |
| ADR-0065 Decision 4 — `Unavailable` abstains | unchanged; this ADR removes the case where an honest finder needed it |
| ADR-0097 Decision 5, first row — a seat that holds a shard | its measurement (§1.2) and its two missing pieces (Decision 5) |

## 8. What is deliberately not decided

* **The draw `k` and the panel size a network wants** (Decision 4) — mint-time, priced by §1.2.
* **Whether seats should be paid.** A seat is never paid today, as the doc on `slash_silent_seats`
  says. The number above assumes every drawn seat replays. The economics of whether one does
  belong to ADR-0033's successor, not here.
* **The pending fall-through** (Decision 6).
* **The shard court's object, its fence, and its close** (Decision 5).

## 9. Number hygiene and implementation record

0098 is the next free number after ADR-0097 (whose README row says so). It is claimed on
`feat/adr-0098-seat-coverage`, branched from `feat/adr-0097-model-fit` at `65ee18cd`. A concurrent
claimant renumbers the later writer. **The next free number is 0099.**

* **2026-09-10** — ADR written and implemented the same day:
  * `consensus/core/src/palw_seat_coverage_v1.rs` — Decision 1, with seven tests.
  * `misaka/palw/base0/src/bin/palw-seat-coverage.rs` — the generator, classified in the
    float-free scan as a measurement tool.
  * `docs/palw-seat-coverage-2026-09-10.md` — the generator's output, as a dated record and not
    as a source.
  * `kaspad/src/palw_fp_seat.rs` — the fault ledger, with its test.
  * `kaspad/src/palw_panel.rs` — Decisions 2 and 3: the three gates, the recorded state-root
    fault, the sampler's three answers, and the source pin.

  kaspad's whole suite ran green, as did the targeted consensus-core and base0 suites. The
  mainnet card and testnet-11 behave as before except that a finder files nothing. On devnet the
  finder is no longer charged.

## Mission alignment amendment — 2026-10-07

seatが嘘を発見したらそのclaimについてValidを出さない原則とsampling実測は保持する。旧「files nothing / fault ledgerだけ」の動作を将来の上限にしない。非seatもfaultを局所化し、RFC14の認証demand・bounded court・terminal proofを提出できる設計へ進める。発見済みのfault ledgerだけをconviction完了と呼ばない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md -->

</details>

<a id="source-0147-independence-is-drawn-not-declared"></a>

<details>
<summary>0147-independence-is-drawn-not-declared.md — 原文 L1–L226</summary>

[原文を開く](../adr/0147-independence-is-drawn-not-declared.md) · SHA-256: `045369886958bf9a983b1603f6dd4884d738b9b00b858d9318ca89cd76d9177f`

<!-- ADR_SOURCE_BEGIN:0147-independence-is-drawn-not-declared.md -->
~~~~~~~~markdown
# ADR-0147 — Independence is drawn, not declared

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** IMPLEMENTED behind `Params::palw_admission_independence`, which is one of the three
fences of the ADR-0145 economic bundle (`validate_palw_v2` arms them at one height or not at all).
Dormant on every preset. Supersedes the identity test that shipped under the same fence on
2026-09-19 (`palw_bond_is_independent_of_registrant_v1`), which is deleted.

This is the admission half of ADR-0145 (I3, and §7's "independent admission") and the 2026-09-19
reward audit's F3: *a class can be judged entirely by its own registrant.*

---

## 1. Why the first repair was not a repair

A claim's panel is drawn from the bonds that may judge its class: under the registry, those with a
fresh possession proof of the class's artifact; below it, those that declared capability for it.
Both are acts a registrant performs for itself. For a model nobody else runs, the class's own
population is the registrant's fleet, and the panel is the registrant.

The first repair asked whether a seat's `operator_id`, `pubkey` and `payout_payload` differed from
the registrant's. Every one of those is a field the registrant writes into its own
`BondRegistered`. The re-audit showed it with an executable fixture: seven sybils paying seven
addresses were seven independent parties, the class left `Candidate` on them, and a quorum of them
licensed its claim — with the fence armed and no stranger anywhere on the chain.

**No comparison of registrant-written fields can do better**, because there is no beneficial
ownership on this chain to compare. What a registrant cannot write is the composition of a
population it did not choose. That is the whole decision.

## 2. Decision

### 2.1 The outsider seat

For a claim of a **bought** class (`registrant_bond` is `Some`) whose `accepted_daa` is at or past
the fence's height:

* the panel's **first seat** is drawn from the **network's base-class population** — every bond
  eligible to judge the liveness floor, under the claim's own executor exclusions, floor, headroom
  and registration cut — minus the registrant's own bond and operator, ranked by
  `H(outsider-ticket domain ‖ anchor ‖ claim ‖ operator_id)`, one entry per operator;
* the other `seat_count − 1` seats are drawn from the class's own population as before, without
  the outsider's operator;
* **no licence stands without the outsider's `Valid`** — on the V1 quorum arm, on the V2 coverage
  arm (which carried no independence check of any kind before this), and in the acceptance layer
  that assembles and validates both. A quorum of the class's own seats is a quorum the registrant
  may hold entirely; the outsider is a condition of the licence, not one voice in it.

An outsider that cannot judge the class says `Incapable`. The claim then voids at its receipt
deadline, which is where every unreachable quorum already ends. **The cost of a model the network
does not run lands on the claims of that model, never on the seat that declined to pretend.**

Genesis classes have no registrant and are never outsider-judged: their panels are the ones they
always had, and the shipped registry's zero slack (`seat_count + 1` bonds) is untouched.

### 2.2 The population is fixed before its randomness

The anchor's hash is a panel's randomness. Once it exists, a party can search operator keys offline
for a ticket below every eligible operator's — a few dozen hashes against a population of
twenty-seven — and register that bond before the panel binds. `palw_bond_maturity` closes this with
a window, **and it is dormant on every shipped preset**, so today every panel on every network is
open to it.

Past this fence, every seat of a governed claim must be held by a bond that some chain block
**before the anchor** registered (`registered_daa < anchor_daa`); the maturity window, where armed,
combines as the stricter. `a_bond_registered_after_the_anchor_cannot_grind_its_way_into_the_outsider_seat`
runs the grind: without the cut the ground bond takes the outsider seat, with it the bond sits
nowhere and the panel is the one the chain would have drawn without it.

This applies to the whole panel of every claim accepted past the fence, genesis classes included —
it is sortition hygiene, not independence, and it costs nothing a bond registered before the claim
does not already have.

### 2.3 The admission jury

A bought class opens `Candidate` (ADR-0145 §7) and leaves it only when a **jury the network drew**
finds a majority of itself holding the class:

* **population** — active bonds above the floor that serve the liveness floor, registered before
  the span whose anchor seeds the draw began, minus the registrant's bond and operator;
* **draw** — `seat_count` operators, one entry each, ranked by
  `H(jury domain ‖ H(class ‖ span ‖ anchor block ‖ anchor execution key) ‖ operator_id)`;
* **randomness** — the execution lane's seed anchor of the span before (ADR-0130): re-rolling it
  costs a winning inference, where a boundary block's own hash costs a header;
* **verdict** — a strict majority (`seat_count / 2 + 1`) of the jurors each hold a bond READY for the
  class by the registry's own five-clause predicate — the same one the draw's population uses, so
  "ready" means one thing;
* **ration** — one audit per `epoch_length / span_daa` spans, on the spans whose index is a multiple
  of it. Stateless: no row records the last audit, so none can be written or rewound to run another.
  No seed anchor in the span before is no audit; a short jury admits nothing. *(Amended 2026-09-23,
  user decision: a network may set the period in DAA — `Params::palw_admission_audit_period_daa`;
  testnet-12 uses the standard `PALW_ADMISSION_AUDIT_PERIOD_DAA_STANDARD` = 100 DAA, one audit per
  100 spans at its one-DAA span. `None` keeps the one-epoch period above, byte for byte.)*

A lottery that may be re-run every span is passed by waiting — at a five-DAA span, a registrant who
wins the jury one time in a thousand would be through in under an hour and a half. The rate the chain
can bound is how often the draw is taken.

### 2.4 One claim, one answer

The claim-level rule reads the fence's **height** against the **claim's own `accepted_daa`**, never
against the block reading it — the same construction `palw_canonical_work` uses and for the same
reason: the draw (resolved at the anchor), the binding, every licensing arm and every node replaying
them read one claim at different chain points. A claim accepted just below the fence is bound
without an outsider and licensed without one, even when its licence lands above the fence
(`a_licence_without_the_outsiders_valid_is_refused_on_every_licensing_arm`). The block-level rules
(`Candidate`, the jury, the refused cadence grant, the work-price filter) ask the same height at the
block. `PalwTransitionExtrasV1::admission_independence_daa` is the one field for both, so the two
readings cannot disagree about where the fence is.

### 2.5 What the stratified draw does not have

`palw_shard_licensing`'s per-shard draw seats no outsider, so an outsider-judged claim licensed by
parts would be judged by the population its registrant fills. `validate_palw_v2` refuses a build
arming this fence beside `palw_shard_licensing`, and the fold refuses a part licence of an
outsider-judged claim by name. Both are dormant on every preset.

## 3. The bound, stated at its real strength

Let `f` be the registrant's share of the network's floor population (its operators over all
operators serving the liveness floor).

| event | probability | per |
|---|---|---|
| the registrant holds a claim's outsider seat | `f` | claim |
| the registrant alone passes an admission audit | `P(Hyp(N, fN, s) ≥ ⌊s/2⌋+1)` ≈ `C(s,⌊s/2⌋+1)·f^(⌊s/2⌋+1)` for small `f` | audit period |

Measured on the fixtures (seven sybils against twenty honest operators, `s = 5`): the outsider was a
sybil at 98 of 400 anchors, 24.5 % (predicted 7/27 = 25.9 %); the registrant passed the jury alone at
343 and 366 of 4,000 seeds in two independent seed families, 8.6 % and 9.2 % (predicted
7,371/80,730 = 9.13 %); against two hundred honest operators, at 2 of 4,000 (predicted 0.036 %).
Under the rule this replaces, the same registrant passed **both** with probability one, for the price
of seven payout addresses.

**What the bound is.** Self-judgement is priced in the honest network's size, where it was a
constant six keys. **What it is not:** a party that IS a large share of the network is that share
(`a_registrant_that_is_the_whole_network_is_admitted_by_it` pins the `f = 1` case), and no rule that
reads only the chain can make a network out of nobody.

**The residual, named.** For a class nobody else runs, honest outsiders plead `Incapable` and its
claims void; the registrant's own licensed rate is `f`, and a claim licensed by a registrant-held
outsider was checked by nobody. That leak is bounded by `f` times the class's eligible volume, which
the lifecycle keeps small until verified use expands it — but it is not zero. What closes it is an
outsider that can check a claim WITHOUT holding the model: ADR-0133's segment-scoped receipts let a
seat attest one segment, which needs that segment's weights and nothing more. Its protocol half is
armed at testnet-11 6,100 and its runtime half is not built. The consensus rule here is unchanged by
it — only what an honest outsider can answer instead of `Incapable`.

## 4. What was removed

`palw_bond_is_independent_of_registrant_v1`, `palw_panel_has_independent_seat_v1`, the errors
`PanelWithoutIndependentSeat` and `QuorumWithoutIndependentSeat`, the observation field
`independent_ready_seats`, and the node assembler's identity check before proposing a panel (the
outsider is IN the derived panel, and the acceptance layer demands that panel exactly).
`admission_independence_active: bool` became `admission_independence_daa: Option<u64>`.

## 5. Tests

In `palw_state_v2::tests::adr0135::admission_independence`:

* `the_outsider_is_drawn_from_the_network_and_not_from_the_class` — below the rule every panel of a
  Kimi claim is five sybils; past it the outsider is a sybil at the network share, never the
  registrant or the producer, and the draw is identical under both payout layouts;
* `a_bond_registered_after_the_anchor_cannot_grind_its_way_into_the_outsider_seat` — §2.2, the grind
  run both ways;
* `a_licence_without_the_outsiders_valid_is_refused_on_every_licensing_arm` — V1 and V2, `Incapable`,
  and the claim-keyed era;
* `a_candidate_leaves_only_on_a_jury_the_network_drew` — through the fold, with the lane's real seed
  anchor: a minority seed keeps it out, a majority seed lets it in, the honest network holding the
  model lets it in, no anchor and off-schedule spans draw nothing;
* `the_registrant_passes_the_jury_alone_at_its_network_share_not_by_construction` — the
  hypergeometric bound over four thousand seeds, and payout-layout invariance;
* `a_registrant_that_is_the_whole_network_is_admitted_by_it` — the bound at `f = 1`;
* `a_registrant_paying_its_sybils_separately_no_longer_judges_its_own_class` — the re-audit's attack,
  re-run, now failing for the attacker;
* the three that survive from the first repair: the `Candidate` gate, genesis/dormant roots, and I4.

And `palw_reward_properties_v1::a_class_whose_owner_holds_every_capable_seat_cannot_be_admitted` —
ADR-0145 §8's property, un-ignored: declaring the class buys no juror, and the owner alone is admitted
at its network share.

## 6. Addendum (2026-09-20): a registration does not straddle the fence

Below `palw_admission_independence` a post-genesis registration must take the minimum grantable
share; past it, exactly 0‰. The gate reads the fence at the block that ACCEPTS the carrier. The
Studio economy drill, which arms the registry and this fence at one height, found the gap between
the two: the node registered the moment the registry opened, on terms the RPC had computed at the
sink's DAA (one block behind, pre-fence, 1‰), and the carrier landed past the fence, which dropped
it — "registers at 1‰; past palw_admission_independence a registration buys existence and not
cadence". The panel's own retry only rebuilt it 200 DAA later.

The registration terms are now resolved at the virtual's DAA, which is where a carrier sent now
lands, and a node does not build a registration within `PALW_REGISTRATION_LANDING_MARGIN_DAA_V1`
(10) DAA below a scheduled independence fence (`palw_registration_waits_for_fences_v2`); it builds
once the fence is in force, on the 0‰ terms the gate will apply. A carrier that still lands late is
caught by the panel's retry, as before. The CLI paths (`model add`, `extension submit`) read the
same terms; they have no landing-margin wait, which leaves a ten-DAA window in which a hand-filed
registration can be refused and its fee lost — named, not closed.

---

## Implementation (2026-09-21)

ADR-0146's search reports 1.000000× — P4 does not forbid this fence. It stays `None` on every
shipped preset. Choosing a height is a different commit (0144 item 0).

## Mission alignment amendment — 2026-10-07

drawn outsiderは通常検査の供給を増やす仕組みとして維持するが、outsiderが選ばれることや一人のhonest seatの存在を全不正の検出保証にしない。選ばれていない普通のpublic bondの訴追権は独立に必要である。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0147-independence-is-drawn-not-declared.md -->

</details>

<a id="source-0166-verifier-unavailability-is-not-producer-fraud"></a>

<details>
<summary>0166-verifier-unavailability-is-not-producer-fraud.md — 原文 L1–L125</summary>

[原文を開く](../adr/0166-verifier-unavailability-is-not-producer-fraud.md) · SHA-256: `1566a51b9f1bde8d1d9a51cfe567eeed2e8f77de0e79e292988fb557fc5871c8`

<!-- ADR_SOURCE_BEGIN:0166-verifier-unavailability-is-not-producer-fraud.md -->
~~~~~~~~markdown
# ADR-0166 — Verifier unavailability is not producer fraud

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Model / claim-evidence boundary, 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md) withdraws model-wide FPR/TRDC/Seeder service liability and availability-based reward/weight gates. This historical court concerns named, bounded claim evidence. Future requests must exclude model acquisition/full-weight/iterated-range disclosure and fix permitted units/cumulative scope. Claim-specific default and authenticated computation fraud retain distinct evidence and responsibility. Existing implementations, tests and old fences keep their historical scope; no new allocation or court restriction is activated here.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-10-03 at the operator's request; IMPLEMENTED the same day on `rcore/panel-liveness`, behind three
  dormant fences (`palw_panel_unavailable_expiry`, `palw_panel_fast_switch`, `palw_seat_availability`) — `None` on every preset,
  so no network's fingerprint, schedule or fold moves. Lane INT arms them in the DAA-5,300 list.
* Amends: ADR-0064 (silence is not observable — kept), ADR-0152 §S0′ (the second `ReceiptTimeout` / `NotReplayBacked` charge),
  ADR-0124 (seat draw weight), ADR-0160 E-4 (the obligation hold has no `PanelUnavailable` row).

## 0. The sentence this ADR is

An honest producer of a real model is never slashed because panel seats stayed silent: two panels that fail to conclude end the
claim with no reward and no charge, a silent first panel is replaced after `T_fast` instead of after the whole receipt window, and
seats that answer are drawn more often than seats that are new or gone.

## 1. The problem

Until now the second receipt timeout (and an S2 licence a second panel did not back by replay) charged the PRODUCER the whole
escrow-inclusive reservation, whoever caused the silence: the chain cannot observe silence (ADR-0064), seats are never charged, and
a permissionless seat Sybil pays nothing to cause it. Heavier models draw panels that are silent more often, so running them was
the more dangerous play and miners were pushed to the cheap base floor.

## 2. Part C — expiry is not fraud (`palw_panel_unavailable_expiry`)

Past the fence the second timeout, and the second panel's `NotReplayBacked`, end the claim `Voided { PanelUnavailable }`
(void reason, Borsh 10, appended): no reward, the reservation and bond lock return AT ONCE (no E-4 hold — nobody abandoned
anything), nothing is slashed, no strike is written, the seats get nothing, the fee is not refunded. The slash funnel
(`void_and_slash_at`) returns 0 for the reason whatever route reaches it. Every exhaustive match over the void reasons names it
(`palw_void_reason_keeps_obligation_v1` and its twin: false; the shadow's attribution: `Undetected`; the ledger name
`panel_unavailable`).

**Still slashable (producer fault, not touched):** a court loss (`CourtFraud`, `CourtDefault`, `CourtHeldVerdict`), an invalid
commitment, and an unanswered data-availability demand (`ProducerWithholding` through the DA court and its three-strike action
tier) — so withholding trace data to make seats silent is not free: a seat that cannot be served files a DA accusation, and the
producer's silence there convicts. AG-2's whole-bond forfeiture and the reputation strikes are triggered by convictions, never by
a void reason, so an expiry cannot reach them.

## 3. Part D — a fast switch (`palw_panel_fast_switch`)

**Deviation from the brief, recommended and implemented:** the brief asks for 5 primary + 2 standby seats whose receipts join the
quorum after `T_fast`. In this code base a seat's duty is a segment mask drawn from `seats.len()` (V2 verification: a full seat and
`K = seats − 1` partials, two attestations per segment), and `panel.seats` is read by ~60 sites (licence arms: plain, coverage,
optimistic, batch, supplementary V3, shard parts; counted-mask recount; false-valid and offence attribution; DA; node
producers). Seating standby seats inside the panel changes the assignment of every primary seat, and keeping the primaries'
assignment while admitting full-mask standby receipts needs every one of those sites changed and re-proved before the freeze.
The goal — a silent panel does not hold a claim for the whole window — is delivered with the machinery that already exists and
is already proved: the REDRAW.

Past the fence the first panel of a claim (`rebound_daa == None`) is swept at `bound + T_fast`, `T_fast = clamp(W_r(c)/20, 30,
W_r(c)/2)` — 30 DAA at the shipped 600-DAA window, 699 at the 2M class's 13,995; healthy quorums are 2–6 DAA at the floor, so
`T_fast` is 5x that and scales with the class's own verification window. The sweep revives the claim (the existing first-timeout
redraw: nobody charged, a fresh panel anchored on the sweep, SW-8's one-block bind) and the redraw gets the WHOLE window
(`T_hard = W_r(c)`), after which part C ends the claim. The sweep deadline is one function
(`PalwStateParamsV2::panel_bound_sweep_window_v1`) read by the bind's arm, DL-1, the load check and the DA re-arm. A claim's
receipt deadline for validity (the supplementary doors, Q-5's gate) is unchanged.

Cost and risk: a healthy-but-slow first panel loses its partial work at `T_fast` (not its claim); the node's backup-seat promotion
(120 DAA, `palw_seat_schedule`) is unchanged and simply becomes moot for a first panel that is replaced earlier. Standby seats are
phase 2 (§6).

## 4. Part E — positive liveness in assignment only (`palw_seat_availability`)

A per-bond row (`PalwSeatAvailabilityV1`: first credit, and per-epoch `assigned` / `credited` counters inside a rolling window)
in rooted, journalled state (delta variant `SeatAvailability`, number 105; carriage tail `0xE6`), written at exactly two places:
the panel bind (`reserve_seat_duties`: one assignment per seat) and the duty row's `0 → credited` edge
(`credit_seat_receipts`: one success). Constants: epoch 2,000 DAA, window 6 epochs (12,000 DAA); factor 0.5x with fewer than 3
credits in the window (new or inactive, including no row), 1.2x with at least 30 credits, at least 900‰ of the window's
assignments credited and a first credit two windows old, else 1.0x; integer permille. The factor scales the CLASS SEATS' race
weight (`max(1, ⌊w·f/1000⌋)`) in `palw_panel_stake_entries_availability_v1`; the SW-10 floor, the ADR-0147 outsider's race, the
security weight, slashing, fork weight and pay stay stake-only. Absence is never counted against a seat: it can only keep a seat
from 1.2x, or — by letting its credits leave the window — return it to the new-seat 0.5x.

## 5. Fences and prerequisites

| fence | prerequisites (at or below) |
|---|---|
| `palw_panel_unavailable_expiry` | `palw_audit_2026_09_23` |
| `palw_panel_fast_switch` | `palw_panel_unavailable_expiry`, `palw_rcore_plus` |
| `palw_seat_availability` | `palw_rcore_plus` |

Each is a bare height with a borsh-skipped mirror in `PalwStateParamsV2`, Some-only in the params and schedule ids with the
`never()` collapse, on the fork-id list, in `PALW_T12_PANEL_LIVENESS_FENCES_V1` (the flag-day entry), and moved together by
`--palw-drill-panel-liveness-at`.

## 6. Open

* Standby seats (the brief's D) as a phase 2: needs the seat-view helper at the ~10 mask/membership sites and a decision on the
  pay denominator (`max(primaries, credited)`), plus capital for 7 seats at bind.
* `T_fast` and the availability thresholds are constants chosen from the 10-02 healthy figures; the adaptive form is stage 6.
* An S2 licence's second `NotReplayBacked` now also expires uncharged: a producer colluding with partial seats to license a false
  claim loses only the reward unless a court convicts (the colluding seats' `Valid` locks stay exposed to `PanelFalseValid`).

## Mission alignment amendment — 2026-10-07

verifier unavailabilityをproducer fraudとしない決定を維持する。licenseした偽claimは、不在timeoutではなくpublic objective courtで責任を問う。Panel redrawや期限切れで、外部proofの独立した受理・証拠保持を消さない。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

## 改定 — 2026-10-12（非不正終了と採掘枠の非早期回復：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3（ADR166との接続）。

新rulesetでは、PanelUnavailableは非不正の終了とし、producerを自動slashしない。ただし「無罰」と「消費した採掘枠を即時に再利用可能」は同義ではない。資金ロックを解除する実装を採る場合でも、元本の消費履歴と `reuse_not_before` を引き継ぎ、移転・再登録による迂回を禁止する。より単純な初期実装では、必要元本の拘束を既定回復時刻まで維持する。Panelの交代も、claimに固定した絶対期限の内側に制限する（RFC-0010・RFC-0014の2026-10-12改定）。
~~~~~~~~
<!-- ADR_SOURCE_END:0166-verifier-unavailability-is-not-producer-fraud.md -->

</details>

