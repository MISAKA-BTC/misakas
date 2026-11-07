# ADR-v2-0028 — Attempt・ticket・抽選・seed anchor

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 5本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md)

## 1. この文書の役割

inferenceをticketへ結び付ける規則と、その乱数・anchorの出所を整理する。

## 2. 統合した判断の読み方

### 2.1

旧attempt価格案と、execution commitmentそのものをticketへ使う改定を区別する。nonce以外の自由fieldによる無償の抽選増幅を避ける議論を保持する。[旧ADR-0071](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md) [旧ADR-0072](../adr/0072-the-ticket-is-the-execution.md)

### 2.2

仕事をdrawの前にcommitし、User／Canonical promptを別modeとして同じclaim検証へ接続する。[旧ADR-0074](../adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md)

### 2.3

merged attemptを含むseed anchorのwindow化を時系列で読む。hash lotteryをなくすADR-0141の問いは、その文書自身が設計不採択としている。[旧ADR-0170](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md) [旧ADR-0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md)

## 3. 維持する境界・後続改定

ADR-0071 Decision 1の撤回とADR-0141の未採択を[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)に記録する。lotteryとheartbeatは別の故障モデルなので、一つの安全性説明に溶かさない。

## 4. 隣接する判断との分担

DAAとretargetは[ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md)、Panel beaconは[ADR-v2-0032](0032-sampling-beacon-and-receipts.md)、仕事価格は[ADR-v2-0026](0026-canonical-work-and-answer-accounting.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0071-the-attempt-lanes-price-and-the-tickets-bound.md](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md) | L1–L278 | L15–L21 |
| [0072-the-ticket-is-the-execution.md](../adr/0072-the-ticket-is-the-execution.md) | L1–L365 | L15–L23 |
| [0074-the-attempt-is-a-claim-drawn-by-the-chain.md](../adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md) | L1–L214 | L23–L35 |
| [0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | L1–L136 | L15–L19 |
| [0170-the-seed-anchor-is-a-window-not-a-span.md](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md) | L1–L147 | L15–L19 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0071-the-attempt-lanes-price-and-the-tickets-bound"></a>

<details>
<summary>0071-the-attempt-lanes-price-and-the-tickets-bound.md — 原文 L1–L278</summary>

[原文を開く](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md) · SHA-256: `8bdf0cbc927aa1bbb4ffd8133e9d6496cf6c798b83c9f0075c10a4d1a6df9c50`

<!-- ADR_SOURCE_BEGIN:0071-the-attempt-lanes-price-and-the-tickets-bound.md -->
~~~~~~~~markdown
# ADR-0071 — The attempt lane's price, the ticket's bound, and who may judge a class

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **IMPLEMENTED WITH DECISION 1 WITHDRAWN (2026-09-02; proposed 2026-09-01).**
Decision 1's target freeze shipped to the public testnet, was measured to remove the only control on
block interval, and is reverted — see §3. Decisions 1a, 2 and 3 stand (Decision 2's pwu divisor is
superseded by ADR-0072, which takes the nonce out of the ticket altogether; the bucket itself
stands as the anchor's position field). Rejected variants retain only the reason and successor here;
the old specification is in Git history. Decision 1a's relative expectation / idle ceiling and
Decision 3's capability filtering remain implemented.

Written against the mainnet-premise audit of ADR-0068 Phase 2, whose findings challenged earlier
decisions. Builds on ADR-0038 (PALW is the
consensus work), ADR-0045 (`DerivedV1`, the class economy), ADR-0054/0056 (share follows
production), ADR-0060 (liveness doctrine), ADR-0065 (a bond must be earned and a seat must be
someone else), ADR-0066 (the heartbeat lane out of header bits) and ADR-0067 (classes are chain
data, kernels are the build). Consistent with the standing doctrine that consensus changes ship by
activation, never by re-genesis.

> **Security amendment appended (2026-09-02)** — see the last section: Decision 3's `capable_classes` is bounded and replaces; each declared class reserves exposure; a seat is drawn for a class only after a positive chain fact (production on it); §5's open item gets a chain-checkable form without judging silence.

## 1. Why these four are one ADR

The audit's premise was the user's: **a Qwen block's weight is given, and weight must not depend on
hash computation.** Measured against that premise, live fork choice already passes — the attempt
lane weighs a constant `1 << 20`, the receipt lane weighs zero, the heartbeat lane weighs
epsilon = 1, and V2 admits no other algorithm, so `calc_work(bits)` is unreachable on a running V2
chain. What does *not* pass is everything downstream of `bits`: the pruning proof priced history by
it (fixed, `cbe5c002`), the lottery still meters *tries* rather than *executions*, the class
difficulty feeds back through the same field, and a claim's collateral was priced on that difficulty
(fixed, this train).

The four items below are what remain. They share one shape: **a quantity that should describe
LLM work is derived from, or bounded by, the hash lottery** — and in three of the four the code
comment at the site states the coupling as an intentional choice. That makes them ADR material
rather than patches, and it makes them one ADR rather than four, because Decision 1 and Decision 2
move the same number in opposite directions and shipping either alone regresses the other.

## 2. What already landed, so the scope is honest

Recorded here because a reader arriving at this ADR needs to know which half of the audit is code
and which is proposal:

* **The pruning proof no longer prices history by `bits`.** `blue_work_diff` took
  `.max(self.level_work)` on the attempt-lane arm, so adopting a heavier history was bought with
  proof-of-work levels. Removed (`cbe5c002`), with a test asserting `level_work(1, 225)` is
  `attempt × 4096` — the inequality that made the `.max()` load-bearing. Level *attainment* still
  requires grinding, so history adoption is grinding-priced linearly rather than exponentially;
  the proposed Decision 1 freeze was withdrawn; ADR-0072 retains `bits` as cadence control.
* **A claim's collateral is no longer priced by difficulty.** `reserved` was
  `attempt.pwu × slash_value_per_pwu`, and under `DerivedV1` `attempt.pwu` is
  `expected_attempts(class_target) × pwu_per_inference` — so a class that retargeted harder reserved
  more against unchanged collateral and locked its own producers out for succeeding. On the floor
  class that is the chain stopping, because a refused attempt on a V2 network is
  `StatusDisqualifiedFromChain` and DAA only advances when blocks are produced. Now
  `palw_exposure_pwu_v1`: one inference's worth, at every site that prices it — admission's ceiling,
  both state writes, the producer's own headroom prediction, and the genesis bind-window gate.
* **The 120 s cadence is a set of fields, applied in one place.** `palw_v2_params_on_base` wrote
  `target_time_per_block` alone, leaving the DAG parameters and the DNS windows counted for the
  base's block rate. At 120 s an inherited `PRODUCTION_DNS_PARAMS.unbonding_period_blocks` states
  14 days and means about 46 years, and `bond_spend_gate` enforces it in consensus. Now
  `Params::with_two_minute_cadence` / `with_palw_v2_depths`, gated in `validate_palw_v2`.
* **The court's close binds the registered class.** `adjudicate_close_proof_v2` accepted a close
  whose `shape_profile` was not the class under judgement.
* **The GDN replay has a ceiling on both arms**, not one.

## 3. Decision 1 — The attempt lane's price comes off `header.bits`

**What is true today.** ADR-0066 took the *heartbeat* lane's price out of `bits`, and its comment
records exactly why: `bits` is the field the difficulty window averages, so a window of rows priced
by their own lane raises the global demand to that lane's price and no other block can re-enter.
The substitution lives in `consensus/pow/src/lib.rs` — the one place every PoW path goes through —
and it covers `POW_ALGO_ID_HEARTBEAT_V1` only. The attempt lane, `POW_ALGO_ID_PALW_COMMITTED_V2`,
still reads its target from `header.bits`, and `pre_pow_validation.rs` still enforces
`header.bits == expected_bits` against the window for it.

**Why that is the same defect.** The attempt lane's *weight* is already a constant, so the coupling
does not show up in fork choice. It shows up in **admission**: `bits` sets the class target, the
class target sets `expected_attempts`, and `expected_attempts` is the number of hash tries an
inference must be paired with. A network that wants "weight does not depend on hash computation"
cannot leave the *rate* of LLM work denominated in a hash difficulty that a window of blocks
feeds back into.

**旧 bits 固定化の導入案は撤回済み。** 理由は以下の withdrawal note に従う。

### Decision 1 — WITHDRAWN after Relaunch 5 measured it (2026-09-02)

`bits` の固定化は再実装しない。class 間の share 調整だけでは単一 class の block cadence を制御できず、実測で floor が目標を大幅に超えて生成し続けたため。Decision 1a の idle-target 修正と残る有効規則は維持する。

### Decision 1a — AMENDED at implementation: the expectation stays relative, and the repair is a ceiling

旧 `share × DAA span` の absolute expectation は採用しない。期待値の総和が実測総数と一致せず、全 class の target を一方向へ動かし続けるため。relative expectation と以下の idle ceiling を採用する。

The diagnosis was also narrower than the draft claimed. A class that produces *any* blocks is
measured correctly: at 500‰ each, A producing 100 and B producing 20 gives A `observed 100 >
expected 60` and B `observed 20 < expected 60`, so B eases. The blind spot is exactly one case wide
— `observed == 0`.

And that case cannot be repaired by easing. **Silence is not evidence of trying.** The chain sees
block counts, never attempts, so "locked out" and "nobody ran it" are the same observation; a rule
that eases on silence lets a registrant buy cadence with patience instead of work — register, wait
for the target to walk to trivial, then take the class's whole epoch budget for free.

**Decision.** An idle class converges toward the price the producing classes are actually paying,
and never past it. `floor_price` is the hardest target any class that produced in this span holds.
A class harder than that is paying more than anyone and losing, so it converges toward that price,
`max_factor`-bounded per boundary, and stops there. A class already easier than that is not locked
out and does not move. Nothing is ever priced below what a producing class pays, so patience buys
the incumbent's terms and never better ones — which is what work buys.

This is arithmetically independent of `retarget_over_span_v1`: an idle class is outside the
`Σ expected = Σ observed` sum by construction, so the ceiling cannot disturb any producer's
expectation. It is also the missing half of a rule the codebase already states elsewhere — an
entrant's initial target is the base class's, "priced like the incumbent rather than by its
registrant" — which was true at registration and never tracked the incumbent again.

Implemented as `palw_class_daa::converge_idle_target_v1`, called from the epoch-close retarget where
the `continue` used to be.

## 4. Decision 2 — The ticket is bound to executions, not to tries

`expected_attempts >> k` で ticket の work を価格付けする旧案は採用しない。実行 seed から nonce を除き、一 execution が一 ticket を生む方式へ変更したため。[ADR-0072](0072-the-ticket-is-the-execution.md) が置き換える。`k = 22` の anchor position は維持するが、work の divisor にはしない。

## 5. Decision 3 — A panel seat must be able to run the class it judges

**What is true today.** `derive_panel_v2_with_maturity` draws seats by bond ticket, excluding the
executor's bond, operator and key, and filtering on collateral and maturity. It does **not** filter
on whether the drawn bond can execute the class under judgement — `PalwBondStateV2` carries no
capability declaration at all (`pubkey`, `operator_id`, `collateral`, `slashed`, `status`,
`registered_daa`, `payout_payload`, and nothing else).

The V1 job panel has exactly this filter and states the rule the V2 draw is missing: a bond with no
capability declaration is **excluded, never defaulted**, because "a validator that never declared
one cannot be assigned to replay a class it may not have, and assigning it anyway would manufacture
no-shows against honest operators."

**Why it bites now and did not before.** While one floor class held all the weight, every seat could
run every class by construction. ADR-0068 gives the model tiers 97.8% of cadence, and a 33 GiB
artifact is not something a seat holds by default — so a panel drawn blind to capability seats
validators who can only abstain, and a claim that cannot reach quorum voids. `palw_unavailable_abstains`
turns that into an abstention rather than a false conviction, which is correct and is not a
substitute: an abstaining panel still fails to license the claim.

**Decision.** Give `PalwBondStateV2` a declared capability set — the class ids whose artifacts the
operator has staked collateral on being able to run — set at registration and amendable by a
lifecycle object, and filter the V2 claim-lane draw on it exactly as the V1 job panel filters on
`runtime_class_id`. Undeclared is excluded, never defaulted.

This is a state-schema change, so it carries the usual freight: a registration object field, a
lifecycle amendment path, the genesis registry, carriage, IBD and pruning round-trips, and an
activation. It is named here rather than patched because a capability the chain does not record
cannot be filtered on, and inventing the record is a design decision about who attests to holding
an artifact and what it costs to lie.

**AMENDED at implementation, on two points.**

*A node's own registration declares nothing.* At registration a node has proved it holds
collateral; it has proved nothing about holding a 33 GiB artifact, and it has not yet read which
classes the chain registers. Declaring there would be volunteering for duty it cannot perform, and
the duty accounting convicts the seats the draw names. So `kaspad`'s self-registration ships an
empty set and the operator declares separately — which is also what makes withdrawal work, since an
operator who deletes an artifact must be able to stop being seated for it rather than choose between
keeping the disk and being convicted.

*Genesis is where capability is assigned rather than claimed*, for the same reason cadence is. The
genesis registry has zero slack by construction (`seat_count + 1` bonds, executor excluded), so a
genesis whose bonds declared nothing would be a network where every claim voids at `BindTimeout`
with its escrow burned. Genesis bonds therefore declare the classes the genesis registers, and the
class-registering assembly extends the declarations at the same moment it funds the tiers — the same
shape as the collateral re-derivation that already sits one line above it.

*And it is not a consensus gate.* ADR-0061 retired the "a genesis must seat a panel" refusal because
an under-seated genesis is transitional: the heartbeat carries blocks, bonds arrive as transactions,
licensing begins when the seats do. A class no seat declares is transitional in exactly the same
way, so refusing it at genesis would re-impose the rule that ADR retired. What is asserted instead
is a test over the real shipped assembly: every class the card funds has `seat_count + 1` distinct
operators declaring it.

**What it is not, stated because this ADR's first draft got it wrong.** A capability declaration is
a claim, not a proof, and **lying about it is not punished on this chain today.** The draft said the
thing making it expensive to lie is that "a declared seat which cannot serve is a seat that gets
convicted". That is false here: ADR-0065 D4 turns an `Unavailable` receipt into an abstention rather
than a conviction, and it is armed on every shipped preset — because silence is not checkable (a
seat that says nothing is indistinguishable from a seat that was never asked), a doctrine this
project reached by measurement and does not intend to reverse.

So what a false declaration actually costs is nothing directly, and it costs the *network* a seat
that can only abstain. What bounds the damage is the redraw — a panel that concludes nothing is
revived once and binds a second — and what bounds the incentive is that a seat which never concludes
earns nothing for sitting. That is weaker than "binding on the declarer", and the honest statement
is that this Decision makes the draw **correct** (it stops seating validators who provably cannot run
the class) without yet making the declaration **costly**.

Making it costly is a separate question and it runs straight into the silence doctrine: any rule
that punishes a declared seat for not answering punishes an offline honest operator identically. It
is named here as the open item rather than assumed away — and it is the reason this Decision is not,
by itself, a defence against a registrant who declares everything.

## 6. Considered and rejected

* **Freeze the attempt lane's `bits`.** 不採用。relative class share だけでは absolute cadence を制御できないため（§3、ADR-0072）。

* **Solve the ticket problem by lowering `NONCES_PER_TEMPLATE`.** Rejected: that is a node-local
  constant in `kaspad`, so it binds honest producers and nobody else. The bound has to be in the
  anchor, which is consensus.
* **Filter the panel by asking nodes at draw time whether they hold the artifact.** Rejected: the
  draw must be a pure function of chain state at the anchor, or two nodes seat different panels for
  one claim.
* **Ship Decision 3 as a node-local preference in the producer.** Rejected for the same reason —
  and because the duty accounting charges exactly the seats the consensus draw names, so a
  node-local filter changes who shows up without changing who is blamed.

## 7. Invariants to verify at each step

1. **Idle target:** above-incumbent targets converge to the producing classes' price and stop there; targets already below it do not ease on silence. `Σ expected = Σ observed` stays unchanged.
2. **Ticket and work:** verify the nonce-free execution ticket and execution-priced work under ADR-0072; the withdrawn freeze and nonce-sweep/divisor rules are not acceptance criteria.
3. **Capabilities:** an undeclared bond is never seated. Capability declarations do not prove independent human operators or justify convicting silent seats.
4. **Activation:** current activation and rollout follow the successor ADRs; the withdrawn variants introduce no future fence.

## What landed

**Decision 1a.** `converge_idle_target_v1` implements the relative expectation and idle-target ceiling. Decision 1's fixed target was reverted for the cadence reason in §3; Decision 2's nonce-sweep / divisor implementation was superseded by ADR-0072's one-execution ticket.

**Decision 3.** `PalwBondStateV2::capable_classes`, carried by `BondRegistered` and covered by its
signature; `BondCapabilityDeclared` as the amendment object, admitted to the ride list on the same
signature-present rule as retirement and authenticated against the bond's own registered key at
acceptance; `palw_bond_may_judge_class_v2` as the one predicate; the filter in
`derive_panel_v2_with_maturity`; genesis bonds declaring the classes their genesis registers, in
both the base assembly and the tier-funding one. `PALW_STATE_V2_VERSION` 13 → 14, with both golden
roots and the ADR-0043 second implementation moved together, per that test's own rule. The operator
path is `misaka bond capability`, built the way `misaka bond retire` is — same ownership guard, same
carrier, same dry-run — because an object an operator cannot send is a rule that only genesis obeys.

Decision 1a and capability filtering remain implemented; the current ticket follows ADR-0072. §2 records the earlier audit fixes.

## Security amendment (2026-09-02) — Decision 3 gets a bound and a price, and §5's open item a chain-checkable form

**SA-1 — The declared set is bounded and replaces.** `capable_classes.len() ≤
PALW_MAX_CAPABLE_CLASSES` (proposed 16), and a new `BondCapabilityDeclared` replaces the previous
set rather than growing it. The set is hashed into the state root (`palw_bond_capability_message_v2`)
with no bound today, so an unbounded declaration is a state-growth lever priced only by transaction
mass.

**SA-2 — Each declared class reserves exposure.** Declaring `C` reserves `CAPABILITY_EXPOSURE_SOMPI`
per class on the bond's exposure ledger (ADR-0056 Decision 3's shape: reserved, not burned;
released when the class is undeclared or the bond retires). Declaring everything so as to be drawn
everywhere becomes a cost proportional to the griefing surface, and "declare and abstain" — §5's
defect — costs the abstainer capital without judging its silence (ADR-0065 Decision 4 stands).

**SA-3 — Capability is proven by a positive chain fact before a seat is drawn.** A bond may be
seated for class `C` only if it declared `C` **and** at least one accepted attempt block or
free-prompt claim on `C` names it as producer or executor — possession proven by production, a fold
fact — or `C` is a genesis class. Silence stays unjudged; production is judged instead.
Consequence, stated: for a class only its registrant can run, the eligible seats are the
registrant's own bonds — which is why ADR-0069 Decision 7 (an uncertified family's blocks weigh
nothing) is the load-bearing rule and this one is a filter.

