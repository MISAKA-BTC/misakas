# ADR-0071 — The attempt lane's price, the ticket's bound, and who may judge a class

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