**SA-4 — A declaration is a lifecycle object like the others:** fee = mass, signed under the network
domain (already), refused for a class the chain does not have (already) and for a bond that is
`Retiring`.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0071-the-attempt-lanes-price-and-the-tickets-bound.md -->

</details>

<a id="source-0072-the-ticket-is-the-execution"></a>

<details>
<summary>0072-the-ticket-is-the-execution.md — 原文 L1–L365</summary>

[原文を開く](../adr/0072-the-ticket-is-the-execution.md) · SHA-256: `fd5a33fabe60f1b2850e820df0ffc00e6f0f90b83c703b05983445fddba717ac`

<!-- ADR_SOURCE_BEGIN:0072-the-ticket-is-the-execution.md -->
~~~~~~~~markdown
# ADR-0072 — The ticket is the execution: both lotteries priced in inferences

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **IMPLEMENTED (2026-09-02); Decision 7's rollout DECIDED the same day — see §3.** Reviewed the same day
by a second session, whose finding became Decision 8 and the invariant in §4. Builds on ADR-0042 (Decision 3a: the algo-6 tag is an
expansion, never an inference), ADR-0044 (Decision 4: the receipt lane's quantum ticket is a
function of the certified execution), ADR-0045 (`DerivedV1`: pwu is derived from the class target),
ADR-0058 (merged work is counted) and ADR-0071 (Decision 1 withdrawn — `bits` stays the absolute
rate control; Decision 2 — the nonce bucket). Supersedes ADR-0071 Decision 2's pwu divisor; keeps
its bucket. Consistent with the standing doctrine that consensus changes ship by activation or
version, never by re-genesis: this is a version bump (`PALW_ATTEMPT_V2_VERSION` 5 → 6) and a
coordinated upgrade, not a re-mint.

> **Security amendment appended (2026-09-02)** — see the last section: the mainnet activation path §3 records must seed the new lane's targets (ADR-0076), ship the fork-id handshake before the fence, fence the version check, and prove cross-lane replay is refused by algo id.
> **Second pass (2026-09-03)** — the amendment was written before the code and the code was then reviewed adversarially. SA-1's window half is narrower than stated (nothing in this tree lane-filters a window), SA-3 binds on every peer-facing caller and not only the shape gate, SA-4 is a predicate that four sites had to ask, and SA-2's gate must refuse on the DISPUTED fence rather than on any fence. See the last section, including what still argues against arming.

## 1. The finding

An algo-6 header enters two lotteries: the class ticket (admission item 6b, against the class's
retargeted `initial_target`) and the Layer-0 digest against `bits`. Until this ADR both were drawn
from `commitment_root_v2`, the attempt's identity root — and the identity covers the `challenge`,
which is `H(network ‖ pre_pow ‖ timestamp ‖ nonce ‖ class ‖ bond)`. So every nonce was a fresh
draw in both lotteries against one inference. ADR-0071 Decision 2 bounded the sweep to a bucket
of `2^22` nonces so that a verifier could name which execution a nonce was paid for by, but inside
the bucket the sweep stayed free: the public producer ran exactly `4_000_000` nonces per template
per inference, and the audit that produced ADR-0071 measured it.

Two things followed from drawing the lottery off the wrong bytes:

* the class retarget priced a sweep, not an inference — a class's difficulty said how many
  hashes its blocks cost, and the LLM work behind them was a constant one;
* pwu had to be patched to divide the expected tries by `2^22` (ADR-0071 Decision 2's formula),
  an accounting correction for a lottery that should never have counted tries.

The receipt lane never had this problem. ADR-0044 Decision 4 drew its ticket from the certified
quantum and the chain's beacon, so the only way to another draw was another execution, and its
nonce was a uniqueness field from the day it shipped. This ADR gives the attempt lane the same
discipline.

## 2. Decisions

**Decision 1 — The priced bytes are the execution.**
`execution_commitment_v3(attempt, anchor) = H(domain ‖ anchor ‖ borsh(attempt with challenge := 0))`.
The attempt struct is not changed and no second projection is written: the field that changes per
nonce is blanked and everything else is priced, so a field added to the attempt tomorrow is priced
the moment it exists (the P0-1 discipline `commitment_root_v2` already keeps). `commitment_root_v2`
and `attempt_id_v2` are unchanged and remain the block's IDENTITY: two nonces are two blocks.

**Decision 2 — The anchor is derived, never carried.**
`execution_anchor_v3(network_domain, pre_pow_hash, class_id, bond, nonce)` is
`palw_job_anchor_v1(…, palw_nonce_bucket_v1(nonce))` — the job anchor the panel and the court
already derive from a header. Every verifier computes it from the header it holds: the finalizer
arm (on every path that computes PoW, the pruning-proof path included) and the composed admission
entry point. It is not a field of the envelope, so the accused does not get to set the question,
and the `challenge` equation is still checked at both places so nonce and timestamp remain bound to
the position while buying nothing.

**Decision 3 — Both lotteries draw from it.**
The class ticket is `class_ticket_v3(attempt, anchor)`, the low 128 bits of
`H(domain ‖ execution_commitment_v3)`. The Layer-0 digest for algo-6 is
`finalizer(network, algo, pre_pow, timestamp := 0, bits, nonce := 0, Expand(execution_commitment_v3))`
— nonce-free and timestamp-free — and it is still compared to `bits`. ADR-0071's withdrawn
Decision 1 is why: `bits` is the only absolute control on block interval (the class retarget is
relative), and the measured floor of 50 blocks/min is what removing it did. It is kept, and it is
priced in inferences now, which is what it was always meant to price.

**Decision 4 — The class lottery moves beside the position.**
Item 6b leaves the envelope-only stateful list (`check_palw_attempt_admission_v2`, which the state
machine re-runs as a transition guard with no header in hand) and becomes
`check_palw_class_lottery_v3(state, attempt, anchor)`, called from the composed entry point after
the stateless list has agreed that the carried domain and challenge are this network's and this
position's. Every attempt passes the composed entry point exactly once — the chain block's own at
the virtual processor, each merged block's at ADR-0058's pre-check — so nothing is lost; what
changes is that the lottery is treated exactly as the network lottery always was: a header-level
fact, checked where the header is and never re-derived in the transition.

**Decision 5 — One draw is one execution.**
`palw_pwu_v1(target, per_inference) = max(1, expected_draws(target)) × per_inference`. ADR-0071
Decision 2's `>> PALW_TICKET_NONCE_BUCKET_LOG2` is withdrawn (that ADR retains the withdrawal reason); the bucket constant itself stands, because it is what names which execution a nonce was
paid for by. The `DerivedV1` equality (ADR-0045) is untouched — a producer still claims exactly the
derived value.

**Decision 6 — The producer: one template, one inference, one draw.**
The nonce search is deleted. `produce_one` derives the bucket from a cursor, runs the inference
for that bucket's job, builds the attempt with the challenge for `nonce = bucket << 22`, and
decides both lotteries with two hashes. The engine is deterministic, so `(template, bucket) → job
→ execution → ticket` is a function: a bucket a template has lost stays lost, and the cursor is
what keeps a producer from re-running it — a template with the same pre-PoW hash resumes at the
next bucket (the timestamp is outside `pre_pow_hash_64`, so a refreshed template with the same
parents continues rather than replaying), any other template starts at zero.

**Decision 7 — Version, and how this goes live.**
`PALW_ATTEMPT_V2_VERSION` 5 → 6. A node on the old rule draws a ticket per nonce and admits blocks
this rule refuses at the class lottery, so the two cannot share a chain; the version check keeps
them from trying. The shipped fingerprints move on the two presets that carry a V2 bundle
(testnet-11, devnet), because the attempt version is inside the bundle's ruleset id.

The first draft of this decision said "testnet-11 upgrades together; no re-genesis" and left the
rest to the DAA. The review showed that is not a rollout, for two reasons that are stated here
rather than fixed here, because the fix was the operator's choice (§3, now decided):

* **The version check is not fence-gated.** A node on this build refuses every version-5 envelope,
  which is every attempt block the chain already holds — a fresh node cannot validate the history
  it is asked to sync. Every earlier attempt-format change shipped with a re-genesis for exactly
  this reason.
* **`bits` cannot relax.** The swap divides the network's draw rate by ~2^22 while `bits` is
  inherited from the old rate. The difficulty window has no lane filter, so it keeps moving only
  through heartbeat blocks, and each heartbeat adds at most (heartbeat interval / target interval)
  ≈ 3,000 of span per block of work; the relaxation is therefore bounded at ~3,000× however long
  one waits, against the ~10^6× the swap needs. The attempt lane would be dead, not slow — the
  algo-6 dead time after the swap is unbounded.

**Decision 8 — Every field inside the priced bytes is pinned, or it is the challenge.**
"Priced" is not "pinned." A field the producer chooses freely and no rule pins is a nonce by
another name, and the review reproduced it: sweeping `trace_retention_daa` over ONE execution gave
4,096 distinct tickets and 4,096 distinct Layer-0 tags and admitted nine of them at a 2^-9
target, with honest roots, so the panel had nothing to convict — 2^64 free draws on both
lotteries, and with Decision 5 in force each ground block also claimed the full
`expected_draws × per_inference` as pwu. Nothing read `trace_manifest_root`; nothing pinned
`trace_chunk_count` beyond `!= 0`. So the three DA fields are pinned at the composed entry point
(`check_palw_attempt_da_pins_v1`): `trace_chunk_count == 1` (the one shape every shipped family
serves), `trace_manifest_root == attempt_trace_manifest_root_v1(trace_root, count)` (one
derivation in consensus, which every family now uses — theirs hashed a job context under a family
domain that no verifier ever read), and `trace_retention_daa == the block's own DAA score +
palw_min_trace_retention_daa_v1` (the obligation the chain defines, derived not chosen; a merged
block's own score, not the accepting block's). The invariant is stated as a test that classifies
every field of the struct exhaustively — chain equality, execution replay, derived, or the one
position field — so a field added tomorrow does not compile until it is placed.

## 3. What this costs, stated before it is measured — and the rollout choice

**Rollout (Decision 7, decided 2026-09-02): this rule goes live inside Relaunch 5's re-genesis,
with no fence.** The operator's decision, taken with both paths below on the table: Relaunch 5
was already pending for the A16 re-pin and ADR-0069's certified set, ADR-0073 ③ (the attempt lane
drawn by the chain's own beacon) lands in the same genesis, and a scheduled activation would have
carried dual finalizer arms, dual admission paths and a lane-filtered difficulty window into a
network that is re-minting anyway. So: genesis `bits` resets the network lottery; there is no
version-5 history to validate; the deployment is a rolling swap in the shape of the 08-29
re-mint — the fingerprint moves, so an un-wiped old peer is refused at the handshake and cannot
re-feed the old chain. The activation path stays recorded as what mainnet will have to do, because
mainnet cannot re-mint (2026-08-27 doctrine); the first draft of it (algo id 9 behind a DAA fence,
the algo-6 arm and its envelope-only lottery kept byte for byte, `pwu` re-derived per lane, the
difficulty window restarted on the new lane) was built far enough to know its shape and then
withdrawn from this branch unmerged.

Two ways this rule could go live, and only two:

* **(a) Relaunch 6.** A re-genesis resets `bits` to the genesis value (~20 draws) and leaves no
  version-5 history to validate. Zero code. It is the re-genesis the standing doctrine
  (2026-08-27: consensus changes ship by activation, never by re-genesis; mainnet cannot re-mint)
  says to stop doing.
* **(b) Activation.** A new algo id for the execution-priced lane, gated by a DAA fence, with the
  algo-6 arm and its envelope-only lottery kept byte-identical for history; the difficulty window
  filtered to the new lane, so it restarts from genesis `bits` at the fence the way the chain
  restarts at genesis (fewer than the minimum window of new-lane blocks → genesis `bits`). The
  doctrine's answer and ADR-0066's shape, at the cost of dual arms and dual admission paths, and
  the fingerprint still moves at the swap (the fork-id handshake that would let it not move is a
  later ADR). Roughly the size of this ADR again.

The rule is the same under both; what differs is whether the chain's history survives the swap.
On testnet-11 it does not, by decision; on mainnet it must, by doctrine.

Expected inferences per block per producer = `1 / (P_class × P_bits)`. At the RC genesis the class
target is `2^-1` and the genesis `bits` is about `1/20`, so a producer expects ~40 inferences per
block: at 9 s per QWEN36 inference that is ~6 minutes per block per producer, and the BASE-0 floor
at milliseconds per inference is bounded by its epoch budget rather than by the lottery. The
network's draw rate fell by roughly `2^22`, so the DAA will relax `bits` toward its trivial cap;
at the cap every draw passes the network lottery and the chain's cadence is bounded by the class
lotteries, the share table and the inference supply. That is the intended state: cadence is a
function of how much LLM work the network can do, and nothing else.

What this does NOT do: it does not make real-demand (free-prompt) work the primary lane — the
attempt lane is still the canonical-job lane — and it does not change the receipt lane at all.
Those are the next question, and this ADR is its foundation, because a lottery priced in
inferences is the only kind that can be handed to real work.

## 4. Invariants the tests hold

* `every_priced_field_moves_the_pow_tag`: every field but `challenge` moves the PoW tag;
  `challenge` moves the block identity and leaves the tag exactly where it was.
* `the_anchor_binds_the_execution_to_its_position_and_nothing_inside_a_bucket_moves_it`: every
  anchor input (network, template, class, bond, bucket) moves tag and ticket; three nonces of one
  bucket move neither.
* `palw_v2_commitment_mutation_invalidates_pow`: nonce + 1 and timestamp + 1, honestly re-derived,
  give the same Layer-0 digest; the next bucket gives a different one; a challenge that does not
  match the header is refused on every PoW path.
* `the_class_target_is_what_admits_a_block_of_that_class`: two nonces, one ticket; two executions,
  two tickets; the ticket is not the tag under another name.
* `pwu_separates_cost_from_difficulty`: an 8× tighter target is 8× the pwu at every scale — the
  bucket no longer flattens the curve — and the easiest target is still one execution.
* The integration tests that produce blocks from real executions draw per bucket: a lost draw is a
  second real inference, never a nonce.
* `every_priced_field_is_pinned_or_is_the_challenge`: every field of the attempt is classified —
  chain equality, execution replay, derived, or the one position field — and the derived ones
  derive.
* `a_free_field_inside_the_priced_bytes_is_a_nonce_by_another_name`: sweeping each DA field over
  one execution reaches the lottery for exactly the one derived value; every other value is
  refused at the pin, by name.

## 5. Supersession

| Decision | Status after this ADR |
|---|---|
| ADR-0042 Decision 3a — tag is `Expand(commitment_root_v2)` | tag is `Expand(execution_commitment_v3)`; 3a's guarantee ("a solved header attests exactly one attempt at exactly this position") is kept by the challenge equation plus the derived anchor |
| ADR-0071 Decision 2 — `executions = tries >> k` | withdrawn: `executions = tries`; the bucket `k = 22` stands as the anchor's position field |
| Admission item 6b in the envelope-only list | moved to `check_palw_class_lottery_v3`, called from the composed entry point |
| The three DA fields, producer-chosen | pinned by equality at the composed entry point (`check_palw_attempt_da_pins_v1`); every family derives the manifest the consensus way |

## Security amendment (2026-09-02) — the mainnet activation path, before it is built

**SA-1 — The new lane is seeded, not restarted.** At the fence, every class's new-lane target is
`attempt_target_seed_v1(share, pwu_per_inference)` from the fold at the fence (ADR-0076
Decision 1), and the lane-filtered difficulty window starts from the pre-fence window's last `bits`,
not from genesis bits. A window restarted from genesis is the empty-chain start burst — an issuance
and reorg hazard on a network that cannot re-mint.

**SA-2 — The fork-id handshake precedes the fence.** A node advertises `(genesis, fired fences, next
fence)` and, past the fence height, refuses peers whose fired set differs. Without it the un-upgraded
minority extends the old arm, refuses the majority's blocks, and neither side sees a partition —
the silent-fork lesson this repository has already paid for. It is the precondition of any mainnet
fence, not a follow-up ADR.

**SA-3 — The version check is fenced.** §3's Decision 7 analysis found the attempt version check is
not fence-gated. A mainnet fence requires pre-fence history validated by the old arm and post-fence
blocks by the new, in one binary, pinned by a test that syncs a mixed history from genesis.

**SA-4 — Cross-lane replay is refused by construction**: the new lane's `algo_id` enters the
Layer-0 digest, so an attempt built for the old lottery is not a candidate in the new; a test
asserts a valid version-5 attempt is refused after the fence by algo id, not merely by version.

## Security amendment, second pass (2026-09-03) — what the implementation measured

The first pass wrote SA-1..SA-4 before the code existed. The code was then written, reviewed
adversarially, and the review found the amendment wrong in one place and the implementation wrong in
six. This section is the correction, so that the divergence lives in the ADR rather than only in Rust
comments.

**SA-1 is one rule, and its second half is a constraint on a filter nobody has written.** The seed
half stands verbatim: at the fence a class's new-lane target is
`attempt_target_seed_v1(share, pwu_per_inference)` from the fold at the fence — the *same function*
a post-genesis admission calls, not a second rule that agrees. The implementation briefly carried a
wrapper (`attempt_lane_target_seed_at_fence_v1`) and an identity function
(`attempt_lane_window_seed_bits_v1(a, b) = a`), each with a test asserting what an alias and an
identity cannot fail; both are deleted, because a ceiling nobody reaches is not a rule.

The window half is narrower than this ADR stated. **No difficulty window in this tree is
lane-filtered at all** — nothing in `consensus/src/processes/window.rs` or `difficulty.rs` reads
`pow_algo_id` — and `SampledDifficultyManager::calculate_difficulty_bits`'s short-window fallback
already returns the *selected parent's own* `bits` unless that parent IS genesis. At a fence on a
live chain the selected parent is never genesis, so the pre-fence `bits` is what the fallback yields
for free. The hazard therefore bites only if someone later writes a lane filter that RESETS the
window rather than emptying it. **The rule, for whoever writes it: a lane-filtered attempt window
starts from the pre-fence window's last `bits`, never from genesis bits.**

**SA-3 binds on every peer-facing caller, not just the shape gate.** Fencing
`check_palw_commitment_shape_at` was not enough: `check_palw_carriage_stateless` called the
un-parameterised `validate_stateless_v2` forty-five lines later, so on an armed network below its
fence the fenced gate admitted a legacy envelope and the un-fenced check refused it — Decision 7's
defect reproduced by its own fix. The relay path now takes `PalwAttemptLaneV1::attempt_version()`
(`validate_stateless_v2_at_version`). **Still compiled in at the current version:** the stateful
admission (`check_palw_attempt_admission_full_v2*`). See "What still argues against arming" below.

**SA-4 is a predicate, not a constant, and four sites had to ask it.** "Which id carries the attempt
lane" is `pow_layer0::is_palw_attempt_algo_id`. Four rules were spelled as equality against the old
id and each broke differently at the fence:

| Site | What the stale spelling did past the fence |
| --- | --- |
| `pre_ghostdag_validation::check_palw_carriage_stateless` | fell to `_ => Ok(())`: no envelope binding and **no ML-DSA signature check**, so one solved PoW minted unbounded distinct blocks (launch blockers §5, on the new lane) |
| `kaspad::palw_producer::produce_one` | hard-errored on every template, so **every producer on every node stopped at the same DAA score**, unrecoverably (`PalwRulesetV2::validate` pins the bundle's `algorithm_id` at 6, so no configuration reaches 9) |
| `ghostdag::protocol` blue work | attempt blocks stopped earning ADR-0068 F2's fixed ε and started earning `calc_work(bits)` — fork-choice weight re-priced at one height as a side effect of an id change |
| `virtual_processor::palw_beacon_fact_of_candidate` | the ADR-0044 beacon walk saw the attempt chain stop at the fence; `prev_attempt_daa` froze permanently |

The beacon walk is the one consumer that must accept **both** ids at once, because it descends
through the fence; the other three resolve the lane at a single height. Sites that legitimately keep
naming algo-6 are the ones that mean *this specific algorithm*: the V2 bundle's own `algorithm_id`
(`PalwRulesetV2::validate`, the genesis bundles), and test fixtures that build a V2 network.

**SA-2's gate refuses on the DISPUTED fence, not on any fence.** As first implemented,
`evaluate_fork_id_v1` classified on "has this node crossed anything", and three shipped presets
already schedule crescendo (mainnet 110,165,000; testnet-10/-11 2,125,000). Arming ADR-0072's fence
at a future height therefore refused every un-armed peer from the moment the build started — in both
directions, for the whole rollout — which is exactly the M1-6 deploy-day partition SA-2 exists to
prevent. `fork_id_gate_fences_v1` now names the fences a refusal may rest on (crescendo is not among
them), and a `NextFenceDiffers` mismatch is tested against the height of the fence actually in
dispute.

### What still argues against arming this fence

* **At genesis (`ForkActivation::always()`) the fence is safe.** `LegacyArm` is unreachable, so none
  of the residuals below can occur, and `consensus_identity_id` already separates armed from
  un-armed builds (a genesis-active fence is a rule about block 1, not a schedule) — which is why
  `fork_id_gate_fences_v1` deliberately excludes height 0.
* **At a future height on a chain with real pre-ADR-0072 history it is not.** Two things behind the
  fenced gates still speak only the current rule: the stateful admission's `validate_stateless_v2`,
  and the pre-ADR-0072 lottery arithmetic itself, which was deleted at Relaunch 5's re-genesis. A
  legacy-arm block cannot pass PoW under this binary whatever version its envelope declares, so §3
  option (b) needs that arithmetic restored byte for byte before the fence can be scheduled ahead of
  a live chain. The shipped producer says the same thing from the other side: it stamps the current
  envelope version on both ids it can build for, and cannot build a `LegacyArm` block at all.

## Security amendment, third pass (2026-09-03) — the gate the sweep could not see

The second pass swept every comparison against `POW_ALGO_ID_PALW_COMMITTED_V2` and fixed four. A
fifth gate had the same defect and no such comparison, so nothing in that sweep could find it:
`consensus/pow/src/palw_admission.rs` called the shape rule through its **un-parameterised** entry
point, which supplies `PalwAttemptLaneV1::Unfenced` — that is, "algo-6 is the attempt lane at every
height" — as an omitted argument. Its live caller is `virtual_processor::utxo_validation`, which
runs for every chain candidate, and an `Err` there becomes `StatusDisqualifiedFromChain`.

Reproduced before it was fixed, at that exact call:

```text
UTXO ADMISSION(algo 6) = Ok(NotBound)
UTXO ADMISSION(algo 9) = Err(Commitment(PalwAttemptLaneClosed { algo_id: 9, open: 6 }))
```

On a network armed at genesis every attempt block declares algo-9, so **every chain candidate was
disqualified and the virtual chain never left genesis** — while the blocks still entered the DAG and
relayed normally, so the only symptom was `disqualified from virtual chain`, a line that never names
the fence. No operator configuration could escape it. That falsified the second pass's own
"conditional YES at genesis".

Three things changed, and the third is the one that closes the class rather than the instance:

1. `check_palw_block_admission_v1` takes an `attempt_lane` and uses `check_palw_commitment_shape_at`;
   the UTXO validator resolves it from `palw_attempt_activation` at the header's own DAA score, the
   way the relay gate and the pruning proof already do.
2. The four test-harness sites that stamped a carriage only for algo-6 were widened to the shared
   predicate — including `TestConsensus::build_header_with_parents`, which declares the id from the
   mode cascade, so an armed harness no longer builds algo-6 headers while the template builder
   builds algo-9 ones.
3. **The un-parameterised entry point is now `#[cfg(test)]`.** After (1) it had no non-test caller in
   the tree — the dead-code lint said so immediately — and a consensus gate that wants the shape rule
   must now call `check_palw_commitment_shape_at` and name the lane it means. That is enforced by the
   compiler rather than by the next sweep.

The evidence base is no longer only unit-level: `palw_v2_a_genesis_armed_attempt_lane_builds_a_chain`
boots a `ConsensusV2` network with `palw_attempt_activation = Some(ForkActivation::always())`, builds
blocks through the real pipeline, and asserts the sink leaves genesis on an algo-9 header. Without
the lane threaded into admission it fails with no body tip ever reaching `StatusUTXOValid`.

**Still not proven, and still fatal above genesis.** The harness runs under `skip_proof_of_work`, so
the algo-9 finalizer arm is not priced end to end here, and no *deployed* devnet has yet booted armed
at genesis. The `LegacyArm` residuals of the second pass are unchanged: the stateful admission
(`palw_admission_v2`) still validates at the compiled-in envelope version — correct for `Unfenced`
and `ExecutionArm`, wrong for `LegacyArm` — and the pre-ADR-0072 lottery arithmetic those blocks were
mined under is still deleted.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0072-the-ticket-is-the-execution.md -->

</details>

<a id="source-0074-the-attempt-is-a-claim-drawn-by-the-chain"></a>

<details>
<summary>0074-the-attempt-is-a-claim-drawn-by-the-chain.md — 原文 L1–L214</summary>

[原文を開く](../adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md) · SHA-256: `3cb1e097036734d8daf6d3d4b587d37c296171bf76dea9604140f16f973d9cc1`

<!-- ADR_SOURCE_BEGIN:0074-the-attempt-is-a-claim-drawn-by-the-chain.md -->
~~~~~~~~markdown
# ADR-0074: The attempt is a claim, drawn by the chain

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **New verification-challenge scope — 2026-10-08:** The chain-derived/no-attestation principle remains. [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) owns the new conformance/algebraic challenge source and seed. Its qualifying future independently Final useful work is not the legacy first-algo-6 ticket/Panel draw or a cheap fallback floor. Do not inherit unpredictability from algo id or inference cost alone, or reinterpret this ADR's legacy seeds. Source scarcity leaves the new checks pending while current heartbeat/BASE-0/transaction liveness continues; no BFT/DNS beacon authority is added.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


> **Status amendment (2026-09-18, ADR-0137):** past `Params::palw_work_target` — testnet-11's DAA 6,001 flag day —
> a class's *share* is a **result**, not a lottery input. The class target, the class DAA, the epoch budget, the
> admission-derived share and the seat price this ADR specifies are **not read** past the fence: a block draws
> against `CCU / W` (one network-wide work target) and the readers report the rolling share of finalized work.
> Below the fence the text stands as the rule it was; see ADR-0137 §3–§5 and §22.

**Status:** PROPOSED (2026-09-02), decided by the user on two points that this ADR only spells
out: (1) the attempt lane adopts the beacon draw (ADR-0073 Decision 3c's open option), and
(2) **the beacon is a fact derived from the chain a node already holds — never an attestation, a
quorum, a validator set, or a finality overlay.** The last beacon this project built was wired to
validators and the design became BFT-dependent; this one is forbidden from reading anything a
validator says. Ships in the Relaunch 5 re-genesis (5d) together with ADR-0072 + D8 and
ADR-0073 Phase ①, deployed as a rolling host swap (the fingerprint moves, so un-wiped peers are
refused at handshake and cannot re-feed the old chain).
**Builds on:** ADR-0044 (the receipt lane: commit, beacon, quantum, spend), ADR-0072 (+ D8: the
self-drawn ticket and its pins), ADR-0073 (court → unit → weight → share).
**Amends:** ADR-0073 Decision 3 (this is the "one unit" and the "lottery discipline" made
concrete), ADR-0044 Decision 7 (CU weights withdrawn), ADR-0071/0072's premise that the attempt
lane is the primary lane.

> **Landed, then amended (index reconciliation, 2026-09-02).** §7 records what landed; Decision 5
> is as landed (`7676d461`: the quantum is an eighth of the class's own canonical job). Decision 6
> is amended by [ADR-0075](0075-certification-is-a-consensus-object.md) (the certified set is chain
> state; a class's free-prompt lane is opened by `ClassLaneCertified`), and
> [ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) seeds the attempt lane's
> per-class target on top of this ADR's draw. Map: [`README.md`](README.md).

## 1. Why a beacon, and why not a validator

The ADR-0072 review found that a ticket drawn from the executor's own hash is only as strong as
every field inside it being pinned; one unpinned u64 made one inference worth 2^64 draws. D8
closed the field, and the invariant ("every priced field is an equality against chain state, a
function of the execution the panel replays, or the position") is now a test. But the receipt
lane has no such surface at all: a claim is fixed on chain first, the beacon that draws it does
not exist yet, and the ticket is `H(beacon ‖ claim ‖ quantum)`. Nothing the executor controls is
inside the draw. That is the discipline both lanes are held to from here: **a draw is a paid
execution, drawn from a value the executor cannot vary after paying.**

A beacon is only as good as its independence from every party that wants a particular value.
The receipt lane's beacon is the first self-drawn (algo-6) chain block at or after the claim's
slot, derived by walking the chain (`derive_beacon_fact_v3`) and validated by two inequalities
(`validate_beacon_fact_v3`): its producer paid an inference for the block, so grinding a beacon
costs an inference per candidate, and its identity is a fact every node that validates the chain
computes for itself. Nobody signs it, nobody votes on it, nobody is asked. **A node that can
validate the chain can derive the beacon; a node that cannot has no business licensing a block.**
The validator-wired beacon of the past failed exactly here — it made block licensing depend on a
liveness assumption about a set of signers — and this ADR names that as the thing it forbids.

## 2. Decisions

**Decision 1 — Every paid unit of LLM work is a claim committed before its draw.**
The claim is the receipt lane's 0x4a commitment (`PalwFreePromptCommitmentV3`), in one of two
prompt modes carried on the job:
* `User` — the prompt is the user's; the ids travel in the payload (PublicDA) and in the job
  material, hash-bound to `prompt_token_ids_hash`. Today's lane, unchanged.
* `Canonical` — the prompt is the family's canonical prompt for the claim's own anchor,
  `fp_canonical_anchor_v1(job) = H(domain ‖ network ‖ class ‖ bond ‖ anchor_block ‖ anchor_daa ‖
  job_nonce)`, derived by the same `job_for_anchor` the attempt lane uses; the ids do NOT travel
  on chain (they are a pure function of the job) but DO travel in the job material, so seats and
  courts take one path for both modes. `prompt_token_ids_hash` must be the derived prompt's hash
  and `prompt_tokens` its length; a seat checks by deriving. Canonical claims are the work queue
  when nobody is asking: an executor keeps earning by running the network's own jobs, and those
  jobs are drawn, priced, verified and tried exactly as a user's.

**Decision 2 — The draw is the chain's beacon.** `fp_quantum_ticket_v3(network, beacon, claim,
quantum) ≤ receipt_target[class]`, with the beacon derived by `derive_beacon_fact_v3` — the
first algo-6 chain block at or after `final_daa + receipt_maturity_daa`, witnessed by the last
algo-6 block before the slot. Stated as law, because it was broken once: **the beacon MUST be a
function of the chain's own blocks and MUST NOT read an attestation, a quorum, a validator set,
a finality overlay, a DNS or MTP or VLT fact, or any value one party can set.** The two
inequalities are the whole validation; a node with the chain has everything.

**Decision 3 — The self-drawn lane stays, as the liveness-and-beacon lane.** Algo-6 blocks are
not retired: they are what makes beacons exist (the floor produces them at milliseconds per
inference, so a class with no self-drawn producer still gets beacons) and what makes beacons
ungrindable (a candidate beacon costs an inference under ADR-0072 + D8). Their share moves to the
claim lane under ADR-0073 Decision 4; their weight is unchanged by this ADR.

**Decision 4 — One inference, one claim.** A claim's work identity is
`H(domain ‖ class ‖ prompt_token_ids_hash ‖ decode_tokens_executed ‖ executor_bond)`; a
commitment whose work identity is already held by a claim that has not retired is refused at the
transition. Canonical claims are unique per (bond, nonce) by construction. What this prices: one
execution committed under N nonces, N retention values or N anchors was N lottery entries at the
cost of hashing. What it does not price: two bonds sharing one set of tiles (each re-hashes the
leaves under its own context) — collusion that costs each party a fee and `reserved` exposure per
claim, recorded here as the residual; it is closed by executor-dependent computation if it is
ever measured to pay.

**Decision 5 — The price is the capture's leaf count, and a quantum is a fraction of the
class's own job.** The commitment carries `work_leaves`, which must equal the binding's
`step_leaf_count`. The quantum is not a network-wide leaf count: it is
`max(1, canonical_leaves / quanta_per_canonical_job)` of the class's own canonical job (the
bundle says `8`; `canonical_leaves` is the class's `pwu_per_inference`, its canonical step leaf
count by genesis rule), so `quanta = min(⌊work_leaves / quantum⌋, max_quanta)` and
`pwu = quanta × quantum`. A job the size of the canonical one is eight draws on every class — the
per-class receipt retarget normalises across classes anyway — while `pwu` stays in leaves, so a
floor claim and a QWEN36 claim weigh in one unit. The class state holds no profile, so the
transition prices from the class's rule and the bundle's `quanta_per_canonical_job`
(`PalwStateParamsV2::with_fp_quanta`, held equal to the free-prompt params by `validate()`).
`PalwFpCuWeightsV3`, `fp_cu_v3`, `QUANTUM_CU` and `PWU_PER_QUANTUM` are withdrawn (ADR-0044 retains the withdrawal reason). A free-prompt claim's exposure is its own `pwu` — the work it
claims — not a whole canonical job. The attempt lane's
pwu is already leaves (`pwu_per_inference = canonical_step_leaf_count` by genesis rule), so
`safe_weight` becomes one unit and ADR-0073 Decision 3 is discharged. The seat verifies the price
against the capture it has authenticated (`verify_material` → `capture_shape().step_leaf_count`),
not against a declared shape: the class state holds no profile, only the family can count leaves,
and a claim whose price the seats refuse never licenses — the same standing the roots have had
since ADR-0044. Shape-crafting is gone by construction: leaves are the work.

**Decision 6 — The floor's free-prompt lane bears weight** (ADR-0073 Decision 2 for
PALW-BASE-0): `palw_rc_fp_certified_families_v1` gains the floor from its drill
(`the_floor_free_prompt_lane_certifies_…`). QWEN25-A16 and QWEN36 join when their free-prompt
paths exist and drill (the registered A16 graph refuses `execute_free_prompt`; QWEN36 has none).

**Decision 7 — Rollout.** Job version 3 → 4 (the mode field and the price field are inside the
job and the commitment), the bundle's free-prompt params change shape, the certified free-prompt
set is non-empty: three fingerprint moves, folded into ONE re-genesis with ADR-0072 + D8 and
ADR-0073 Phase ①. Rolling swap per the Relaunch 5 runbook; the new testnet-11 fingerprint is read
from the pin test in an isolated worktree and handed to the fleet operator before any host moves.

## 3. What this costs, stated before it is measured

* **Cadence.** Unchanged in mechanism: claims draw per quantum against `receipt_target[class]`,
  which the per-class retarget already maintains for the receipt lane; canonical claims add
  supply to that lane, so the silent-lane renormalisation in `apply_class_retargets` stops firing
  for classes with an idle executor.
* **Latency to a licensed block** for a claim: bind + receipt + `receipt_maturity_daa` +
  `use_window` — hours at the frozen 120 s cadence, as today for user claims. The attempt lane
  keeps the chain moving in between (Decision 3).
* **Seat cost** is Phase ①'s (hashing plus sampled leaves); pricing adds one equality.
* **State.** One `BTreeMap` of work identities keyed to claims, released at claim retirement.

## 4. Invariants the tests must hold

* The beacon derivation reads only `PalwChainBlockFactV3` items (block, DAA, algo id) — no other
  input type exists on that path, and a test asserts the function's signature by construction.
* A commitment whose work identity is live is refused; the same commitment after the earlier
  claim retires is admitted.
* A canonical claim whose `prompt_token_ids_hash` is not the derived prompt's hash is not
  certified by any seat; one whose hash is derived certifies through the Phase ① path.
* `work_leaves ≠ capture_shape().step_leaf_count` → the seat refuses; equal → `Valid`.
* One floor free-prompt job and one floor canonical claim price in the same unit as one floor
  attempt: `pwu` values are comparable by ratio of leaves, not by lane.

## 5. Supersession

| Decision | Status after this ADR |
|---|---|
| ADR-0044 Decision 7 — CU weights price the lane | withdrawn (Decision 5) |
| ADR-0044 "only attempt-class blocks can be beacons" | kept and named as law (Decision 2) |
| ADR-0072 §3 "the attempt lane is still the canonical-job lane" | the canonical job is a claim too (Decision 1); the attempt lane is the liveness-and-beacon lane (Decision 3) |
| ADR-0073 Decision 3 (a, b, c) | discharged by Decisions 5, 5 and 2 respectively |
| ADR-0073 Decision 2 activation gate | met for PALW-BASE-0 (Decision 6); open for A16 and QWEN36 |

## 7. What landed (2026-09-02, branch `palw-adr0073-fp-weight`)

* **Decision 1:** `PalwFreePromptJobV3::prompt_mode` (`PALW_FP_PROMPT_MODE_USER` / `_CANONICAL`),
  `fp_canonical_anchor_v1`; the chain refuses a canonical payload that carries ids; seats, the
  court arm and the challenger derive and verify the canonical prompt (`fp_prompt_for_job`). The
  work queue is the node panel's: `--palw-canonical-claims`, `--palw-canonical-class`,
  `--palw-canonical-interval-daa`, funded from `--palw-fee-outpoint`; the commitment is assembled
  from the capture's own context (`palw_fp_commitment_from_context_v3`) and the `FPC1` material is
  retained and broadcast under the claim id.
* **Decision 2:** no code change — `derive_beacon_fact_v3` is the beacon; this ADR is its law.
* **Decision 3:** algo 6 unchanged (ADR-0072 + D8 on main).
* **Decision 4:** `fp_work_id_v1`, `PalwClaimStateV2::work_id`, the state's derived work-id index,
  `DuplicateWork`.
* **Decision 5:** `work_leaves` replaces `cu`; `fp_class_quantum_leaves_v1` (⅛ of the class's job,
  `PalwPwuRuleV2::canonical_leaves_v1`); the transition prices from `PalwStateParamsV2::with_fp_quanta`
  and the bundle's `validate()` holds the two readers equal; exposure = the claim's own pwu; the
  seat's pricing check is `capture_shape().step_leaf_count == work_leaves`; worker results carry
  `step_leaf_count`; CU weights, `QUANTUM_CU`, `PWU_PER_QUANTUM` withdrawn.
* **Decision 6:** `palw_rc_fp_certified_families_v1` = {PALW-BASE-0}, pinned from the floor's own
  drill; presets carry `with_fp_certified_classes`; `FreePromptLaneUncertified`.
* **Found on review (ac), closed:** the free-prompt lane never had admission item 8 — a
  commitment meets no admission list, and `reserve_for_claim` only added — so a bond could pile
  free-prompt reserves past its collateral, and Decision 5 made each claim up to eight jobs'
  worth. The ceiling is now the transition's (`PalwStateParamsV2::with_fp_exposure_ceiling`,
  `FreePromptExposureCeiling`), the same expression as the attempt lane's item 8, held equal to
  the admission ratio by the bundle's `validate()`; every preset carries it.
* **Decision 7:** FP wire version 3 → 4, `PALW_STATE_V2_VERSION` 14 → 15, golden ids and roots
  re-taken; the preset fingerprints are re-pinned on the re-genesis build (rebased on main's
  ADR-0072 + D8) and handed to the fleet operator before any host moves.
* **Outside this repo:** MISAKA Studio's worker request must carry `prompt_mode` (wire version 4),
  and its rail must pass the class's canonical leaves to `build_fp_commitment_tx`.

* **Decision 6, continued by ADR-0075:** the free-prompt-certified set gains a chain half —
  `FamilyCertified` / `ClassLaneCertified` objects graded by the court in the transition — and
  the genesis half is derived by kernel coverage from fixture free-prompt drills of all three RC
  families, so the real models take free-prompt claims from the 5e genesis and every later model
  takes the on-chain route (`docs/adr/0075-certification-is-a-consensus-object.md`).

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0074-the-attempt-is-a-claim-drawn-by-the-chain.md -->

</details>

<a id="source-0141-can-an-inference-be-the-ticket-without-a-hash-lottery"></a>

<details>
<summary>0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md — 原文 L1–L136</summary>

[原文を開く](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) · SHA-256: `d33f93e4adf64c9f3bde871d3df21b7bc2703a2d43da4c833485aaf6b361776e`

<!-- ADR_SOURCE_BEGIN:0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md -->
~~~~~~~~markdown
# ADR-0141 — Can an inference be the ticket without a hash lottery?

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED 2026-09-18 on `feat/palw-exec-lane-and-validator-retirement`. **This ADR decides
nothing about the lottery, changes no rule and builds nothing.** It states the question precisely,
records why it is the one place a hash is still worth reopening, lists what would have to be answered
before anything replaced it, and specifies the counters that would settle the first half of the
argument — to be built when the 7,101 rollout is behind us, not before.

**Builds on:** ADR-0071 (the attempt lane's price and the ticket's bound), ADR-0072 (the ticket is
the execution), ADR-0076 (the class target seed), ADR-0117 (one forward, one draw), ADR-0132 S (the
single lottery), ADR-0137 (one work target `W`), ADR-0140 §6 (the inventory this follows from).

**Why this one and not the heartbeat.** ADR-0140 keeps the heartbeat's hash: it prices a lane that
should be idle almost always, and swapping it for a sequential-work primitive would buy a new
cryptographic dependency for something that runs when the lights are out. The hashing that actually
shapes *who produces blocks* is here, on the economic lane, and it is the only one of the three
worth reopening.

## 1. The question

A producer runs one deterministic inference, commits its execution, derives a ticket digest, and the
block is admitted when that digest falls under a target. Past 7,101 the target is `CCU / W` — the
class's counted compute against the network's one work target — and the Layer-0 digest is no longer
compared to `bits` at all (ADR-0132 S). The comparison is still a comparison against a target.

So: **the producer has already paid for the inference before the comparison happens.** Most draws
lose, and every losing draw is a forward pass the network bought and discarded. The lottery is not
grinding — ADR-0117 fixed one forward to one draw, so a producer cannot search — but it is still a
target comparison, and its job is rate control rather than weight.

Weight does not come from it. An attempt block's fork-choice work is a constant `2²⁰`, set by the
lane and not by the draw. The bond is what stands behind the claim. ADR-0071 §1 records the residual
coupling in its own words: the quantity that ought to describe LLM work is derived from a hash
lottery.

Hence the question in the title. If rate control is the only job, something that is not a lottery
could do it:

```
today        eligible producer → one inference → digest → digest < target → block
the question eligible producer → one inference → valid proof → block
```

## 2. Why this is the biggest of the three, and still not first

ADR-0140 §6 inventories the three surviving uses of a hash: identity and commitments, the
heartbeat's spam price, and this. The first is not going anywhere and nothing proposes it should.
The second is kept deliberately — it prices the emergency generator, which should be asleep. This
one prices nothing and secures nothing: it meters, on the lane that decides who gets paid.

It is nevertheless not the first thing to change, for a reason that has nothing to do with its
difficulty. **Removing the lottery means answering "who produces slot N?", and that question has no
small answer.** Everything the lottery currently makes moot comes back at once:

* a producer that is offline when its turn arrives,
* several producers eligible for one slot, and the DAG concurrency that follows,
* grinding of producer identity to land on favourable slots,
* Sybil bonds buying eligibility,
* manipulation of whatever beacon assigns turns — and ADR-0074's beacon is retired for exactly this
  class of reason, so a new randomness source would need its own justification,
* how a class's share becomes a number of slots,
* the receipt lane, which is scheduled against the attempt lane,
* how a newcomer with a fresh bond gets its first turn without an invitation,
* censorship, when the eligible producer is the one refusing a transaction,
* an empty slot, and what the chain does with it.

That is ADR-0071, ADR-0072 and ADR-0076 rewritten together with the class economy. It is a season of
work, not a change.

## 3. What would have to be true first

**M1. The waste has to be measured, not assumed.** The argument for removing the lottery is that it
burns inference. Nobody has counted it. Required, per class and fleet-wide: draws per accepted
block, forwards spent on losing draws, wall-clock between winning draws, and the gap between a
class's bonded share and its realised production share. If the waste is small, the whole question is
an aesthetic one and should be closed.

**M2. A replacement must not reintroduce grinding.** ADR-0117's "one forward, one draw" is what makes
the current lane honest. Any eligibility rule has to be a function of facts a producer cannot cheaply
vary — and "cheaply" has to be measured against the cost of one forward pass, which is seconds, not
microseconds.

**M3. A replacement must survive an offline producer without a silence condition.** ADR-0064 Fact A
binds here exactly as it binds the clock: a branch cannot prove that the producer whose turn it was
did not produce. Any "if the scheduled producer is absent, then…" rule is unlockable by anyone. This
is the constraint that kills the obvious designs, and it should be met before a design is drawn, not
after.

**M4. It must not need a new beacon.** ADR-0074's beacon retired. A scheme whose first requirement is
"a fresh randomness source" is proposing that retirement be reversed, and must say so.

**M5. ADR-0140's four claims must be in force first.** Slot assignment is stated in time, and any
scheme that changes who may produce also changes what the heartbeat lane sees. C1 to C5 hold the line
between the two lanes; reopening this one while that line is only believed rather than guarded would
mean debugging both at once.

## 4. Decision

**D1. The lottery is not changed, and no design is adopted here.** The question is recorded, its
cost stated, and its preconditions named.

**D2. The waste gets counted before it gets argued about.** M1's counters are specified here and
built after the 7,101 rollout: draws, wins, inference spent on losing draws, and the longest run of
consecutive losses, which is the wait a producer actually feels and which an average erases. They
cost nothing, they are useful to an operator regardless of this question, and no rule reads them.
Until they exist, every claim about how much this lottery wastes — including the ones in this
document — is an estimate.

**D3. This ADR does not merge into ADR-0140.** The clock and the lottery are separate mechanisms
with separate failure models, and a single document covering both would let a conclusion about one
carry a conclusion about the other. That is the specific mistake this pair of ADRs exists to avoid.

**D4. Nothing here is scheduled.** No fence, no height, no dependency for the 7,101 bundle.

## 5. What this ADR does not decide

Whether an inference can be the ticket. That is the title, and the honest answer today is that the
data to argue it does not exist yet. D2 creates the data.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md -->

</details>

<a id="source-0170-the-seed-anchor-is-a-window-not-a-span"></a>

<details>
<summary>0170-the-seed-anchor-is-a-window-not-a-span.md — 原文 L1–L147</summary>

[原文を開く](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md) · SHA-256: `03f4e8bf29af7051b1aac13ce313286c88bba25f1d62f87372d94ca119426e1f`

<!-- ADR_SOURCE_BEGIN:0170-the-seed-anchor-is-a-window-not-a-span.md -->
~~~~~~~~markdown
# ADR-0170 — The seed anchor is a window, not a span: a merged attempt anchors, the anchor survives span boundaries, and the admission jury and the schedule seeding read the latest anchor of `S − 24 … S − 1`

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED 2026-10-03 on `anchor/window` (lane P2), for the DAA-5,300 flag day, the user's decision of the same evening
(variant "M1 + M2 + M3, W = 24"). Consensus change behind ONE fence, **dormant on every shipped preset**:
`Params::palw_anchor_window_v1`, a bare height whose companion values (`W`, `M1`) are hashed with it. Amends **ADR-0130** (what a seed
anchor is and how old a reader may take it) and **ADR-0147** (the jury's randomness and its population cut). Touches nothing else:
**SW-8's binding rule, ADR-0125's permit ledger, the audit period and its stagger, and every window of the panel are as they were.**

**Builds on:** ADR-0125/0130/0151 (the execution lane, its seed anchor, the economic-safety rotation), ADR-0147 (the admission jury),
ADR-0152-adjacent R2 (the staggered audit), ADR-0165 (the reserve: the reason this is needed now), ADR-0058 (merged work, blue or red).

## 0. The sentence this ADR is

**A chain block's own attempt used to be the only thing that could anchor a span, and under ADR-0165's reserve it almost never has an
attempt; the anchor is now the latest admitted attempt the fold took — its own or one it merged — kept until a reader that wants it
within 24 spans has used it.**

## 1. The finding (P2, 2026-10-03; evidence `lanes/evidence/head-admission-slip-1003/p2-anchor/`)

`round_seed_anchor` is recorded by exactly one thing — a chain block whose OWN header carries an admitted attempt (step 4's `Ok` arm) —
cleared at every span boundary, and read by exactly two:

* **ADR-0147's admission jury** (`admission_jury_v1`): an audit needs the anchor of span `S − 1`; without it no jury sits and the audit is
  SKIPPED, not deferred, until the class's next period (100 DAA on testnet-12);
* **the schedule seeding** (`rotate_round_lane`): one due snapshot a span, seeded only by the anchor of the span before; a snapshot not
  seeded within `PALW_EXEC_PENDING_GRACE_SPANS_V1` (64) spans of its target is dropped, and its Finals' execution rights with it.

While floors flooded the chain this was true of 98.7 % of testnet-12's spans (297 of 301 in the last 300 DAA). The int-11 drill's head class
was admitted one audit period late for the plain reason that span 90 held two heartbeats and no attempt (19.5 % of that drill's spans had
none). ADR-0165's reserve refuses a floor attempt while real work flows (`FloorNotIdle`: skipped by the fold, never a claim, never an
anchor), and a REAL attempt is almost never a chain block (it is merged, blue or red, by the heartbeat that took another selected parent:
0.5 % of the replay's chain blocks). Replaying testnet-12's last 300 DAA under the reserve (K = 20, all producers compliant):

| | today's rule | M1 | M1 + M2 + M3, W = 8 | **W = 24** |
|---|---|---|---|---|
| spans that carry an anchor | 2–3 % | 22–24 % | — | — |
| an audit seats (per period) | **2.7 %** | 24 % | 84 % | **100 %** |
| snapshots seeded (exec lane) | **34 %** (credits 41 %) | 99 % | 99 % | 99 % |

(Idle/Probe/Normal 20/8: 2.0 % / 24 % / 83 % / 100 %, exec 25 % → 99 %.) Every number carries replay #2's stated weak assumptions: one DAG,
exogenous REAL cadence, 72 REAL attempts. The measurement of the carried anchor's age at an audit: p50 3, p95 15, max 20 spans.

## 2. The rule

Past `palw_anchor_window_v1` (resolved at the folding block's DAA, the mirror `PalwStateParamsV2::anchor_window_from_daa`):

* **M1 — a merged admitted attempt records the seed anchor.** In step 4b, after a merged attempt (blue or red) is admitted, the fold
  records `PalwExecSeedAnchorV1 { span: the folding block's span, block: the attempt's own block, execution_key: the attempt's own }`.
  Own work first (step 4), then the mergeset in consensus order; the last admitted attempt wins, as the last chain block of a span always did.
  A refused attempt records nothing. The flag is `PALW_ANCHOR_WINDOW_MERGED_V1` and is hashed with the fence: `false` is the conservative
  variant (own anchors only), whose window is then `PALW_ANCHOR_WINDOW_SPANS_V1 = 64` (P2's replay: 83 % of audits seat on 64 spans of own
  anchors where 24 seat 41 %) — one line each, and a new network id.
* **M2 — the anchor is not cleared at a span boundary.** It keeps the span it was recorded in; every reader checks its age. No state field,
  delta or carriage tail is added: `round_seed_anchor` is reused (it is simply `Some` for longer), so the root and the carriage differ from a
  chain without the fence only in WHEN the anchor is `Some`, from the fence's height on.
* **M3 — the readers take the latest anchor of the window `S − W … S − 1`** (`W = 24`, `palw_anchor_window_admits_v1`):
  * the **admission jury** draws from `palw_admission_jury_seed_{v1,v2}(class, S, anchor)` as before, with the population cut at the ANCHOR's
    span (`palw_anchor_window_jury_cut_span_v1`: bonds registered before the span the randomness was recorded in began), so a bond registered
    after the seed existed is never on the jury it seeds. The audit's own span — once a period, at the class's stagger — is untouched, so there
    is no deferral and no re-roll, and P3's proof-timing adapter (`palw_candidate_audit_due_v1`) is unchanged. **The Activation Pool's (a)
    keeps its meaning**: it pays a drawn juror whose readiness proof LANDED before the seed existed — "two spans before the audit" for the
    anchor of the span before it, as always (review M6) — and so, past the fence, `min(S − 2, anchor.span − 1)`
    (`palw_anchor_window_prep_landed_by_v1`; the jury carries its anchor's span): a juror that can see the seed knows whether it is drawn,
    and a proof sent after that is no preparation. P3's first send (`S − 22` or `S − 21`, landing about `S − 20`) precedes every anchor
    of the window but the oldest;
  * the **schedule seeding** seeds a due snapshot with the anchor only if it was recorded at or after the span the snapshot was taken in
    (`target − 1 − maturity_spans`, `palw_anchor_window_seeds_snapshot_v1`): ADR-0130's "participants first, randomness after". On testnet-12
    the 120-DAA maturity already implies it for `W ≤ 120`; it is asserted rather than assumed. One snapshot a span, oldest first, the 64-span
    grace: as before.

## 3. What it costs, stated (the judgments this ADR asks for)

1. **ADR-0130 — what moves an anchor.** Moving it costs an admitted attempt: while the reserve is closed a REAL inference of an Active class,
   while it is open a bonded floor. That is the property the lane was built on and it holds. **The new freedom** is WHICH of the attempts a
   merging block takes is the last: a choice among attempts that are already public, by a block producer who already chooses its parents —
   not a free re-roll of a value. A producer that can make a cheap admitted attempt (a tiny Active class: ADR-0165's idle ledger has the same
   weakness, a cheap REAL attempt keeps the floor closed) moves the anchor at that cost; the fence adds no cheaper way than the floor's was.
2. **ADR-0147 — the jury's lead time.** The jury's seed is known `S − anchor.span` spans before the audit instead of one: p50 3, p95 15,
   max 20 spans (about an hour at the worst, W = 24). Its population is fixed before the seed (the cut above), so the registrant gains only
   time to make drawn operators hold the class — which they can do only by holding it, a proof signed with their own bond key.
3. **ADR-0125/0130 — the schedule's privacy.** "A span's producers are public for one span, not before" weakens to "the seed is public up
   to W spans before"; the schedule also needs the frontier at the span's opening, which moves when Finals mature. Participants are still
   fixed by the snapshot, which is taken at least `maturity_spans` before.
4. **SW-8 is untouched.** A claim still binds only in its own anchor block (the first chain block at or past its slot that is or merges an
   operator's attempt); a claim whose anchor block does not bind it still voids there; the anti-grinding argument is as ADR-0152 wrote it. Under
   the reserve a claim WAITS for the next operator attempt (P2: 6 / 16 / 21 DAA past the slot at p50 / p95 / max on the replay; no void, REAL
   or floor, operator or external); the hole that remains — external REAL keeping the state Normal while no operator attempt appears — is
   lane RS's R1 (an operator floor producer that holds only for the idle policy mines one binder floor when a claim has waited 30 slots).

## 4. The fence

`Params::palw_anchor_window_v1: Option<ForkActivation>`. Written in the four places of ADR-0165's fences: the field; `for_each_fence`;
Some-only in `consensus_params_id` and `consensus_schedule_id` with `palw_anchor_window_value_v1()` = `[24, 1]`; the `never()` collapse. The
bundle's mirror `PalwStateParamsV2::anchor_window_from_daa` is written by `Params::sync_palw_anchor_window_v1` (called from
`palw_v2_params_on_base` and by the entry's `set`). `validate_palw_v2` refuses it, by name, off ConsensusV2, with the mirror unsynced, and
without the execution lane (`palw_execution_lane`), the economic-safety bundle (`palw_economic_safety`), ADR-0147's jury
(`palw_admission_independence`) or the model registry (`palw_model_registry`) at or below it — all in force from genesis on testnet-12. Its
check runs LAST on both paths of `validate_palw_v2`, after every other fence's own, so a prerequisite it shares with another fence is named
by that fence's refusal first (the other fences' prerequisite tests need no edit).
Its entry `PALW_T12_ANCHOR_WINDOW_ENTRY` is the 24th of the 29 entries of `PALW_T12_INT11_FENCES_V1`, right after `palw_real_clock_tick_v1` and before
`palw_capacity_network_verify` (the capacity entries stay the tail): the DAA-5,300 flag day, and `--palw-drill-int11-at` moves it with the
list, so there is no drill flag of its own. Every shipped preset and `palw_t12_release_v5_params` (the DAA-3,600 release) leave it `None`
and fingerprint byte for byte as before.

## 5. Tests

* `consensus/core/src/palw_anchor_window_v1.rs` — the window's arithmetic, the population cut, the snapshot rule, the pool's landing bound,
  the companion values;
* `consensus/core/tests/palw_anchor_window_fence.rs` — dormant on every ruleset, armed moves params and schedule ids and never the identity, the
  fork id gates it, refused without its prerequisites or with the mirror unsynced;
* `consensus/core/tests/palw_t12_flag_day_int11.rs` — the list is 29 entries, in order, with the window's prerequisites refused by name;
* `palw_state_v2/tests/anchor_window_v1.rs` — M1 (merged anchors; the last admitted wins; a refused one records nothing; below the fence none),
  M2 (survives span boundaries), M3 on the schedule rotation (seeds a snapshot the old rule would have left waiting; older than the snapshot
  seeds nothing; W's far edge), and **byte identity before the fence** (the same chains, a fence at a height never reached: the same root at
  every block);
* `palw_state_v2/tests/adr0135/admission_independence/anchor_window_v1.rs` — the audit seats when the only admitted attempts of the window are
  merged (and not below the fence), the anchor survives to the audit, the window's 24-span edge, the population cut at the anchor's span,
  byte identity before the fence;
* `consensus/src/pipeline/virtual_processor/tests/t12_anchor_window.rs` — testnet-12's own GHOSTDAG (the real pipeline: template, GHOSTDAG
  colouring, fold, state store) with the reserve armed from genesis, floors held (none is mined) and the 8k class's REAL attempts flowing
  (two a period, each templated 340 s before the heartbeat that merges it, a side block and never a chain block): with the fence the head
  class (planted `Candidate`, fresh readiness on all eight operators) is admitted AT ITS FIRST AUDIT (span 130: the REAL attempts merged at
  spans 113 and 124 anchored, the anchor of span 124 seeded the jury, the class left `Candidate` for `Prefetching`); under the reserve alone
  the same traffic leaves no seed anchor at either audit (spans 130 and 230) and the class stays `Candidate` through both — the int-11
  drill's skipped audit, reproduced and closed. (The harness runs no panel, so a REAL claim never resolves and the panel's replay room, five
  claims of the class, bounds the traffic; the live chain's supply of anchors is far denser.)

## Mission alignment amendment — 2026-10-07

seed windowはassignment/admission randomnessの規則であり、public proofの提出資格や算術真実の承認ではない。anchorがない場合の通常処理と、独立した外部courtの受理を混同しない。

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0170-the-seed-anchor-is-a-window-not-a-span.md -->

</details>

