# ADR-0160 — Claim capacity is separated from collateral price (v3): the bond is one shared guarantee split four ways — liability, issuance, fork-choice cap, network share — ρ is a risk tier, and a credited claim is paid only after an independent audit

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* Status: **PROPOSED v3 (2026-09-26 night).** Amends v2 (`93395bafa`, same file) in place, answering the
  second review of v2 (twelve findings; each is fixed or rebutted with evidence, Appendix D). The
  v2 → v3 and v1 → v2 changelogs are Appendix B; v1's summary is Appendix C. **Nothing in v3 rides the
  DAA-750 release.** Every consensus rule sits behind a t12-only dormant fence (§10), armed at one
  common height `H_cap` ∉ {500, 750, 1,000}, not a multiple of 250. The one node-only lane (shadow)
  ships first and moves no verdict.
* Date: 2026-09-26
* Base: public testnet-12 runs node update `8a0810992`, consensus params fingerprint `b8564b88…`.
  An unqualified `file:line` is at v2's base `40b23d64d` (= `rcore/cap-spec2`'s code, whose references
  the reviewer reproduced); code that exists only on the newer DAA-500 release line is cited as
  `rcore/int-4@a067f490d:path:line`; a lane reference is `branch@head:path:line`.
  Lane heads read for v3: weight `3aa4abec4`, escrow `d6a058249`, liab `deccda81b`, verify
  `29b1cb442`, shadow `151f48b0c` (the v1 fix-and-integrate workflow's current heads).
* Branch of this document: `rcore/cap-spec2` (worktree `~/Downloads/MISAKA-wt-b/wt-cap-spec2`).
  v1 lanes: `rcore/cap-{weight,escrow,liab,verify,shadow}`, integrating on `rcore/cap-int`.
  v3 lanes (§11): `rcore/cap-{d,q,s,b,n,m}`, each branched from `rcore/cap-int` after the v1 fixes merge.
* Numbers: every figure is reproduced by `docs/adr/0160-capacity/capcalc_v3.py`, whose output is
  committed as `docs/adr/0160-capacity/numbers-v3.md` (tables T1–T13, cited by name). v2's
  `capcalc_v2.py` / `numbers-v2.md` stay as history; where v3 and v2 differ, v3 governs.
* Related: ADR-0152 v3.1 (R-core+: SR, V, L, A, DA, §3.6 slash schedule, T-caps); ADR-0153 (2M
  attribution, unwritten); ADR-0043 (state root; amended by lane M's M-0b); ADR-0137/0144 (price of
  work); ADR-0142 (the DAA clock is a heartbeat cursor); the operator stopgap of 2026-09-26 01:00 (its
  anchor lane "lane A", `palw_operator_anchor_v1.rs`, and its operator-DA lane,
  `kaspad/src/palw_operator_da.rs`); the claim capacity map
  (`audit/claim-capacity` @ `c4e4a7c63`, `docs/palw-claim-capacity-map-2026-09-26.md`).

---

## 運用者向け要約（日本語）

**v3 で何が変わるか。** v2 の 2 回目のレビュー（CRITICAL 1・HIGH 5・MEDIUM 3・LOW 3）に全件答えた
（付録 D に対応表）。一番大きい変更は **credit（`m_c = ⌈E/ρ⌉`）の根拠** である。v2 は「抜き取り監査の
検出率 q が十分高いこと」を根拠にしていたが、レビューが示したとおり q は chain から観測できず、運用者の
ホスト 3 台の稼働に相関し、DAA-500 の lock life（F + 1,000）で seat の S4 が満期前に消え、しかも EV の式が
S4 を licence されない分岐にまで数えていた（正しい損益分岐は 225–250‰ で、v2 の 250‰ の関門には半分ルール
以外の余裕がなかった）。v3 は根拠を次のように置き換える：

> **credit を受ける claim は、panel に座っていない運用者（genesis カード）の 1 台（ρ ≥ 250 では 2 台）が
> 再計算して OK の receipt を chain に載せるまで Final しない（監査の扉 = lane Q、fence F-Q）。**

| レビューの指摘 | v3 の答え |
|---|---|
| CRITICAL: AG-7（有罪 seat の署名した licence を連鎖的に無効化）が網の約 5/8 の誠実な報酬を燃やす | **AG-7 は撤回**。監査の扉では不正 claim は 1 件ずつ Final 前に止まるので、連鎖は要らない |
| HIGH: lock life（F + 1,000）で S4 が消える | credit の安全はもう S4 に依存しない（S4 は余裕分）。それでも余裕を残すため、credit 付き claim の Final 前の lock は `L + window_court` のまま（**AS-5**、Final で F + 1,000 に付け替え）。誠実な claim の seat 資本は増えない |
| HIGH: q が観測できず相関、字義どおりの最悪値は N·(E − m_c) | 監査 receipt は chain に載る。監査が来ない claim は **待つだけ（燃やさない）**。字義どおりの持ち逃げ最大値は、監査 pool が誠実なら **0**（T7）。pool が侵害されれば N·E ——運用者 anchor が侵害されれば J-1 が破れるのと同じ前提 → **D-23** |
| HIGH: ρ_eff の保存場所がない | 受理時に claim の記録へ Λ の添字と credit の有無を書く。以後の読み手は全部その記録を読む。ブレーカーは境界ブロックの末尾で評価 |
| HIGH: lane N の配分が work-conserving でない | 未充足の需要（拒否された bond）だけが取り分を予約する work-conserving 配分に変更。13k は、網が詰まっていなければ ρ=10/25/100/1000 で 20 / 50 / 200 / 2,000 本まで届く。全面競合なら stake 比例で 1 / 4 / 9 / 36 本（T12） |
| HIGH: ブレーカーが T11 の約 1/3 で落とせる・B3 が網全体・S-I1 が壊れる | K1（満期後の有罪）は実質到達不能なうえ、到達しても何も回収しない（v2 の見積もりの約 1/3 で落とせた）ので廃止。**K1′**（監査済み claim の有罪 = 偽の監査 → 即 level 0、その監査者は pool から除外）と **K6**（監査の遅れ）を追加。K2 は「別々の bond 3 つ以上」。B3 は廃止。最も安い強制発動は K2 で約 39,000 MSK（T11）。S-I1 は「N_out 以上なら受理しない」に言い換え |
| MEDIUM: EV 式が S4 を過大に数えた | 式を修正（T2）。監査の扉では d = 1000‰ なので全ての行を満たす。**決定 2 の字義どおりの行（kind 12、生産者側の回収だけ）も満たす**（T3） |
| MEDIUM: W-8 が weight lane の実装と矛盾 | v2 の W-8 は撤回。拒否された兄弟ブロックは virtual の親から外れるので、「次のブロックが両方を merge する」という v2 の前提が成り立たない。weight lane の実装（2 DAA tick 以内の同点は GHOSTDAG が決める、8/8 で収束を実測）を採用。公表する確定深さは **2 DAA tick 超（約 9 chain block、約 4 分）** |
| MEDIUM: Stage 0 の公開 G2 が誤った q を測る | 「judges down」の測定は salted drill で行う。公開 t12 では運用者の監査経路に偽の capture を off-chain で流すだけ（署名も発行もしない） |
| LOW: T9 に anchor 上限がない / 8 段の上限が尽きる / A14 が過小 | T9 に anchor 列と運用者だけの carriage 列を追加（N5 は B_bind ≥ 385、2 倍余裕なら 770 が要る）。段の上限を 16 に。A14 を「500‰ 上限（13k で 6,500 MSK）の stage commitment ＋ S1 tier 最大 1 回（1,300）＋ 未稼得報酬、bond の残りは失わない」に訂正 |

**監査の扉の費用。** credit 付き claim 1 件につき運用者の再計算 1 回（floor は計算量で縛られていない。
8k は運用者の GPU が要るので floor が先、D-18）。ρ ≥ 250 では 2 人の監査を必須にする（**k_aud = 2**）：
T4 のとおり、侵害された監査ホスト 1 台で、ρ = 1000・10 個の Sybil producer の作戦は β = 0.7% で黒字になるが、
2 台必須なら 8.4% まで耐える。監査は運用者の既存の職務（panel-seed 対策の operator DA の judge、既に int-4 にマージ済み）
を chain で観測できる形にしたもので、protocol の職務として常時 on（flag ではない）。

**数字（v3）。**
- bond あたり同時本数（13k / 100k / 1M）：今日 2 / 15 / 156 → ρ=100 で 203 / 1,562 / 15,620 → ρ=1000 で
  **2,030 / 15,620 / 156,203（×1,015）**。fork の力は常に ≤ 2 / 15 / 153 FCW。これらは bond の**上限**で、
  実際に届くかは lane N の空き次第（T12）。
- ネットワーク（floor/DAA）：DAA-500 以降（V02(a) ＋ lock life）で seat 資本 4.46。credit 後は ×ρ（ρ=10 で 44.6）。
  ×100 では anchor（B_bind 100 で 260/DAA）が最初に縛り、×1000 では anchor（B_bind ≥ 385）、運用者だけの
  carriage（1,820/DAA）、状態（6.4 GB）、発行量（1 DAA あたり約 445 万 MSK、D-19）が縛る（T9）。
- 運用者の監査負荷：ρ=100（100/DAA）で 1 ホスト 12.5 件/DAA、×1000 で 125 件/DAA（T13）。

**範囲外だが重要（open risk）。** weight lane の実測（`3aa4abec4`）では、DAA-500 リリースの strict-win だけで
（F-W なしで）誠実な兄弟ブロックが 8/8 で分裂する。公開 t12 で DAA 500 以降に起きうるので、int-4 リリース側で
同じ同点規則を入れる必要がある。また lock life（F + 1,000）の下では、licence 後に開いた DA セッションの
default（開始から 1,200 DAA 後）の時点で covering seat の lock がもう切れているので、**今日の DA-7 は covering
signer に S4 を課せない**。同じ理由で X7 は F + 1,000 以降の Final claim に答えられるのを producer だけにするので、
**誠実な producer が W_disclose の間オフラインだと、Final claim 1 件ごとに S3（13k で 3,250 MSK）を取られうる**
（決定 1 の趣旨に反する）。AS-5 は credit 付き claim だけを直す（lock を Final まで長く、答える権利を満期まで）。
全 claim の修正は int-4 リリース側（例：material を持つ誰でも答えられるようにし、X7 の責任判定は課金にだけ使う）。

**実装の順。** J-1（済）→ 集約責任（済）→ lane D（決定 1〜4、価格の記録、AG-6、指摘の修正、seat 以外の
告発者）→ **lane Q（監査の扉）** → lane S（枠と上限）→ lane B（ブレーカー）→ lane N（網の room と
work-conserving 配分）→ lane M（fold・状態・rider）。ρ=100 を先に入れることはしない。

**ユーザーに決めてもらうこと**は §14（D-23 が最重要：credit の安全を運用者の監査 pool に置くこと）。

---

## 1. The user's design (binding, verbatim)

### 1.1 2026-09-26 11:00 JST (v1's basis; unchanged)

> 100倍化するなら「担保単価を1/100にする」変更と「claim throughputを100倍にする」変更を分離して設計する。
> 今の構造のまま数値だけ1/100にするとfork-choice powerだけ100倍安くなる。現行2Mは R=E+W=3,200.85+59,742.94=
> 62,943.79 MSK、予約を1/100にすると629.44 MSK、50% ceilingなら最低bond約1,259 MSK。escrow Eだけで3,200.85ある
> ので reserve=E+W のままでは629にならない(W=0でも最低担保約6,402)。100分の1化にはescrowをbondからclaimごとに
> 予約する設計自体を変える必要がある。4つに分離: Raw PWU C(verifier容量)/ Cash risk E(reward escrow)/
> Consensus weight W(fork choice cap)。PWUは維持(2Mが実際に重いことをprotocolへ正しく伝えるため)。bond
> reservationとfork weightをPWUから切り離す。
> 1. 未licence claimに2Mの巨大weightを与えない(最優先): J-1 を Σ W_provisional(bond) ≤ W_bond_cap とし100 claim
>    出してもfork choiceには最大2〜4 claim相当しか効かない。安全側: Created weight=0 / Anchored tiny provisional /
>    Licensed full/partial / Final full。claim数×100 ≠ fork power×100。2Mを安くする前にこれを入れる。
> 2. escrow Eを事前bond予約から外す: claim作成→報酬はまだ支払わない→licence→earned rewardからEをescrow化。存在
>    していない将来報酬をbondから先に3,200確保する構造をやめる。不正claimがlicence前に報酬を引き出せないなら
>    pre-licence cash exposureは大幅に小さくできる。成立すれば reserve_claim≈600 MSK のような設計が可能。
> 3. per-claim担保からaggregate bond liabilityへ: bond=13,000 MSK、claim#1..#100 → shared slash pool。条件: licence
>    前はrewardを引き出せない / 不正証明が出たらbond全体をaggregate forfeiture / voidしても義務は消えない / 同じ
>    bondによる連続不正は最初の検出で全claimをfreeze(J-2/J-3型)。「100 claimなら100×Eを担保しろ」から抜ける。
> 4. 2Mの処理能力は別途100倍化: 現在 network全体同時1本+replay約9.7日なら担保を1/100にしても100倍検証できない
>    (100 claim投入→99 claim待ち)。verification capacity ≥ issuance rate にする: 2M用seatを大幅に増やす/replayを
>    複数seatへ並列化/deterministic sharding/同一model executionの共通部分をcache/batch receipt/claimごとにfull
>    replayせずchallenge/fraud-proof層/licence前のfast verificationと後続deep auditを分離。完全replay 9.7日なら100倍化
>    には約100倍の計算資源か検証方式の変更が要る。担保parameterでは解決しない。
> 次期式: raw_pwu=現行のまま; claim_reserve = monetary_prelicense_risk + consensus_reservation;
> consensus_reservation = effective_weight × price_per_weight; effective_weight = min(raw_weight,
> per_bond_remaining_weight_cap); 2Mについて price_per_weight ≈ 現行の1/100、ただしJ-1でeffective_weight総量を先に
> 固定(価格を1/100にしてもbondが購入できる総fork powerは100倍にならない)。
> 重要: 13k bondで100本同時を狙うなら1/100では足りない: 13,000 bondの50% ceiling 6,500、100 claim同時なら
> 65 MSK/claim 以下、現在62,943.79なので約1/1000が必要。予約を1/100の629に下げるだけなら13kで約10本。「100倍
> 安くしたい」と「13kで100本出したい」は別目標として仕様に書く。
> 進め方: 1 J-1先行実装 2 escrowのfunding point変更 3 aggregate forfeiture導入 4 2M verifier改造(まず10倍capacityを
> 実測) 5 shadow accounting 6 testnetで10×→25×→100×と段階的に上げ private fork・free void・bond split・panel
> starvationを攻撃テスト 7 問題なければfenceで切り替える。
> 最終目標: 100倍claimを発行できる、しかし同時に100倍の金銭リスクもfork-choice powerも発生しない。
> ユーザーの目標: bond(枚数)あたりのclaimできる数を数百から数千倍にする。

### 1.2 2026-09-26 17:55 JST (v2's basis; v3's too)

> ここまで数字が出ているなら、設計の方向はかなり明確です。「bond が claim 1本ごとに escrow を丸ごと抱える」構造を
> やめて、bond を "多数の claim を支える共有保証枠" に変えるのが本筋です。いまの問題は N_claim ≈ 0.5×Bond /
> Reserve_per_claim なので、claim 数を100倍にしたければ Reserve_per_claim をほぼ1/100にする必要があります。ρ方式は
> まさにそれです。ただし、予約だけ小さくすると安全性まで100分の1になるので、J-1・aggregate forfeiture・検証
> capacityをセットにする必要があります。
>
> 1. bond の役割を「per-claim担保」から「aggregate liability」に変える。現行: bond ├ claim 1 → 3,200 MSK予約
>    ├ claim 2 → 3,200 MSK予約 … 新設計: 13k bond │ ├─ aggregate slash pool ├─ claim 1 … └─ claim 100+。claimを
>    100本出しても、それぞれに3,200 MSKを積むのではなく、bond全体が一つの責任主体になります。悪性claimが一定数検出
>    されたら、そのclaimだけではなくbond全体をfreeze / forfeitできるようにします。これがJ-2/J-3相当です。
> 2. claim発行枠とfork-choice weightを完全に分離する(J-1)。13k bondなら 発行可能: 200 claim outstanding、fork
>    choice contribution: 最大 floor 2 claim相当。W_bond = min(Σ W_i, W_cap(Bond))。claimを2本から200本に増やしても
>    W_cap(13k)=2×W_floor のまま。claim throughput ×100でもfork power ×1。これはρを50や100へ上げる前提条件にする。
> 3. ρ は直接「claim倍率」に使わず、risk tierとして使う。いきなり ρ=100 固定より rho = 10 → 25 → 50 → 100 の段階制。
>    reserve_i = E_i/ρ + W_i^effective として、実測のfraud detection率が一定期間十分ならρを上げる。検出率低下・panel
>    backlog増加・slash失敗・receipt期限超過が起きたら自動的にρを下げる。ただしconsensus parameterをリアルタイム
>    自動変更すると別の危険が増えるので、実際にはepoch/fence単位で切り替える方がいい。
> 4. 「同時claim数」と「発行rate」も分離する。13kで203本持てるからといって一瞬で203本出せる必要はない。13k bond:
>    outstanding cap = 200、burst cap = 8、issuance rate = 10 claim / DAA、fork weight cap = floor 2 claims相当。
>    private forkで203本を一瞬に吐くことも防げる。rateだけに安全性を任せない。安全性はJ-1とaggregate forfeitureが
>    担当し、rate/burstはDoS・queue保護だけに使う。
> 5. licenceで「発行枠」を返す。Created → Pending verification → Licensed → Final とし、Licensedになった時点で
>    claim slotを返す。ただしbondの不正責任まで消す必要はない。issuance slot → licenceで返却、slash liability →
>    challenge window終了まで残る、fork weight → J-1上限、の3つを別々の寿命にする。現状は「枠・担保・責任・weight」の
>    寿命が同じだから、throughputを上げようとすると全部が危険になる。
> 6. 13kの目標値(floor): outstanding claim cap 100〜200、burst cap 8〜16、fork-choice cap floor 2本相当、aggregate
>    slash pool bond全体、issue slot release licence、liability release challenge終了後、escrow reservation
>    E/50〜E/100、target throughput 80 DAAで800前後。800/80 DAA = 10 claim/DAA なのでネットワーク側もそれ以上処理
>    できないと意味がない。licence transportが約13/DAAなので、floor 1 bondで10/DAAならもうかなり上限近く。13k bondを
>    複数人が同時に全開で使えば、すぐネットワーク側が詰まる。
> 7. bond capはnetwork capacityから逆算する。networkが安全に50 claim/DAA処理できるなら global issue capacity =
>    50 / DAA を設定し、その中をbond間でfair-shareする。そうしないと 13k bond × 100 miners × 10 claim/DAA = 1000
>    claim/DAA になって、bond側は正常でもpanel/licenceが崩壊する。最終的には Allowed_i = min(BondCapacity_i,
>    FairShare_i, NetworkRoom, ClassRoom)。
>
> 設計として一番大事な変更: 今のMisakaは実質 bond額 → claim数 → fork weight → verification load がかなり連動している。
> これを Bond ├─ economic liability ├─ claim issuance capacity ├─ fork-choice cap └─ network fair-share の4本に切る。
> この分離ができれば 13kで2 claim → 100〜200 claim まで増やしても、fork力は2本相当のまま、不正時はbond全体を没収
> 可能、burstは8〜16程度、panel capacityを超えると発行停止、という設計にできる。実装順: J-1 → aggregate forfeiture
> → claim-slot/licence分離 → ρ=10 → 25 → 50 → 100 → network fair-share拡張。ρ=100だけ先に入れるのは避ける。J-1を
> 先に完成させることが100倍claim化の土台。1000倍を目標にエージェントを立てて実装を行なって。

### 1.3 The user's decisions of 2026-09-26 17:50 JST (binding)

1. **DA default (S1) stays TIER class** (3G cap + lifting freeze), not intent-class whole-bond
   forfeiture. An honest producer offline through one 1,200-DAA DA window must not lose its whole
   bond. Revisit only after auto-DA answering is measured robust.
2. **Credit pricing uses the loss a conviction ACTUALLY collects per offence class** (tier-capped
   kind 12 `LogitsNotStepOutput` / `StepArithmetic` collect about `w + m_c + min(10%·C0, 3G)` ≈ 1,620 MSK
   at a 13k bond), not an assumed `L = 3G/3E`. Fraud must never be profitable at a credited step.
3. **D-1** = 2 floor-claim weights per 13k (`⌊C/6,500⌋`); **D-2** class-neutral FCW unit; **D-3** C7
   (2M) Final weight capped at the 8k raw weight; **D-13** keep the 2M C7 cap of 1 until ADR-0153.
4. **Reporter reward capped**: no reward on the whole-bond forfeiture surplus; the reward stays within
   the pre-existing penalty tier (10% of the tier collection, `PALW_RCORE_REPORTER_REWARD_BPS_V1 =
   1,000`, `palw_state_v2.rs:970`).

Also binding, from 2026-09-26: the DAA-500 release arms V02 "both" — `palw_final_lock_full_collateral`
(V02(a)) **and** `palw_final_lock_life` (a resolved `Valid` lock lives `F + 1,000` and stops being
slashable when it stops being committed; `rcore/int-4@a067f490d:consensus/core/src/config/params.rs:1558–1598`).
v3 prices nothing on a lock past that life (§5.3, AS-5).

### 1.4 Standing rules this ADR obeys

* **Design bar** (`claim-collateral-design-bar`): cumulative claims unbounded; only concurrent
  unresolved risk is reserved; max run-away profit ≤ recoverable value, with a table of the max
  uncovered loss at 1,000 and at the ×1000 target (§4.4, T7 — literal and expected, side by side);
  vesting is collateral only when frozen + burnable + no escape. v3 meets the bar **literally** under
  two stated assumptions (an honest operator audit pool, a complete audit replay), which D-23 asks the
  user to confirm; v2's silent expected-value reading is withdrawn.
* **Withdrawal after observation** never erases an obligation; the obligation is fixed when the claim
  or anchor is accepted (§4.2).
* **Protocol duties are always on**, never flags (every rule here is a consensus rule or a node duty;
  the audit is an operator duty decided by identity, §5.9).
* **Honest users must not lose money**; an offline-honest producer is never whole-bond forfeited
  (decision 1; the bound is §5.5 and A14); nothing in v3 burns or voids an honest claim on a deadline it
  does not control (the audit door holds, never burns).
* **Fence rules**: t12-only, `None` on every shipped preset, Some-only hashing with the never()
  collapse, in `palw_fences_v1` (fork id), validated in `validate_palw_v2`, registered in
  `scripts/t12_repin.py` and `PALW_T12_POST_LAUNCH_FENCES_V1`; a flag day needs a drill that
  crosses it; the drill runs the shipping binary; t12 uses mainnet-intended values.

## 2. Where the v1 lanes stand, and what v3 does with their findings

`rcore/cap-int` is still `40b23d64d` (nothing merged). The fix-and-integrate workflow
(`wf_7ab46138-429`) has landed each lane's fixes on its branch; the v3 lanes start from the integrated
head.

| lane | head | fence | what it implements | its review findings, as fixed on the lane | v3 disposition |
|---|---|---|---|---|---|
| weight | `rcore/cap-weight@3aa4abec4` | F-W `palw_capacity_weight_cap` | staged weight; J-1 `min(X_b, W_cap)`; capped reservation; C7 Final ceiling; one Final pricing (`palw_weight_final_safe_v1`); **the shallow-tie rule** `palw_deep_reorg_capacity_v1` (`palw_fork_authority_v2.rs:145`, `PALW_CAPACITY_SHALLOW_REORG_DAA_V1 = 2`, `palw_weight_cap_v1.rs:139`) | reversed Final takes back its C7-scaled weight; F-W requires lane A; a shallow all-economic tie is GHOSTDAG's (8/8 convergence measured) | **kept as built**; v2's W-8 is withdrawn in its favour (§5.2); W-9 added by lane D |
| escrow | `rcore/cap-escrow@d6a058249` | F-E `palw_capacity_escrow_at_licence` | `m_c` in the escrow slot; E-4 obligation hold; E-5 stage-commitment charge; C7 gate; the bind reserves `max(duty, lock_2)`; the credit only at `q ≥ q_seat`; `palw_escrow_m_star_v2` on the conviction floor | as the title | the credit predicate becomes §5.3's audit-door predicate (lane D); `m*` stays an audit trail in the formulas module |
| liab | `rcore/cap-liab@deccda81b` | F-L `palw_capacity_aggregate_liability` (value: the ρ schedule) | AG-1…AG-5; the rooted freeze map; AS-1/AS-2; a refund to a forfeited bond joins its forfeiture (AG-9); the forger's forfeiture refunds the challenger (AG-10); the seat duty backs the lock; only whole-bond convictions credit q | as the title | F-L's value becomes v3 (§10.1: ≤ 16 steps, `k_aud`, per-class credits keyed on the audit door); AS-1 → AS-1′; AS-5 new (lane Q) |
| shadow | `rcore/cap-shadow@151f48b0c` | none (node-only) | op 201, the log, the CLI, the formulas module; seat duties by AS-1; A8 on a real t12 chain | as the title | ships first (Stage 0, §13) with the v3 formulas and the operator's would-be audit latency |
| verify | `rcore/cap-verify@29b1cb442` | F-R `palw_capacity_verify_room`, F-B `palw_capacity_batch_licence` | batch licence; room v2 (k = 2, replay speed 2,000‰); stake-proportional class share with one slack unit a bond (`8c4d8090b`); J-6 floor room; V3 private deferred audit (`PALW_DEFERRED_AUDIT_FRACTION_PERMILLE_V1 = 100`, `kaspad/src/palw_operator_da.rs:226`); V4 runs | V-T3/V-T4/V-T5 re-measured | the private deferred audit becomes the **second line** behind the audit door (it can expose a false receipt, K1′); lane N makes the network share work-conserving (§7) |

## 3. The four-way split of a bond

A bond `b` with posted collateral `C_b` plays four roles. Each is a separate quantity with its own
rule and its own lane; none is derived from another except through the collateral they share.

| # | role | quantity (per bond) | rule | implemented | new in v3 |
|---|---|---|---|---|---|
| ① | **economic liability** — one aggregate slash pool | `Pool_b = C_b + Σ unmatured legs of b + Σ withheld E of b's live claims`, plus, per refused fraud, the covering seat's `S4` (margin) | frozen at the first conviction (AG-3); intent class → whole pool forfeited (AG-2); every reward withheld to `Final` + maturity (E-2); **a credited claim reaches `Final` only through its audit (§5.9)**; exit shut while any row or commitment lives (B-3, `palw_bond_collateral_is_locked_v6`) | liab (F-L), escrow (F-E) | the audit door (lane Q); AG-6 scoped to credited claims; AG-7 withdrawn; AS-5 |
| ② | **claim issuance capacity** | `BondCapacity_b = min(N_bond, N_out, tokens)` | `N_bond`: `committed + (m_c + reserved) ≤ 500‰·C_b` (SR-7); `N_out = u_b·ρ_eff`; a token bucket | escrow (`m_c`), weight (`R_budget`) | lane S (§6), unchanged but for S-I1 |
| ③ | **fork-choice cap (J-1)** | `W_bond = min(Σ_i x_i, W_cap(C_b))`, `W_cap = ⌊C_b/6,500 MSK⌋·FCW` | staged `x_i`; class-neutral (D-2); C7 Final ceiling (D-3); the shallow-tie rule | weight (F-W) | W-9 (§5.2); the audit door also keeps an unaudited credited claim out of `safe_weight` |
| ④ | **network fair share** | the work-conserving room of the network level `L_net`, `ClassRoom_b`, and the credited-audit backlog | `L_net = min(L_seat, L_carry, L_anchor)`; registered unmet demand reserves its share, idle capacity is lent (§7.2); `A_max` bounds credited claims awaiting their audit (§5.9) | verify (F-R) | lane N (§7), lane Q's backlog |

The admission of one more claim of class `c` by bond `b` at DAA `t` is, in one function the fold,
the admission gate and the producer's facts share (SR-7's three-site rule):

```
admit  ⟺  BondCapacity_b(t) − held_b ≥ 1                               ② lane S + escrow + weight
       ∧  palw_network_admits_v1(state, b, t)                           ④ lane N, work-conserving (§7.2)
       ∧  ClassRoom_b(c, t) − held_b(c) ≥ 1                              ④ F-R (class share, or J-6 for the floor)
       ∧  (not credited(c, t)  ∨  credited_unaudited(t) < A_max(c))     ④ lane Q (§5.9)
       (and the lifecycle, NM, C7 and producer-floor gates, unchanged)
```

① and ③ are not admission terms: ① prices what a claim commits and what a conviction takes; ③
bounds what the admitted claims weigh. That is the separation: a bond can hold 2,030 claims (②) whose
fork power is 2 FCW (③), none of which is paid unless its audit passes (①), and it gets them only as
fast as the network's verification capacity has room for it (④).

## 4. The lifetimes per claim

### 4.1 The lifetimes

The user's lifecycle maps onto the phases as Created = `Provisional`, Pending verification =
`PanelBound` (and `DefaultDisputed`), Licensed = `ReceiptLicensed` with a counted recount (`basis_k ≥
2`, `palw_rcore_counts_licensed_v1`), Final = `Final`. v3 adds, for a **credited** claim only, the audit
between Licensed and Final.

| lifetime | acquired | released | what it bounds | rule |
|---|---|---|---|---|
| **issuance slot** (②) | acceptance | a **counted** licence (k ≥ 2); or `Final`; or a conviction void; or the end of E-4's hold for an unconvicted void. An S2 licence keeps its slot to `Final`. Never by a producer action | queue load; DoS | lane S (S.1, S.5) |
| **stage commitment** `m_c + reserved` (①) | acceptance | a counted licence where SR-1 holds; else `Final`; on an unconvicted void, `voided_daa + h_obl` (E-4); charged on S0′ (E-5) or conviction | pre-licence cash risk | escrow (F-E) |
| **audit** (①, credited claims only) | the licence (any licence, S2 included) | `k_aud` receipts from distinct members of the claim's audit pool (§5.9); **never by a deadline**: an unaudited credited claim waits | payment: no credited claim reaches `Final`, weight in `safe_weight`, or a vesting row without its audit | lane Q (F-Q) |
| **slash liability** (①) | acceptance | the withheld reward `E`: burned on void/conviction, paid only at `Final` + maturity; convictability to retirement (`max(terminal + claim_retirement_daa, last session close + 1)`, `palw_state_v2.rs:3637`); a credited claim's `Valid` locks stay slashable to `L + window_court` before `Final` (AS-5) | fraud | liab (F-L) + D + Q |
| **fork weight** (③) | acceptance, at 0 | Created 0 → Anchored ⌊10‰·W_full⌋ → Licensed W_full (S2 250‰) → `Final` (for a credited claim, only after its audit): `W_full` into `safe_weight`; the bond's sum capped at `W_cap` | fork power | weight (F-W) |

The user's "slash liability → challenge window終了まで残る" is the fourth row: it outlives the slot by
the whole conviction and maturity horizon, and it is carried by the claim's own withheld reward plus
the bond's pool, never by the slot.

### 4.2 Proposition 1 — the slot released at licence cannot be used to void after observation

*Claim.* No producer action after its panel is observable frees the slot, the commitment or the
liability earlier than the obligation horizon fixed at acceptance.

*Proof.*
1. A producer first observes its panel at the bind (`PanelBound`). From then on the only voids are
   `ReceiptTimeout`, `UnavailableQuorum`, `NotReplayBacked` (S0′), a DA default or a conviction —
   none a producer's choice. The three S0′ reasons keep the commitment to `voided_daa + h_obl`
   (`palw_void_reason_keeps_obligation_v1`, `rcore/cap-escrow@d6a058249:…/palw_escrow_funding_v2.rs`),
   and lane S keeps the slot for the same span (S.1). A DA default or a conviction charges at the void.
2. The claim stays convictable through the hold (AG-5); an abandonment after the panel is known is
   withholding, which auto-DA turns into S1 (decision 1: tier, with a freeze).
3. The one early release of the slot is a counted licence — the output of the fairly drawn panel's
   replay (`basis_k ≥ 2`), not an option the producer exercises. S2 does not release the slot.
4. After a licence there is no producer-initiated void; walking away forfeits the unminted `E`. The
   audit is not a void: a credited claim whose audit is refused is convicted through the ordinary
   routes (§5.9 (e)), and one whose audit is late waits.

So a void never buys a re-draw. ∎

### 4.3 Proposition 2 — recycling slots at licence cannot exceed the recoverable value

*Claim.* However many slots a bond recycles, at every instant the value it could extract without an
honest audit is 0, and every liability it has piled up is recoverable.

*Proof.*
1. Each recycled slot leaves a licensed claim carrying its own withheld reward `E` (unminted; burned on
   void or conviction). The pile's liability is funded by the pile itself.
2. A credited claim reaches `Final` — and so a vesting row, and so any payment — only with `k_aud`
   receipts from its audit pool (§5.9). A fraudulent claim cannot obtain an honest receipt: the pool
   member replays it, the roots and answer do not reproduce, it files instead (§5.9 (e)).
3. Nothing is extractable before maturity, and the bond's collateral cannot leave while it is payee
   of any unmatured row (B-3). Hence *extractable* = 0 ≤ `Pool_b` at every instant, for any number of
   recycled slots, **with an honest audit pool**.
4. The slot count does not enter 1–3; the S caps (§6) bound only queue lengths (and the audit
   backlog is bounded separately by `A_max`). ∎

### 4.4 Max uncovered loss (design bar)

T7, floor/8k, one bond, `N` concurrent credited claims (8k is identical under F-W because its `w` is
capped at `R_budget`):

| bond (MSK) | ρ | N | commitment `N(m_c+w)` | withheld `N·E` (frozen + burnable + no escape) | extractable before the audit | **literal max run-away, honest audit pool** | literal max, audit pool compromised | expected, an auditor passing β = 10‰ |
|---|---|---|---|---|---|---|---|---|
| 13,000 | 100 | 203 | 6,498 | 649,772 | 0 | **0** | 649,772 | 3,249 |
| 13,000 | 1000 | 1,000 | 3,201 | 3,200,847 | 0 | **0** | 3,200,847 | 16,004 |
| 13,000 | 1000 | **2,030 (×1000 target)** | 6,498 | 6,497,718 | 0 | **0** | 6,497,718 | 32,489 |
| 100,000 | 1000 | 15,620 | 49,999 | 49,997,222 | 0 | **0** | 49,997,222 | 249,986 |
| 1,000,000 | 1000 | 156,203 | 499,998 | 499,981,826 | 0 | **0** | 499,981,826 | 2,499,909 |

* **The literal reading (the reviewer's finding 3).** With every credited claim refused before `Final`
  unless an honest pool member reproduces it, a fraud is never paid, whatever `N`: the literal max
  run-away is 0, not `N·(E − m_c)`. It rests on two assumptions, both named: (i) the operator audit pool
  is honest — the same stopgap trust that already makes J-1 hold against a private branch (the operator
  anchor, §5.2) — and (ii) the audit replay checks what the panel's full seat checks (the adversarial
  battery G2′ and the D-5 burn-in, §13). When (i) fails, the literal max is `N·E` (the "compromised"
  column): the stopgap's failure, exactly as a compromised operator anchor defeats J-1. `k_aud = 2`
  from ρ 250 (§5.9 (c)) makes one compromised host insufficient (T4). D-23 asks the user to confirm
  this basis.
* **Today, for comparison.** ρ 1, `m = E`, no audit: an undetected licensed fraud is paid `E`; a 13k
  bond holds 2, so its literal max 2E = 6,401.69 MSK fits the 6,500 MSK it posts. A credit gives that up
  unless the audit refuses every fraud — which is why v3 prices the credit on the audit, not on a
  detection rate.
* **Fork weight uncovered:** ≤ `W_cap(C_b)` (2 / 15 / 153 FCW) of immature weight at any `N`, any class
  (J-1); and an unaudited credited claim adds nothing to `safe_weight`.
* **2M (C7):** never credited (`m = E`), the C7 cap 1: ≤ E = 3,200.85 MSK uncovered, as in v1/v2.
* **The pile** (T8): a 13k bond at ρ 100 issuing at 10/DAA holds ≈ 31,420 licensed-unmatured claims
  (100.6M MSK withheld, 7,736× its collateral) before its first reward matures; every one reached
  `Final` through its own audit.

## 5. Normative accounting (v3)

**Keying.** Claim-level rules read the claim's **recorded price** (below); conviction-level rules
read the DAA of the block carrying the conviction; object-level rules read the carrying block's DAA;
epoch-level rules (lane B) read the epoch the DAA falls in.

**The price record (finding 4).** Every new-rule claim accepted past F-L carries, in its rcore record,
an appended `price: Option<PalwCapacityPriceV1 { rho_idx: u8, credited: bool }>` (written only past
F-L; `None` below it; serialized through the record's existing versioned encoding, so nothing below
the fence moves). It is written once, at acceptance, by one function
`palw_capacity_price_at_acceptance_v1(state, params, bond, class, daa)`: `ρ_eff` from the step and the
breaker levels in force at that block (§8.3), and `credited` by §5.3. **Every later reader reads the
record, never live state**: the escrow term `palw_escrow_term_v3(params, record)`, the ledger's reserve
and release, SR-1's licence release, the seat duty at bind and the lock at licence (AS-1′/AS-5), the
forfeit, the audit door, and the load-time re-derivation. Only admission computes it, and there
`accepted_daa` is the carrying block's own DAA. So a breaker trip between a claim's acceptance and its
bind, licence or a restart changes nothing already committed (test D-T-price, §11.1).

### 5.1 The reservation (unchanged)

```
claim_reserve(c) = m_c + min(w_c, R_budget(b) − Σ_{live new-rule claims c' of b} reserved(c'))
R_budget(b)      = ⌊C_b / 6,500 MSK⌋ × w_FCW,   w_FCW = 10,752,660 sompi
ceiling (SR-7)   : committed(b) + claim_reserve(c) ≤ 500‰ · C_b
```

### 5.2 W — staged weight and J-1 (built), and the tie rule

As built on F-W (`rcore/cap-weight@3aa4abec4:consensus/core/src/palw_weight_cap_v1.rs`, constants FCW
604,250,611; 6,500 MSK per FCW; Anchored 10‰; S2 250‰; C7 ceiling 138,892,697,241 = 229.86 FCW):

```
x_c              = ⌊W_full(c) × stage‰⌋,  W_full(c) = min(raw_c, C7 ceiling)
W_bond(b)        = min(Σ_{live new-rule claims of b} x_c, W_cap(b)),   W_cap(b) = ⌊C_b / 6,500 MSK⌋ × FCW
bounded_immature = Σ_{live old-rule claims} raw_c + Σ_b W_bond(b)
```

Monotonicity (ADR-0042 D9) holds as v1 §4.2 proved. **W-7 (kept):** every site that moves a `Final`'s
weight prices it through `palw_weight_final_safe_v1`.

**The tie rule (v2's W-8 withdrawn; the weight lane's rule adopted).** Past F-W a `Created` claim
weighs 0 and a bond at its cap adds nothing, so honest siblings tie on (frontier, safe, live_total), and
strict-win (`palw_reorg_strict_economic_win`) keeps each node on the sibling it saw first. v2's W-8
allowed a tie only when the challenger merges the incumbent sink, relying on "the next block merges
both". That never comes: **a refused sibling leaves the virtual's parents** (the weight lane's analysis,
`3aa4abec4`), so no honest node's template references it and each side keeps extending its own sink.
The weight lane's rule, as built, is adopted unchanged:

```
past F-W (read at the incumbent's DAA, as strict-win is):
  strict-win decides every case it decides; on an ALL-ECONOMIC TIE whose reorg spans at most
  PALW_CAPACITY_SHALLOW_REORG_DAA_V1 = 2 DAA ticks on the incumbent's side, GHOSTDAG's own order
  (blue work, then hash) decides (palw_capacity_shallow_ghostdag_win_v1, processor.rs:14218, a bounded
  walk of 64 chain blocks, false on an unreadable store); a deeper tie keeps the incumbent.
```

Measured on the lane (two nodes, 7 exchanges, 8 runs): strict-win alone splits at the first exchange
in 8/8 runs; with F-W's rule all 7 exchanges agree in 8/8; a heavier tying private branch flips at 1
and 2 ticks and is refused at 3. **Published confirmation rule** (D-17′): an economic-tie reorg cannot
reverse a transaction buried more than 2 DAA ticks (≈ 9 chain blocks at the live 4.45 blocks a DAA,
≈ 4 minutes); within that window a tying branch heavier on blue work can (the price the lane pinned).
Lane D adds W-T12's cases: a withheld tied branch 3–9 chain blocks deep with higher blue work flips
only within 2 ticks; one constant, one test. (The DAA-500 release arms strict-win without this rule; the
lane measured that it splits honest siblings — §10.4, an open risk outside this ADR.)

**W-9 (kept).** SA-7's bound on court openings prices a claim's stake by its `G = g_res +
escrowed_reward` (`palw_claim_g_v1`, `palw_state_v2.rs:4546`), not by `claim.reserved`.

**Prerequisites of F-W (kept):** `palw_rcore_plus`, `palw_reorg_strict_economic_win`, and
`palw_operator_anchor` with its seed `palw_panel_seed_execution`, at or below.

### 5.3 m — the credit is priced on refusal before payment (decision 2)

**The corrected expectation (finding 7).** A fraud licenses with `P*` (500‰, the licence probability
the deterrent is sized for, `rcore/cap-escrow@d6a058249:…/palw_escrow_funding_v2.rs:73`). Only a
licensed fraud has a `Valid` signer to charge; on the licensed branch the stage commitment was released
at the counted licence (SR-1), so a refusal costs the coalition `X` beyond the reward it never gets; on
the unlicensed branch the claim voids and its stage commitment is charged:

```
EV(1)    = P*·[(1 − q)·E − q·X] − (1 − P*)·(m + w)                    (w dropped below: conservative)
q_needed = [P*·E − (1 − P*)·m] / [P*·(E + X)]   →   (E − m)/(E + X)  at P* = ½
```

v2 charged `X` on both branches (`(1−q)[P*E − (1−P*)m] − q(m + X)`), overstating the deterrent by
`q(1−P*)X` ≈ 1,200 MSK at q = 250‰. T2, corrected (permille):

| route | X (MSK) | ρ 10 | 25 | 50 | 100 | 250 | 500 | 1000 |
|---|---|---|---|---|---|---|---|---|
| seat charge `X = S4` (DA-7 on a default, AG-6 on a refutation) | 9,602.54 | 225 | 240 | 245 | 248 | 249 | 250 | 250 |
| garbage, producer side only (S2 tier) — **decision 2's literal** | 1,300.00 | 641 | 683 | 697 | 705 | 709 | 710 | 711 |
| naive, producer side only (a first S1 adds no tier, X11) | 0 | 900 | 960 | 980 | 990 | 996 | 998 | 999 |
| borrowed, producer side only (the whole 13k bond, AG-2) | 13,000.00 | 178 | 190 | 194 | 196 | 197 | 198 | 198 |
| *v2's formula, X = S4 (withdrawn)* | 9,602.54 | 127 | 137 | 140 | 142 | 143 | 143 | 143 |

With the seat charge the break-even tends to `E/(E + 3E)` = 250‰ — exactly `q_seat`. So v2's "q_needed
≤ 143 < q_seat, therefore all-or-nothing" was wrong, and at the v2 gate q = 250‰ EV(1) was only −160 /
−16 / −1.6 MSK at ρ 10 / 100 / 1000: the half rule was its only margin.

**Why a sampled q cannot carry the credit (findings 1, 2, 3).** Three independent reasons, each
measured or read from code:
1. *It is not observable and it is correlated.* The detection that produces q is node policy
   (`kaspad/src/palw_operator_da.rs`, the V3 private draw at a = 100‰), leaves no record unless it finds
   a lie, and rests on ≈ 3 non-panel operator hosts per claim (8 genesis cards, 5 per panel). An outage
   sets q = 0 for every claim licensed in it, and no breaker metric sees it before the rewards mature.
2. *The seat charge it needs is gone before most detections.* Past `palw_final_lock_life` a resolved
   `Valid` lock lives `F + 1,000` and the S-4 funnel charges only a slashable lock
   (`palw_false_valid_lock_slashable_v1`, `rcore/int-4@a067f490d:…/palw_state_v2.rs:2939`); DA-7 needs
   `da_lock_live_v1` (`palw_state_v2.rs:12594`). A detection in `(F + 1,000, F + 3,000]` collects no S4, and T2's producer-only rows need
   641–999‰.
3. *A campaign needs a reduction that hurts honest producers.* Without one, a colluding seat's 13
   charges are exhausted and a 1,000-claim campaign earns +258k to +1.07M MSK (T4b, corrected formula,
   q = 657‰ and 250‰); v2's reduction (AG-7) burned honest producers' rewards across ≈ 5/8 of the pile
   (the CRITICAL finding).

**The audit door (v3's basis).** A credited claim does not reach `Final` until `k_aud` distinct members
of its audit pool — the operator bonds that are neither its producer nor seats of its panel — have
replayed it and posted a receipt that its roots and answer reproduce (§5.9). A pool member whose replay
does not reproduce files instead (a DA accusation for missing material; a kind-4 refutation built from
the capture for garbage and borrowed, D.10 (a)). So, with an honest pool, the probability that a licensed
fraud is refused before payment is the audit's detection rate `d`, which is **1000‰ by construction**
for the three strategies — the pool member recomputes the claim's execution from the job its header
derives (`attempt_job_for_claim`) and compares the committed roots and answer, the check the panel's full
seat runs: roots with no material behind them (naive), wrong arithmetic or logits (garbage) and another
job's execution (borrowed) all fail to reproduce; the material is needed only to convict. Every row of T2 is
met — **decision 2's literal row included** (T3: kind 12 needs 641–711‰; the audit refuses 1000‰), so
fraud is unprofitable on what the producer side collects alone, and the seat's S4 (DA-7, AG-6) is
margin. T4 prices the stopgap's failure mode: an auditor that passes a fraction β of licensed frauds.
One producer's full campaign through one colluding seat breaks even at β = 796 / 421 / 169 / 43‰ at
ρ 10 / 100 / 250 / 1000 with one receipt, and at 411 / 206‰ at ρ 250 / 1000 with two; ten Sybil
producers sharing the seat's 13 charges break even at β = 7‰ with one receipt and 84‰ with two at
ρ 1000. Hence `k_aud = 2` from ρ 250 (§5.9 (c)).

**The rule the fold applies (lane D, reading lane Q).**

```
credited(c at acceptance)  ⟺  F-Q is active at accepted_daa
                              ∧ class c ∉ C7 ∧ E(c) > 0
                              ∧ the step at accepted_daa lists c with d(c, s) = 1000‰ for s ∈ {naive, garbage, borrowed}
m_c = 0                  if E = 0                  (the free-prompt lane; unescrowed merged work)
    = E                  if not credited
    = ⌈E / Λ[rho_idx]⌉   if credited                (rho_idx and credited from the price record)
```

The credit is all-or-nothing per class; `m*` stays in the formulas module as an audit trail (the shadow
prints it). **The credit table** (F-L's value, §10.1): `credits: Vec<(class_id, [d_naive, d_garbage,
d_borrowed] permille)>`, each entry the rate G2′ measured (§13); `validate_palw_v2` refuses a credit whose
`d` is not 1000, a C7 class, a credited step without F-Q at or below it, and `k_aud = 1` at ρ ≥ 250. The
floor ramps first; 8k follows when the operator's GPU audit capacity is measured (D-18). v1's half rule
(D-8) is retired with the sampled q it applied to: `d` is a deterministic property demonstrated per
class and strategy, not an estimate.

### 5.4 E — the funding point (built, unchanged)

E-1…E-6 as built on F-E (`d6a058249`): the bond never reserves `E` past the fence; the escrow slot holds
`m_c`; the reward stays withheld to `Final` + maturity; `m_c` is released at a counted licence under SR-1;
an unconvicted void keeps `m_c + reserved` for `h_obl = window_receipt` (600) past `voided_daa`, and lane
S keeps the slot for the same span; an unattributed failure costs exactly the stage commitment. The
lane's `palw_escrow_credit_applies_v1` becomes §5.3's `credited` read from the price record.

### 5.5 Aggregate liability — the producer side

As built on F-L (`deccda81b`): AG-1 (one pool, never debt, `slash_bond` saturates), AG-2 (intent class →
the whole collateral, the bond's unmatured legs, every live claim voided `AggregateForfeit`), AG-3 (the
rooted freeze: no maturity, `ProducerFrozen`, not drawn, exit shut; intent → final, tier → lifts at
`since + window_court`), AG-4 (only attributed convictions reach the pool beyond the stage commitment),
AG-5 (a voided claim stays convictable while its record stands). v3:

* **D1 — decision 1.** `palw_offence_is_intent_class_v1` returns **false** for `DaDefault`; the intent
  class is `CourtFraud` and contradictions 9, 10, 11, 13. **The honest-outage bound (finding 12),
  stated exactly:** an honest 13k producer offline through one `W_disclose` (1,200 DAA) loses (i) the
  stage commitments of its in-flight (unlicensed) claims, which its panels cannot replay without it and
  which void S0′ or default S1 — **at most its 500‰ ceiling, 6,500 MSK at 13k, at every ρ** (today:
  2 × 3,200.95 = 6,401.9); (ii) **at most one S1 tier**, `min(10%·C0, 3G)` = 1,300 MSK, if its defaults
  span three aligned strike epochs (X11: one strike per `⌊daa/1,000⌋`, the third inside 7,500 DAA adds
  the tier, `palw_state_v2.rs:954–959, :3770`); (iii) those claims' unearned rewards; and it is frozen for
  3,000 DAA (rows delayed, not burned). It never loses the rest of its bond: its credited licensed and
  `Final` claims are audited without it (the audit replays from the header's job) and answered for by
  their covering signers and auditors to maturity (AS-5 (2)). The bound holds for every claim the
  capacity fences price; an **uncredited** `Final` claim past `F + 1,000` is answerable only by its
  producer under the DAA-500 lock life (§10.4 (iii), an open risk of that release).
* **D4 — decision 4.** The reporter reward is 10% of the conviction's tier debit, never of a forfeiture
  surplus; AG-6 charges are burned and never rewarded.
* **AG-6 — the refutation's covering signers, automatically (credited claims only).** A conviction that
  proves a **credited** claim's execution false — kind 4 `ExecutorRefuted`, a kind-3 contradiction,
  `CourtFraud`, `CourtHeldVerdict` — charges, in the same funnel and before the producer's leg, every
  counted `Valid` signer whose attested mask covers the refuted segment: its lock first, then
  `min(25%·C0, 3G)`, under the audit's `(seat, claim)` key (`palw_false_valid_offence_id_v2`). It is DA-7
  step 1 generalized from "covers an unanswered unit" to "covers the refuted segment". Scoped to credited
  claims, it arms with the first credited step, which G9 holds until the D-5 determinism burn-in has
  measured the honest mismatch rate of kinds 5/12 (§13); uncredited claims keep today's rules. It is
  margin under the audit door, and AS-5 keeps the lock it takes live.
* **AG-7 — withdrawn (the CRITICAL finding).** v2's taint voided or burned every claim a convicted seat
  had co-signed unless untainted seats re-attested it in time — with 8 genesis seats and 5-seat panels,
  ≈ 5/8 of the network's pile (19,500 claims and 62.4M MSK withheld at N2) after one ≈ 9,602-MSK
  self-conviction, with rows maturing inside one replay latency burned outright. Under the audit door
  every credited fraud is refused individually before payment, so no campaign reduction is needed and
  nothing is held or burned on a co-signer's conviction. A future design that samples audits (`a <
  1000‰`) must bring its own campaign reduction in the form the review specified — hold, never burn on a
  deadline; a rooted re-attestation queue served before new admissions; taint only on a located fault
  in the seat's own mask; gated on the burn-in; a per-seat bound on the re-attestation load — and a new
  ADR (Appendix D, finding 1).
* **AG-8 — density escalation** (the user's "一定数検出されたら bond 全体"): as v2 — off (`M_esc = ∞`) at
  `H_cap`, a later flag day after the burn-in (D-16); DA defaults of one outage count as one.
* **AG-9 — a forfeited bond is a sink**, and **AG-10 — `AggregateForfeit` refunds the held forfeits**:
  as built on the liab lane (`deccda81b`: a refund reaching a forfeited bond joins its forfeiture; the
  forger's forfeiture reads each live claim's held forfeits before the void and refunds them after).

### 5.6 Aggregate liability — the seat side

* **AS-1′ (kept).** A seat bound to a claim reserves `e = max(duty′, lock′_2)`, duty and lock divided by
  the same `ρ_s` (`= Λ[rho_idx]` if the claim's record says credited, else 1): `duty′ =
  palw_rcore_duty_bind_v1(⌈λ/ρ_s⌉, ⌈lock_2/ρ_s⌉, m_c + reserved, seats)`, `lock′ = max(1 sompi,
  ⌈lock_k/ρ_s⌉)`. Uncredited claims are priced exactly as today; a drawn seat can always post its lock.
* **AS-2 (kept).** The lock falls by `ρ_s` only for a credited claim; it stays ≥ 1 sompi so B-3's exit
  gate pins the seat to its horizon.
* **AS-3 (kept, minus AG-7).** A seat's false-`Valid` conviction takes AG-2's funnel against the seat
  (tier for 5/12 and covering signers; the whole seat bond for 9/10/11/13 with a located fault in its own
  mask) and the freeze.
* **AS-4 (kept).** One ledger; `palw_bond_committed_v1` reads the re-priced duty and lock.
* **AS-5 (new; finding 2) — a credited claim's pre-`Final` lock keeps the long life, and its verifiers
  may answer for it to maturity.**
  1. *The lock.* At the licence, a counted `Valid` lock of a credited claim is stamped `L + window_court`
     (the life below the lock-life fence) whatever `palw_final_lock_life` says; at `Final` it is re-dated
     as V02 dates every lock (`F + 1,000`). The lock stays in `palw_bond_committed_v1` for its whole stamp,
     so slashable ⇔ committed (the user's V02 invariant) holds. Effect: a credited claim held before
     `Final` by its audit keeps its covering seats' locks live through a DA session (`W_disclose` 1,200)
     opened on the audit's refusal (`da_lock_covers_v1` reads liveness at the default's DAA,
     `palw_state_v2.rs:12594–12620`), so DA-7 and AG-6 collect S4 there. An honest credited claim reaches
     `Final` at ≈ L + 121 and its lock life is exactly V02's, so seat capital is unchanged (T6).
  2. *The answer right.* X7 lets only the producer or a bond with a live lock answer a DA session
     (`da_discloser_liable_v1`). Past the lock life that leaves an honest producer alone answerable for its
     `Final` claims in `(F + 1,000, maturity]`: offline through one `W_disclose`, a griefer's accusations
     there default, each an S3 (`min(25%·C0, 3G)` = 3,250 MSK at 13k) plus the row — four drain a 13k bond,
     and the accuser is refunded with a reward. For a credited claim, the covering signers and the
     receipting auditors may answer until its row matures, lock or no lock: an answer is checked by hash
     arithmetic against the claim's own roots, so the right carries no liability and moves no collateral.
     (Uncredited claims keep today's X7; the same gap there is an open risk for the DAA-500 release, §10.4.)

### 5.7 Proposition 3 — no campaign pays under the audit door

*Setting.* A coalition makes `K` fraud attempts of a credited class through covering seats it controls
(any number of producer bonds, any timing). Each licenses with `P*`; each licensed fraud is presented to
its audit pool.

*Claim.* With an honest audit pool, no fraud is paid, and `EV(K) = −(what the refusals collect) −
K·(1−P*)·(m + w) < 0` for every `K`.

*Proof.* By §5.9, a credited claim reaches `Final` — the only path to a vesting row and to `safe_weight`
— only with `k_aud` receipts from distinct pool members, each attesting that the claim's committed roots
and answer reproduce. An honest member's replay of a fraud does not reproduce, so it posts no receipt;
it files, and the claim is convicted through the ordinary route (DA default for missing material;
kind 4 for garbage or borrowed) while its locks are live (AS-5). The claim never pays; its producer loses
the tier and its unlicensed siblings lose `m + w`; the covering seats pay S4 while their collateral lasts.
No term grows with `K` on the gain side, so the sum is negative for every `K`. ∎

*When the pool is not honest* (the stopgap's failure): T4 gives the break-even pass rate β per step, one
and two receipts, one and ten producers; the private deferred audit (V3) replays audited claims too, and
its refutation of an audited claim is K1′: the passing auditor is excluded at once and the network
breaker drops to ρ 1 at the next boundary (§8.3).

### 5.8 Invariants and how each holds

| invariant | holds by |
|---|---|
| claims × N never gives fork power × N | J-1 (`W_bond ≤ W_cap`), Created 0, the operator anchor on a private branch, the shallow-tie rule; an unaudited credited claim adds nothing to `safe_weight` |
| a fraud never cashes out more than is recoverable | `E` withheld to maturity (E-2); B-3 exit gate; **the audit door: no credited claim reaches `Final` without `k_aud` honest receipts**; the seat's S4 as margin (DA-7, AG-6, AS-5) |
| voiding after observation never removes the obligation | E-4 (commitment) and S.1 (slot) held to `voided_daa + h_obl`; AG-5 convictability; auto-DA → S1 |
| honest users never lose money | decision 1 (the bound of §5.5 D1); E-5; the audit door waits and never burns; AG-9/AG-10; AS-1′ |
| rate and burst carry no safety | §6.3: the attack battery passes with the S caps set to ∞ |
| a price never moves after acceptance | the price record (§5, finding 4); the breaker evaluates at the end of the boundary block |
| below every fence nothing changes | dormant fences; the price record `None` below F-L; new rooted maps hashed only when non-empty; borsh-skipped mirrors; Some-only fingerprints with the never() collapse |

### 5.9 The audit door (lane Q, fence F-Q `palw_capacity_audit_door`)

**(a) The pool.** For a credited claim `c`, `pool(c)` = the operator bonds of lane A's rule
(`palw_operator_anchor_rule_v1`; on t12 the eight genesis cards) that are neither `c`'s executor bond
nor a seat of its current panel nor frozen (AG-3), minus `excluded_auditors`, ranked by `H(domain ‖ claim
‖ bond)`. The
rank function is lifted from the node (`palw_operator_da_order_v1`) into consensus-core so the fold and
the node read one order; the fold uses the pool as a set, the rank is the node's turn order. On t12,
|pool| = 3.

**(b) The receipt.** A new object `PalwAuditReceiptBatchV1 { auditor: PalwBondKeyV2, entries:
Vec<PalwAuditEntryV1 { claim_id, reproduced_root: Hash64 }>, signature }` (the next free object tag,
confirmed against the tag registry), signed by the auditor bond's genesis-registered key (the key lane A's
`operator_of_v1` checks), carried as a block-body object (an F-B list carrier may bundle it). The fold,
at the carrying block's DAA past F-Q, **refuses** the object (the block is invalid; the mempool refuses it
first) if its signature does not verify under the auditor bond's genesis-registered key, the auditor is not
an operator bond or is excluded, or an entry's `reproduced_root` differs from its claim's committed
execution root (the value the claim's panel receipts attest); it **skips** an entry whose claim is gone,
terminal, not credited, not yet licensed, already receipted by this auditor, or whose pool does not hold
the auditor. A kept entry appends `(auditor, daa)` to the rooted
map `audit_receipts: BTreeMap<Hash64, PalwAuditStatusV1>` (hashed only when non-empty; an entry leaves with
its claim's retirement).

**(c) `k_aud`.** A credited claim is **audited** when it holds receipts from `k_aud` distinct auditors.
`k_aud` is a field of F-L's step: 1 below ρ 250, 2 from ρ 250 (`validate_palw_v2`). With |pool| = 3 on
t12, `k_aud = 2` tolerates one pool member down.

**(d) The Final gate.** A credited claim that is not audited is not finalized at its `Final` sweep:
`palw_rcore_deadline_v1` returns no `Final` deadline for it (as it does for a claim under an open seat
session), and the deadline is re-derived when its last needed receipt lands, at `max(L +
short_challenge, audit_daa)`; `rebuild_deadline_index_v2` reads `audit_receipts`, so the index is exact at
rest. It stays licensed (weight J-1-capped) and **never voids, burns or charges for lack of an audit**.

**(e) Refusal.** A pool member whose replay does not reproduce posts no receipt and files: a DA
accusation where the material does not open (the existing `palw_da_accusation_object_v1`), a kind-4
`ExecutorRefuted` / `StepLeaf` demand built from the capture otherwise (D.10 (a)). The conviction then runs
as today, with AG-6 and AS-5 keeping the covering seats chargeable.

**(f) The backlog.** A derived index (rebuilt at load, checked by `assert_internal_consistency_v3`)
counts `credited_unaudited` = credited, licensed, non-terminal claims not yet audited and with no open
conviction session against them. Admission of a credited claim is refused `AuditBacklogFull` (a skip for
the block's own attempt, as `IssuanceCapped`) while `credited_unaudited ≥ A_max(class)`. `A_max` is a
release constant per class, set from Stage 0's measured operator audit rate × 60 DAA × ½ (a change is a
new fence); uncredited claims are never refused by it.

**(g) Accountability.** A conviction of a claim that holds any audit receipt inserts every receipting
auditor into the rooted set `excluded_auditors` (hashed when non-empty; only a flag day removes one) and
increments the epoch's **K1′** counter (lane B). The epoch's credited licences and those audited within
`W_aud = 60` DAA of their licence feed **K6** (lane B). A pool shrunk by exclusions below `k_aud` holds its
claims (they wait); K6 then trips.

**(h) The node duty (always on, by identity).** An operator node — one whose bond is an operator bond,
carrying `--palw-fee-outpoint` past F-Q — replays each credited licensed claim in whose pool it sits on its
turn (the existing judge turns, `PALW_OPERATOR_DA_TURN_DAA_V1 = 30`, `kaspad/src/palw_operator_da.rs:109`,
extended from "claims relying on a non-operator `Valid`" to "every credited claim"), batches a receipt
entry when the roots and answer reproduce, files per (e) when they do not, and passes its turn (loudly)
when its host cannot run the class. The V3 private deferred audit continues at the code's a = 100‰ and
now draws audited credited claims too: it is the second line that can expose a false receipt (K1′).

**(i) Cost** (T13): one operator replay per credited claim (two from ρ 250): 1.25 / 3.1 / 12.5 / 125
audits a DAA per operator host at 10 / 25 / 100 / 1,000 credited floor claims a DAA; audit entries ≈ 0.5k
transient mass each plus one ≈ 25k signature per auditor batch (an estimate lane Q measures). The floor has
no class room today (its replay is not compute-bound); an 8k audit is a GPU replay, so a credited 8k claim
costs its class room `k + k_aud` replays (F-R reads it) and 8k is credited only when the operator's GPU
audit capacity is measured (D-18).

## 6. Issuance controls — outstanding cap, burst, rate (lane S): DoS and queue protection only

### 6.1 Definitions

All integers, all read from rooted state at the carrying block's DAA (SR-7):

```
u_b          = ⌊C_b / 6,500 MSK⌋                                  FCW units: 2 at 13k, 15 at 100k, 153 at 1M
N_out(b, t)  = u_b × ρ_eff(b, t)                                   outstanding (slot-holding) claims
B_b(t)       = max(4, ⌈u_b × ρ_eff / 25⌉)                          bucket depth (burst), in claims
r_b(t)       = u_b × ρ_eff / 20                                    refill, claims per DAA (milli-claim fixed point)
tokens_b(t)  = min(1,000·B_b, tokens_b(t0) + 1,000·r_b·(t − t0))   refilled at use; a claim costs 1,000
BondCapacity_b(t) = min(N_bond(b), N_out(b, t)) − outstanding_b,  and a token must be available
```

`ρ_eff(b, t)` here is the value `palw_capacity_price_at_acceptance_v1` would record for a claim accepted at
`t` (§5); without a credit `m_c = E` and `N_bond` (2 at 13k) binds long before the caps.

### 6.2 Values (T5)

| ρ_eff | 13k: N_bond / N_out / B / r per DAA / per 80 DAA | 100k: same | 1M: same |
|---|---|---|---|
| 10 | 20 / 20 / 4 / 1.0 / 76 | 156 / 150 / 6 / 7.5 / 571 | 1,562 / 1,530 / 62 / 76.5 / 5,829 |
| 25 | 50 / 50 / 4 / 2.5 / 190 | 390 / 375 / 15 / 18.8 / 1,429 | 3,905 / 3,825 / 153 / 191.2 / 14,571 |
| 50 | 101 / 100 / 4 / 5.0 / 381 | 781 / 750 / 30 / 37.5 / 2,857 | 7,810 / 7,650 / 306 / 382.5 / 29,143 |
| **100** | **203 / 200 / 8 / 10.0 / 762** | 1,562 / 1,500 / 60 / 75.0 / 5,714 | 15,620 / 15,300 / 612 / 765.0 / 58,286 |
| 250 | 507 / 500 / 20 / 25.0 / 1,905 | 3,905 / 3,750 / 150 / 187.5 / 14,286 | 39,050 / 38,250 / 1,530 / 1,912.5 / 145,714 |
| 500 | 1,015 / 1,000 / 40 / 50.0 / 3,810 | 7,810 / 7,500 / 300 / 375.0 / 28,571 | 78,101 / 76,500 / 3,060 / 3,825.0 / 291,429 |
| **1000** | **2,030 / 2,000 / 80 / 100.0 / 7,619** | 15,620 / 15,000 / 600 / 750.0 / 57,143 | 156,203 / 153,000 / 6,120 / 7,650.0 / 582,857 |

These are per-bond **ceilings**: "per 80 DAA" assumes one attempt block per claim (or M-1 riders) and
the network room lane N leaves the bond (T12: under the live mix a 13k bond reaches them; under full
contention it gets its stake share). At ρ 100 a 13k bond gives the user's example: outstanding 200, burst
8, 10 claims a DAA, ≈ 762 per 80 DAA. Today's figure is 7.9 (capacity map §4.1).

### 6.3 Safety holds with the caps removed

Nothing in §4–§5 reads `N_out`, `B` or `r`: J-1 bounds weight at any claim count; the audit door refuses
every fraud individually (Proposition 3); the liability of any pile is its own withheld rewards plus the
pools (Proposition 2). The caps bound queue lengths (panels, carriage, anchors, audits, state growth).
**Test S-T6:** the attack battery A1, A2, A5, A9, A10 and A14 is run with `N_out = B = r = ∞` and must
pass with the same criteria.

### 6.4 The slot's accounting (S.1–S.5)

* **S.1** `outstanding_b` = the bond's new-rule attempt claims (accepted past F-S) that are not terminal
  and not counted-licensed, plus its voids inside E-4's hold. A derived index (not hashed, rebuilt at
  load, checked by `assert_internal_consistency_v3`).
* **S.2** A refusal on `N_out` or an empty bucket is `IssuanceCapped` — non-fatal for the block's own
  attempt; a merged attempt is skipped generically.
* **S.3** The bucket is rooted: `issuance_buckets: BTreeMap<PalwBondKeyV2, PalwIssuanceBucketV1 {
  tokens_milli: u64, last_daa: u64 }>`, hashed only when non-empty; a full bucket of a bond holding no slot
  is removed at its next touch.
* **S.4** One function, `palw_issuance_admits_v1(state, params, bond, daa)`, at `apply_attempt`,
  `palw_admission_v2` and `palw_producer_facts_v4`.
* **S.5** A slot is freed only by a counted licence, `Final`, a conviction void, or the end of E-4's hold.
  The audit does not hold a counted-licensed claim's slot (it was freed at the licence); an S2 claim's
  slot is held to `Final`, so to its audit.

### 6.5 A burst on a private branch

The DAA on t12 advances only by heartbeats, and `palw_clock_lead_cap` admits a header at most 132 s past
the receiver's clock (ADR-0142). A private branch with a lead of `L` DAA lets the attacker's bonds issue at
most `Σ (B_b + r_b·L)` claims — and J-1 plus the operator anchor keep every one of them at weight 0
(Created) there. The bucket makes the burst finite; J-1 makes it harmless (§12 A9).

## 7. Network issue capacity and fair share (lane N)

### 7.1 The network level `L_net`, from consensus-observable capacity

```
L_seat   = F-R's J-6 floor room (rcore/cap-verify@29b1cb442:…/palw_verify_capacity_v1.rs:263; e = AS-1′'s eligibility)
L_carry  = H_L × LPB × b_carry,  b_carry = max(1, ā_op), ā_op the mean operator-attempt chain blocks a DAA
           over the last 32 DAA (a rooted ring); LPB = 3 without F-B, 59 (16 seats) / 64 (8) with F-B paths,
           ≈ 700 with F-B's list form (audit entries share it, §5.9 (i))
L_anchor = B_bind × ā_op × anchor_delay
L_net    = min(L_seat, L_carry, L_anchor);   model classes use F-R's class room instead of L_seat
```

`b_carry` and `L_anchor` read operator attempts only (D-22), so a non-operator cannot inflate the level
with blocks that carry nothing. `B_bind` is set from V4's bind-cost run (100 until measured). Rates at the
live `ā_op ≈ 2.6` (T6): carriage 7.8 / 153 / 1,820 a DAA (none / paths / lists), anchors 260 a DAA at
`B_bind` 100 — so ×1000 (N5) needs `B_bind ≥ 385` (≥ 770 for G7's 2× margin) or more operator attempts, and
its operator-only list carriage (1,820) clears 1,000 by only 1.8× (G7 wants 2×: ā_op ≥ 2.86).

### 7.2 Fair share: work-conserving, split-neutral, over registered demand (finding 5)

v2's share reserved every active holder's full share whether or not it used it, and counted a bond only
once it held a claim: beside the live genesis producers a 13k bond got 1 / 3 / 10 / 97 claims at N2–N5
while ≈ 97% of `L_net` sat idle (the review's table), and a newcomer waited behind incumbents refilling
every freed unit (the verify lane's own stated residual, `palw_verify_capacity_v1.rs:45`). v3 reserves
capacity only for **unmet demand**:

```
units_a      = ⌊C_a / C_min⌋,  C_min = 13,000 MSK
Reg(t)       = { a : network_demand[a] ≥ t }            rooted map bond → until_daa, set to t + H_L at the
                                                        bond's NetworkRoomExhausted refusal (own or merged
                                                        attempt); expired entries
                                                        removed at their next touch; hashed when non-empty
share_a(t)   = ⌊L_net × units_a / Σ_{x ∈ Reg ∪ {b}} units_x⌋
deficit_¬b   = Σ_{a ∈ Reg, a ≠ b} max(0, share_a − held_a)
free         = L_net − U                                 (U = unlicensed claims network-wide)
palw_network_admits_v1(b)  ⟺  free ≥ 1  ∧  ( held_b < max(share_b, 1)  ∨  free − deficit_¬b ≥ 1 )
on refusal:  network_demand[b] = t + H_L
```

* **Work-conserving.** With no unmet demand (`Reg` empty) or every registered bond at its share, any bond
  may take all free capacity. Capacity is withheld only for a registered bond below its share.
* **The honest share.** A registered bond below its share is admitted whenever `free ≥ 1`, and nobody else
  may take the freed unit while its deficit stands: it reaches `⌊L_net·units_b/Σ_Reg units⌋` within about
  one `H_L` of registering, whatever others held before.
* **Split-neutral.** `Σ⌊C_i/C_min⌋ ≤ ⌊ΣC_i/C_min⌋`; a piece gains at most the one unit `max(share, 1)`
  gives it (V-I4, the verify lane's one-slack-unit rule, `8c4d8090b`). Registering costs a refused attempt
  block (real work) every `H_L`.

T12 (13k bond, floor): under the live mix (the genesis producers hold ≈ 55 unlicensed claims and are not
refused) the bond reaches its ceiling — 20 / 50 / 200 / 2,000 concurrent, 76 / 190 / 762 / 7,619 per 80
DAA at N2 / N3 / N4 / N5; under full contention (all eight cards registered, 576 units) it is guaranteed
1 / 4 / 9 / 36. **So the per-bond ×100/×1000 figures are reachable wherever the network has room, and
are stake-proportional where it does not** — the fairness the user asked for (§1.2 item 7).

### 7.3 Composition with F-R, lane S and lane Q

`palw_network_admits_v1` is one function used at the three sites; a refusal is `NetworkRoomExhausted`,
non-fatal for the block's own attempt. F-R keeps its per-class rooms and shares; lane N adds the
cross-class level and the work-conserving share; lane Q adds the credited-audit backlog (§5.9 (f)). The
user's example (100 × 13k at 10 a DAA = 1,000 a DAA of demand) at N2 (`L_net` ≈ 936): every bond is
registered, each is guaranteed ≈ 9 concurrent (≈ 0.43 a DAA), the network runs at its 44.6 a DAA seat
capital, and nobody's claims void `BindTimeout`.

## 8. ρ is a risk tier, and a breaker only lowers it (lane B)

### 8.1 The ladder

`Λ = [1, 10, 25, 50, 100, 250, 500, 1000]`. F-L's value (v3) is the fence height and its steps
`{ from_daa, ρ ∈ Λ, k_aud, credits }`, at most **16** (`PALW_CAPACITY_MAX_STEPS_V1` raised from 8,
finding 11; named step slots `_step_2 … _step_16`, each a fork-id-visible flag day). A step may keep ρ and
change only its credits or `k_aud` (a credit-only step: the 8k credit, an emergency withdrawal). The first
step is at `H_cap`; each later step is appended by its own flag day with a drill that crosses it; a public
step is never above the highest step a drill passed (§13). The v3 ladder needs at most 7 ρ steps + 1
uncredited `H_cap` step + 1–2 credit-only steps + contingencies ≤ 16.

### 8.2 What each step needs

For every class in the step's credit table: the audit door armed at or below it; G2′'s `d = 1000‰` for
each strategy; `k_aud = 2` from ρ 250; the other gates of §13 at the step's target rate.

### 8.3 The breaker: deterministic, lower-only, epoch-granular

**Epoch.** The aligned 1,000-DAA epoch (`PALW_RCORE_STRIKE_EPOCH_DAA_V1`). **Evaluation (finding 4):** at
the **end** of the first chain block at or past each epoch boundary, after that block's own attempts are
admitted and priced at the parent's level — so admission (`palw_admission_v2`, parent state) and the fold
price that block's attempts identically; the new level holds from the next block for the epoch. No
real-time adjustment.

**Levels.** `level_net ∈ {0…7}` and, per bond, `level_b` (a map, non-empty only while lowered), indices
into `Λ`. `ρ_eff(b, t) = min(ρ_step(t), Λ[level_net], Λ[level_b])`, **read only by
`palw_capacity_price_at_acceptance_v1`**; every claim thereafter reads its recorded price (§5).

**Network metrics** — each needs a conviction, a real service failure, or collateral to move:

| # | metric (epoch k) | where counted | trips when | severe |
|---|---|---|---|---|
| K1′ | **false audit**: a conviction of a claim holding an audit receipt (§5.9 (g)) | the conviction funnel | ≥ 1 | always → level 0 (ρ 1) |
| K2 | **slash shortfall**: `Σ(nominal − collected)` over the epoch's convictions, excluding convictions of bonds already forfeited whole (AG-2 took them) | `close_conviction_v1` | ≥ 10% of Σ nominal **and** shortfalls on ≥ 3 distinct bonds | — |
| K3 | **receipt-deadline overrun** (unattributed): `ReceiptTimeout` voids with at most one silent seat on the epoch's non-service list | the void sites | ≥ 5% of claims reaching their receipt deadline (≥ 20) | — |
| K4 | **panel backlog** (unattributed): `BindTimeout` / `NoCapablePanel` voids not caused by the producer | the anchor sweep | ≥ 5% of claims reaching their anchor (≥ 20) | — |
| K6 | **audit service**: credited licences not audited within `W_aud = 60` DAA | lane Q (§5.9 (g)) | ≥ 5% of the epoch's credited licences reaching `L + 60` (≥ 20) | ≥ 50% → level 0 |

**Per-bond metrics** (lower only the bond's own level): B1 the bond's own S0′ voids ≥ 20% of its claims
reaching a deadline (≥ 10); B2 any conviction of the bond (frozen too; on the lift it restarts one rung
below its pre-freeze level).

**Withdrawn from v2 (finding 6).** *K1 (escaped value)*: retirement is `max(Final + 3,000, last session
close + 1)` and maturity is `Final + 3,000` or later, so a conviction after maturity needs a session that
outlives both — the metric is near-unreachable and uninformative, and where it is reached the event
collects nothing (`post_final_producer_leg_v1`: "Otherwise nothing: no row", no live lock), so v2's 57,910
MSK trip cost was ≈ 3× too high (19,224–21,126 MSK, T11). K1′ replaces it. *K5 (detection lag)*: under
the audit door a credited claim is judged before `Final`; late convictions of uncredited claims do not
bear on the credit (and cost ≈ 3,250 MSK each to force). *B3 (AG-7 holds)*: gone with AG-7; a producer does
not choose its panel.

**Hysteresis.** A trip lowers one rung (severe: to level 0). A level rises one rung only after two
consecutive clean epochs (every metric below half its threshold), never above the step.

**Outstanding claims when ρ drops.** Nothing accepted is re-priced (the record); no claim is voided or
charged by a drop. New claims take the lower ρ, and `N_out`, `B`, `r` shrink with it, so a bond above its
new `N_out` issues nothing until its slots drain. **S-I1 is an admission property**: no admission while
`outstanding_b ≥ N_out(b, t)`; the state may hold `outstanding_b > N_out` after a drop (not a load
assertion).

### 8.4 Griefing — what one deliberate trip costs (T11)

* **K1′** — needs an operator auditor to sign a receipt for a claim later convicted: an outsider cannot
  force it; if it fires, it is the event it exists for.
* **K2** — ≥ 3 distinct bonds convicted past their collateral by tier convictions (forfeited bonds
  excluded): ≥ 3 × 13,000 = 39,000 MSK of the griefer's collateral drained. The cheapest priced trip.
* **K3/K4** — unattributable residual only; a griefer's own seats silent on > 20% of their draws enter the
  non-service list and stop counting (qualitative, as v2).
* **K6** — forced only by taking the operator's hosts down, which is the event it reports; the credited
  claims wait (nothing is paid unaudited), and new credited claims get a lower ρ.
* **B1/B2** — a bond's own failures lower only its own level. A convicted seat co-signing 5/8 of all
  panels lowers no honest bond's level (A11).
* **The harm of a trip** — one rung of ρ for new claims for at least three epochs (≈ 4 days at 120 s a
  DAA); no honest producer loses money, outstanding claims keep their prices.

## 9. The ×1000 roadmap

### 9.1 Per bond (collateral; the user's goal)

| ρ | 13k concurrent (T1) | × today | 13k per 80 DAA (T5) | × today (7.9) | credit needs | also needed (incl. the network stage that carries one such bond, §9.2) |
|---|---|---|---|---|---|---|
| 1 (today) | 2 | 1 | 7.9 | 1 | — | — |
| 10 | 20 | 10 | 76 | 10 | audit door; G2′ `d = 1000‰`; G9 burn-in | F-W F-L F-E F-R F-B F-S F-N F-K F-Q; lane D |
| 25 | 50 | 25 | 190 | 24 | same | lane S, lane B |
| 50 | 101 | 50 | 381 | 48 | same | J-1 armed since `H_cap` (the user's precondition for ρ ≥ 50) |
| 100 | 203 | 102 | 762 | 96 | same | lane N (the bond's room must hold ≈ 10 a DAA: T12 live mix yes, full contention 9 concurrent) |
| 250 | 507 | 254 | 1,905 | 241 | + `k_aud = 2` | M-0a (the fold's clone per merged attempt) |
| 500 | 1,015 | 508 | 3,810 | 482 | + `k_aud = 2` | M-0b (the state commitment) |
| **1000** | **2,030** | **1,015** | **7,619** | **964** | + `k_aud = 2` | ≈ 100 attempt blocks a DAA from one bond, or M-1 riders; network room (T12) |

Fork power ≤ 2 FCW at every row (J-1); literal uncovered loss 0 with an honest audit pool (§4.4). 8k
follows the floor's collateral column but its credit waits for operator GPU audit capacity (D-18) and its
class room pays `k + k_aud` replays. 2M stays at `m = E` and the C7 cap of 1 (D-13).

### 9.2 Network (floor claims a DAA; T6, T9)

| stage | enablers armed | seat capital | carriage all blocks / operator-only | anchors (B_bind 100) | target | audits per operator host a DAA | binds |
|---|---|---|---|---|---|---|---|
| N0 today (`0e8ec984e`) | — | 0.94 | 13 / 8 | 2.6·B_bind | 1 | — | seat capital |
| N0′ past DAA 500 | V02(a) + lock life | 4.46 | 13 / 8 | 260 | 2.7 | — | demand |
| N1 `H_cap`, uncredited | F-W F-L F-E F-R F-B F-S F-N F-K F-Q | 4.46 | 263 / 153 | 260 | 4 | — | seat capital |
| N2 first credited step, ρ 10 | + lane D; the audit door passed G2′, G9 | 44.6 | 263 / 153 | 260 | 10 | 1.25 | seat capital (4.5× margin) |
| N3 ρ 25 | + M-0a | 111.5 | 263 / 153 | 260 | 25 | 3.1 | seat capital / anchors |
| N4 ρ 100 | + F-B lists, M-0b | 446 | 3,115 / 1,820 | 260 | 100 | 12.5 | anchors (2.6×), then seats |
| N5 ρ 1000, floor only | + M-1 or ≥ 1,000 attempt blocks a DAA; `B_bind ≥ 385` (770 for 2×) or more operator attempts; D-19 | 4,460 | 3,115 / 1,820 | needs ≥ 1,000 | 1,000 | 125 | anchors and operator carriage (1.8× < 2×), emission, state |

The limiters, each from code (v2's list, corrected):
* **Seat capital** (capacity map §6.2): 3,992,004 MSK·DAA per floor claim with F + 3,000 locks, 1,591,004
  with the DAA-500 lock life; eight genesis seats: 0.94 shipped, 1.78 with V02(a) alone, **4.46 with both
  (what public t12 runs past DAA 500)** — v2's N1–N3 figures assumed locks the DAA-500 release no longer
  has (finding 2). × `ρ_s` with a credit (AS-1′); AS-5 leaves honest claims' lock life unchanged.
* **Licence carriage**: 3 a block without F-B; 59 (16 seats) / 64 (8) with paths; ≈ 700 with lists; lane
  N counts operator attempts only (2.6 a DAA): 7.8 / 153 / 1,820 a DAA (finding 10).
* **Anchors**: `B_bind × ā_op`: 260 a DAA at the placeholder 100; N5 needs ≥ 385 (a G7 measurement).
* **Audits** (lane Q): per operator host ≈ credited rate / 8; carriage of audit entries shares the list
  form (T13).
* **The attempt lane**: one claim per attempt block; every merged attempt clones the whole state
  (`checkpoint()` = `self.state.clone()`, `palw_state_v2.rs:20737`) — lane M's M-0a.
* **The state commitment**: `state_root` re-hashes every collection each block (`collection_root`,
  `:11815`) — O(state): 64 MB at 10 a DAA, 643 MB at 100, 6.4 GB at 1,000 (estimates) — lane M's M-0b.
* **Emission**: one subsidy per claim-bearing block: 4,445.62 MSK a claim; 4,445,620 MSK a DAA at 1,000
  (D-19).
* **8k replay** (k = 2 panel + 1 audit): 1,000 a DAA needs 2,804 seats at the engine's speed (1,869
  without the audit); 8k ×1000 is a GPU fleet.

### 9.3 Lane M — multi-claim carriage (D-12) and the O(Δ) fold

As v2: **M-0a** (the journal fold, no fence, byte-identical; twin-fold test), **M-0b** (fence F-M0, an
incremental state commitment, `state_root` v6), **M-1** (fence F-M1, up to `n_max = 64` rider attempts of
the same executor bond in one body carrier `PalwAttemptRidersV1`; the carve split `E_i = ⌊carve/(1+n)⌋`,
emission a block unchanged; riders share their lead's J-1 cap; each rider is its own claim — its own
panel, licence, slot, share, room **and audit entry** (a credited rider is audited like any credited
claim); a withheld rider block's riders void `BindTimeout`, charged to the withholder only). M-1 raises
claims per block at a constant reward per block; with one subsidy per claim (today) the same claim count
costs ×(1+n) emission instead (D-19).

## 10. The fence family (v3)

### 10.1 Names, values, prerequisites

All t12-only, `None` on every shipped preset, Some-only hashed with the never() collapse, listed in
`palw_fences_v1`, validated in `validate_palw_v2` (prerequisites "at or below"), registered in
`scripts/t12_repin.py` (a `_triple(… "shipped.testnet-12", dormancy test)`) and in
`PALW_T12_POST_LAUNCH_FENCES_V1` (so `--palw-drill-fence-at` crosses them).

| # | field | kind | lane | prerequisites (at or below) | keyed on | inserts after (merge anchor) |
|---|---|---|---|---|---|---|
| F-W | `palw_capacity_weight_cap` | bare | weight (+ D: W-9, W-T12) | `palw_rcore_plus`, `palw_reorg_strict_economic_win`, `palw_operator_anchor`, `palw_panel_seed_execution` | claim `accepted_daa`; the tie rule at the incumbent's DAA | `palw_reorg_strict_economic_win` |
| F-L | `palw_capacity_aggregate_liability` | value **v3** `{ activation, steps: [{ from_daa, rho ∈ Λ, k_aud ∈ {1, 2}, credits: [(class, [d_n, d_g, d_b])] }] }`, hashed whole; ≤ 16 steps, each a named slot | liab (+ D) | `palw_rcore_plus`, `palw_offence_attribution`; **a credited step: F-Q** | conviction DAA; the price record | `palw_operator_anchor` |
| F-E | `palw_capacity_escrow_at_licence` | bare | escrow (+ D) | F-W, F-L, `palw_rcore_plus`, `palw_offence_attribution`, `palw_audit_2026_09_23` | the price record | `palw_heartbeat_transparent_same_chain` |
| F-R | `palw_capacity_verify_room` | bare | verify | F-L, `palw_rcore_plus`, `palw_audit_2026_09_23`, `palw_offence_attribution` | block DAA | `palw_model_sink_bound` |
| F-B | `palw_capacity_batch_licence` | bare | verify | `palw_rcore_plus`, `palw_verification_v2` | carrying block DAA | `palw_model_sink_bound` (after F-R) |
| **F-Q** | `palw_capacity_audit_door` | bare | **Q** | F-L, F-B, `palw_operator_anchor` | carrying block DAA (receipts); the price record (the gate) | `palw_final_lock_life` |
| F-S | `palw_capacity_issuance_slots` | bare | S | F-W, F-L, F-E | block DAA (bucket); `accepted_daa` (slot) | `palw_capacity_weight_cap` |
| F-N | `palw_capacity_network_room` | bare | N | F-R, F-S, `palw_operator_anchor` | block DAA | `palw_capacity_verify_room` |
| F-K | `palw_capacity_rho_breaker` | bare | B | F-L, F-S, F-Q | epoch boundary (end of the boundary block); the price record | `palw_capacity_aggregate_liability` |
| F-M0 | `palw_capacity_state_commitment` | bare | M | — | block DAA (root v6) | `palw_capacity_batch_licence` |
| F-M1 | `palw_capacity_multi_claim` | bare | M | F-M0, F-B, F-N, F-S, F-W, F-Q | carrying block DAA | `palw_capacity_state_commitment` |

Lane D adds no fence: F-W, F-L and F-E are armed nowhere, so their rules change in place (and F-L's value
type becomes v3 before anything pins it).

**Constants** (not fence values; a change is a new fence): FCW 604,250,611; 6,500 MSK per FCW; Anchored
10‰, S2 250‰; C7 ceiling; P* 500‰; `h_obl = window_receipt`; `PALW_CAPACITY_SHALLOW_REORG_DAA_V1 = 2`;
`PALW_CAPACITY_MAX_STEPS_V1 = 16`; the S coefficients; lane N's `LPB` per F-B mode, `B_bind`, the 32-DAA
ring, `H_L = 21`; lane Q's `W_aud = 60`, `A_max(class)`; lane B's epoch, thresholds and hysteresis;
M-1's `n_max` 64. All at their mainnet-intended values.

### 10.2 Plumbing and parallel merges

As v2 §10.2: each new fence touches the ten sites (the `Params` field + doc; `: None` in every struct
literal; the never() collapse; `palw_fences_v1`; `for_each_fence`; the Some-only writes in
`consensus_params_id` and `consensus_schedule_id`; `override_params`; `validate_palw_v2`;
`PALW_T12_POST_LAUNCH_FENCES_V1`, whose entry sets the field **and** syncs the mirror; `scripts/t12_repin.py`
plus `consensus/core/tests/palw_capacity_<x>_is_t12_only.rs`). Each lane inserts after a different
existing fence's lines (the last column), so the branches merge onto `rcore/cap-int` without textual
conflicts. Mirrors in `PalwStateParamsV2` (`#[borsh(skip)]`): `capacity_slots_from_daa`,
`capacity_network_from_daa`, `capacity_breaker_from_daa`, `capacity_audit_from_daa`,
`capacity_state_v6_from_daa`, `capacity_riders_from_daa`.

### 10.3 `H_cap`

One common height arms F-W, F-L (its first step), F-E, F-R, F-B, F-S, F-N, F-K and F-Q. Constraints: not
500, 750 or 1,000, and not a multiple of 250; at least `D_shadow + 2,000` (Stage 0) `+ ≥ 300` (the drill)
`+ ≥ 200` (deploy margin), where `D_shadow` is the DAA at which the shadow node update is live on public
t12. Example: shadow live at DAA 1,100 → `H_cap` ≥ 3,600, e.g. 3,601. The first step is credited for the
floor only if Stage 1's credited drill, G2′ and G9 passed before the release is cut; otherwise it is ρ 10
uncredited and the credit comes by a later flag day. F-M0/F-M1 join only if their G7 passed; otherwise they
form a later release. Every later ρ step is its own flag day at its own height.

### 10.4 Relation to the DAA-500 and DAA-750 releases

Nothing here rides DAA-750. The capacity entries join `PALW_T12_POST_LAUNCH_FENCES_V1` on `rcore/cap-int`
after the 750 release is cut; `--palw-drill-fence-at` arms every entry at one height, so the capacity drill
crosses the 500/750 fences and the capacity fences together. The DAA-500 release arms V02(a)
(`palw_final_lock_full_collateral`) **and** `palw_final_lock_life`; every seat-capital figure in v3 assumes
both (T6), and AS-5 is written against the lock life. Three observations about that release, outside this
ADR's scope and reported as open risks: (i) strict-win alone splits honest siblings (the weight lane
measured 8/8 splits with the release's own fence set); the int-4 release needs the shallow-tie rule keyed on
its own fence, and F-W's copy reconciles with it at `rcore/cap-int`; (ii) a DA default lands `W_disclose` =
1,200 DAA after its accusation, and `da_lock_covers_v1` reads the covering locks' liveness at the default,
so for any accusation opened after its claim's licence the covering seats' locks (dated `L + 1,000`, then
`F + 1,000`) are dead and today's DA-7 seat charge is not collected; (iii) for the same reason X7 leaves the
producer alone answerable for its `Final` claims in `(F + 1,000, maturity]`, so an honest producer offline
through one `W_disclose` can be griefed into one S3 per such claim (3,250 MSK at 13k; the accuser is
refunded with a reward) — against decision 1's intent. AS-5 closes (ii) and (iii) for credited claims only;
for every claim, the int-4 release could let any holder of the material answer (an answer is checked
against the claim's roots) and keep X7's liability reading for the charge alone.

## 11. Lane contracts (the implementation phase)

### 11.0 Common to every lane

* **Base and branch.** Branch from `rcore/cap-int` after the v1 fix-and-integrate workflow merges weight,
  escrow, liab, verify and shadow; commit named files only (never `git add -A`, never `git stash`); one
  lane per worktree `~/Downloads/MISAKA-wt-b/wt-cap-<x>`.
* **Order and parallelism** (the user's order; few and elite): D → Q → S → B → N → M-0a, then M-0b and
  M-1. At most two implementing lanes at once, `nice -n 19`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, a
  target dir per lane; delete finished lanes' targets first; the DAA-750 release keeps priority.
* **Dormancy and identity.** Every fence dormant; `palw_the_release_did_not_move.rs` and the shipped pins
  (`b8564b88…`) green untouched; a twin fold (fence `None` vs armed above the tip) gives identical state
  roots; a twin restart/reorg test across the lane's own transition.
* **The formulas module** (`palw_capacity_formulas_v1.rs`, lane shadow) is extended by lane D with `_v3`
  functions (never edited once a consensus lane calls it); the shadow cross-checks every lane on planted
  states (G5); T2/T3/T5 are golden tables in it.
* **No SendMessage to running workflow agents.**

### 11.1 Lane D — the decisions, the price record, AG-6 and the review's findings (`rcore/cap-d`)

**Fences:** none new (F-L value → v3; rules behind F-L, F-E, F-W). **Depends on:** `rcore/cap-int` with the
v1 fixes.
* **D.1 (decision 1).** `palw_offence_is_intent_class_v1(DaDefault) = false`; `CoveringSigner` tier; intent
  = {`CourtFraud`, 9, 10, 11, 13}.
* **D.2 (the price record, finding 4).** The appended `price: Option<PalwCapacityPriceV1 { rho_idx,
  credited }>` in the claim's rcore record, written only past F-L, by
  `palw_capacity_price_at_acceptance_v1` (ρ_eff = the step's ρ until lane B adds its levels; `credited` per
  §5.3, reading F-Q's activation); `palw_escrow_term_v3(params, record)`, AS-1′'s `ρ_s`, AS-5, the forfeit,
  SR-1's release, the ledger and the load re-derivation read the record. The escrow lane's
  `palw_escrow_credit_applies_v1` and the liab lane's `palw_seat_credit_applies_v1` become one predicate
  over the record.
* **D.3 (F-L value v3, finding 11).** `{ activation, steps: [{ from_daa, rho, k_aud, credits: [(class,
  [d_n, d_g, d_b])] }] }`; `PALW_CAPACITY_MAX_STEPS_V1 = 16` with named slots `_step_2 … _step_16`;
  credit-only steps; `validate_palw_v2` refuses a C7 class, a `d ≠ 1000`, ρ ∉ Λ, `k_aud = 1` at ρ ≥ 250, a
  credited step without F-Q at or below its `from_daa`, non-increasing `from_daa`, more than 16 steps.
* **D.4 (decision 4).** Reporter reward on the tier only; AG-6 charges burned, never rewarded.
* **D.5 AG-6** as §5.5, credited claims only (the record), in the S-4/DA funnel, keyed `(seat, claim)`.
* **D.6 AG-9 / AG-10** as built on liab (`deccda81b`); kept and re-tested on cap-int.
* **D.7 AS-1′** as §5.6 (one predicate for both sides, reading the record).
* **D.8 the tie rule and W-9.** Keep `palw_deep_reorg_capacity_v1` as built; add W-T12 (a withheld tied
  branch 3–9 chain blocks deep with higher blue work: flips only within 2 DAA ticks, refused beyond);
  W-9 (SA-7 by `G`).
* **D.9 shadow v3.** The shadow reads the record, §5.3's credit and the audit door's state; its q column
  becomes `d` per (class, strategy) and the operator's would-be audit latency.
* **D.10 node (kaspad).** (a) The non-seat kind-4 filers: from a claim's capture, build and file the
  `ExecutorRefuted` / `StepLeaf` evidence (P2-8b/8d for non-seats) — the garbage and borrowed routes the
  audit door's refusals and the private deferred audit use, within the operator-DA burn budget
  (`palw_operator_da_budget_v1`); (b) the private
  deferred audit stays at the code's `a = 100‰` and also draws audited credited claims.

**Tests.** Unit: the intent table; the credit predicate (F-Q, C7, `d = 1000`); the record read at every
site; the v3 validate refusals incl. 16 steps and `k_aud`; AG-6's covering predicate (V2 masks, `Windowed`
receipts); T2/T3 golden. Processor: a twin fold across F-L with S1 tier; **D-T-price** — a planted `ρ_eff`
change (a test hook until lane B lands, the real breaker after) between acceptance and bind/licence, then a
restart mid-epoch: committed and index state equal to a node that did not restart; AG-6 on a batch-licensed credited claim; the liab panic reproduction loads;
the challenger refund reproduction; W-T12. Attack: A2, A5, A14 (v3 bound).
**Acceptance.** The whole v1 battery green; every second-review reproduction in Appendix D passes; 0
honest-loss events; the record survives restart, reorg and IBD identically.

### 11.2 Lane Q — the audit door (`rcore/cap-q`)

**Fence:** F-Q `palw_capacity_audit_door` (prerequisites F-L, F-B, `palw_operator_anchor`; inserts after
`palw_final_lock_life`). **Depends on:** D (the record, the filers).
**Contract.** §5.9 (a)–(h): the pool function in consensus-core (the rank lifted from
`palw_operator_da_order_v1`); `PalwAuditReceiptBatchV1` (next free tag) with its validity and skip rules;
the rooted `audit_receipts` map and `excluded_auditors` set (hashed when non-empty); the `Final` gate in
`palw_rcore_deadline_v1` and `rebuild_deadline_index_v2`; the derived `credited_unaudited` index and
`AuditBacklogFull` at `A_max`; the K1′ and K6 counters (a rooted pair until lane B folds them into its
counter struct); AS-5 (1) the licence-time stamp for credited claims and AS-5 (2) the answer right of a
credited claim's covering signers and receipting auditors to its row's maturity (a branch of
`da_discloser_liable_v1` read off the price record, answer only, no charge); the operator node duty
(§5.9 (h)), including answering DA sessions on the credited claims it receipted.
**Tests.** Unit: pool membership (producer, panel seats and excluded auditors removed), node and fold rank
equal; receipt validity (key, root mismatch → invalid; stale → skipped); `k_aud` counting; AS-5's stamp and
its re-date at `Final`; AS-5 (2)'s predicate (credited: signers and auditors answer to maturity; uncredited:
today's X7 byte for byte). Processor: a credited claim finalizes only after its receipt(s), and on time
when the receipt lands before `L + 121`; an unaudited credited claim waits past `L + 121` and finalizes when
the receipt lands (deadline exact at rest); with every pool member down an honest claim waits, is never
voided or charged, and finalizes after recovery; a conviction of an audited claim excludes its auditor and
counts K1′; a naive credited fraud refused by its audit defaults with its covering seats' locks live (DA-7
charges S4); the honest-outage case — a credited 13k producer offline through one `W_disclose` while a
griefer accuses its `Final` claims past `F + 1,000`: every session is answered by a covering signer or an
auditor, no S3, the loss within A14's bound; reorg across a receipt; restart with pending audits.
**Acceptance.** G2′ in the salted drill (§13): ≥ 50 adversarial floor claims per strategy licensed by
red-team seats, 100% refused before `Final` and convicted, S4 collected on each; honest credited claims
audited with p99 ≤ `W_aud` at the drill's target rate; T13's per-host load measured and `A_max` set from it.

### 11.3 Lane S — the slot, its lifetime, and the DoS caps (`rcore/cap-s`)

**Fence:** F-S. **Depends on:** D. **Contract:** §6 (S.1–S.5): the derived `outstanding` index; `N_out`,
`B`, `r` from `ρ_eff`; the rooted `issuance_buckets`; `palw_issuance_admits_v1` at the three sites;
`IssuanceCapped` joins the own-attempt skip arm.
**Invariants:** S-I1 no admission while `outstanding_b ≥ N_out(b, t)` (the state may exceed `N_out` after a
breaker drop); S-I2 the bucket is a pure function of the chain; S-I3 safety without caps (S-T6); S-I4 byte
identity below F-S.
**Tests.** Unit: the bucket arithmetic, T5 golden. Processor: a counted licence frees the slot, S2 does
not; an E-4 void keeps it for `h_obl`; a conviction void frees it; reorg across a refill; a breaker drop
leaves `outstanding > N_out` and admits nothing until it drains. Attack: A9, A10, S-T6.
**Acceptance.** In the capacity harness, a 13k floor bond at ρ 100 (credited, ideal seats, network room
free) makes 762 ± 10% claims per 80 DAA at 12 blocks/DAA, the figure at 4.45 blocks/DAA reported beside it.

### 11.4 Lane B — the lower-only breaker (`rcore/cap-b`)

**Fence:** F-K. **Depends on:** D (the record), S, Q (K1′/K6 counters). **Contract:** §8.3: rooted
`capacity_breaker { level_net, clean_streak, epoch, counters }` and `capacity_bond_levels`; counters in the
conviction funnel (K1′ via Q, K2 with its distinct-bond floor and forfeited-bond exclusion), the void sites
(K3, B1), the anchor sweep (K4), lane Q (K6), the freeze (B2); evaluation at the end of the first chain
block at or past each boundary; `palw_capacity_rho_eff_v1(state, params, bond, daa)` read only by
`palw_capacity_price_at_acceptance_v1`.
**Tests.** Unit: the ladder, hysteresis, severe K1′/K6, per-bond isolation. Processor: planted metrics trip
at the boundary and restore after two clean epochs; the boundary block's own attempt is priced at the
parent's level by admission and fold alike; reorg across a boundary; restart mid-epoch after a trip
(identical committed and index state); a claim accepted before a trip keeps its price. Attack: A11 — every
metric's cheapest forced trip ≥ T11; an AG-6-convicted seat on 5/8 of the panels lowers no honest bond's
level.
**Acceptance.** Drill: a stopped audit pool trips K6 at the next boundary; ρ_eff drops for new claims only;
recorded prices unchanged; restore after two clean epochs; no false trip over 2,000 DAA of honest traffic
(Stage 0, off-chain).

### 11.5 Lane N — the network level and the work-conserving share (`rcore/cap-n`)

**Fence:** F-N. **Depends on:** F-R (verify), S, Q. **Contract:** §7: the rooted operator-attempt ring;
`L_carry`, `L_anchor`, `L_seat` (F-R's J-6) and `L_net`; the rooted `network_demand` map; the work-conserving
`palw_network_admits_v1` at the three sites; `NetworkRoomExhausted` registers demand; lane Q's backlog gate
beside it.
**Tests.** Unit: the levels arithmetic; split neutrality (76 × 13k vs 1 × 988k beside an honest 1M: pieces ≤
the whole's share + one per piece); the honest share reached within `H_L` of registering; work conservation
(no registered deficit → all free capacity usable). Processor: T12's two scenarios with the eight genesis
cards (the live mix: a 13k bond reaches its ceiling; full contention: ≥ `max(⌊L_net/577⌋, 1)`); the user's
100 × 13k example — admitted ≤ `L_net`, no honest `BindTimeout`, carriage never above `LPB` per block.
Attack: A4, A12 with the genesis cards producing.
**Acceptance.** Capacity harness at 16 and 100 producers: the admitted rate within 10% of the smallest
limiter; no idle `L_net` while any bond is refused; every registered bond ≥ its share − 1 after `H_L`.

### 11.6 Lane M — the O(Δ) fold, the state commitment, riders (`rcore/cap-m`)

**Fences:** M-0a none; F-M0; F-M1. **Depends on:** F-B, N, S, D, Q. **Contract:** §9.3.
**Measurements (first deliverable):** V4's attempt lane at 4 / 8 / 16 / 32 producers with and without
M-0a; fold ms per chain block against 10⁴ / 10⁵ / 10⁶ live claims; `state_root` time v5 vs v6; bind cost at
100 and 1,000 binds in one anchor block (`B_bind`); rider mass and the block count at 64 riders; audit
entries per rider batch.
**Tests.** M-0a twin equivalence; F-M0 crossing; riders: W-T1 with riders, dedup, a malformed rider
disqualifies, a live-state refusal skips, reorg across a 64-rider block, each credited rider audited, A13.
**Acceptance.** At N4 (100 a DAA, 314,200 live claims) a chain block folds in ≤ 1 s and roots in ≤ 100 ms
on the reference host; at N5 (1,000 a DAA via 16 blocks × 64 riders) ≤ 2 s and ≤ 200 ms.

### 11.7 The v1 lanes

Weight, escrow, liab, verify and shadow finish as §2 records. v3 changes their contracts only through lane D
(which edits their modules on cap-int) and lane Q, and through the plumbing anchors above. Merge order on
cap-int: shadow → weight → liab → escrow → verify, then D → Q → S → B → N → M.

## 12. Attack model (tested at every step, §13)

v1's A1–A8 stand with these updates: **A1** runs with the shallow-tie rule and with riders; **A2** also
asserts the slot is held to `+h_obl`; **A3** adds lane N's unit share; **A4** runs against `L_net` with the
genesis cards producing; **A5** asserts AG-9 and AG-10; **A8** (q collapse) becomes the audit-pool outage
(A15).

| # | attack | mechanism | what bounds it | test (lane) | pass criterion |
|---|---|---|---|---|---|
| A9 | **burst private fork** | bonds spend their bucket and `N_out` on a private branch with lead `L` ∈ {1, 13} DAA, then release it; with and without F-S; with riders | J-1, Created 0, the operator anchor, strict-win + the shallow-tie rule | S | deep reorg refused; `live_A − live_public ≤ Σ W_cap(C_A)` independent of `B`, `r`, `N_out` (also with caps ∞); a tying withheld branch flips only within 2 DAA ticks |
| A10 | **slot recycling at ×1000** | a coalition licenses its own junk through a colluding seat as fast as slots return, K ∈ {10², 10³, 10⁴}, all three strategies | the audit door refuses each credited fraud before `Final` | D + Q + S | no credited fraud reaches `Final`; realized EV < 0 over 1,000 trials; an honest pile of the same size is audited and paid on time |
| A11 | **breaker griefing** | force each metric at the least cost; flood own claims and abandon them; a seat co-signing 5/8 of panels convicted | K1′ needs a false receipt; K2 needs ≥ 3 distinct drained bonds; K3/K4 unattributable only; K6 needs the operator's hosts down; B1/B2 per bond | B | the cheapest forced network trip costs ≥ T11 (39,000 MSK); one convicted seat lowers no honest bond's level; honest producers lose no money and keep their prices |
| A12 | **fair-share Sybil split** | 1 × 988k vs 76 × 13k beside an honest 1M and the genesis cards, under a full `L_net` | units `⌊C/C_min⌋`, registered demand, `max(share, 1)` | N | pieces hold ≤ the whole's share + one per piece; the honest bond reaches its share within `H_L` |
| A13 | **multi-claim block withholding and reorg** | withhold a 64-rider block past its riders' anchors; reorg a rider block across its anchor, licence and audit | riders carry the carrying block's DAA → `BindTimeout` charged to the withholder; dedup; the delta reverts every rider | M | withheld riders charge only the withholder; twin roots identical; the bond's term ≤ `W_cap` |
| A14 | **liability beyond the bond at ×1000, and an honest outage** | a 13k bond at ρ 1000 with 2,030 claims and a 314,200-claim pile, all fraudulent; and an honest 13k bond with the same pile offline through one `W_disclose` | commitments ≤ 6,500 MSK; the audit door; decision 1 | D + Q + S | no debt, no panic, no fraud reaches `Final`; **the honest bond loses at most its 500‰ ceiling (6,500 MSK) in stage commitments + at most one S1 tier (1,300 MSK) + the unearned rewards of its in-flight claims, never the rest of its bond or its licensed pile** — including under a griefer's DA accusations on its `Final` claims past `F + 1,000` (answered by AS-5 (2)) |
| A15 | **audit-pool outage** | stop every operator auditor for 1, 100 and 1,500 DAA under N2 load; also stop one of three | the audit door waits; `A_max` stops new credited admissions; K6 | Q + B | no credited claim reaches `Final` unaudited; no honest claim voided, charged or burned; admissions of credited claims stop at `A_max`, uncredited continue; K6 trips; all waiting claims finalize after recovery |
| A16 | **a compromised audit host** | one pool member signs receipts for frauds, at `k_aud` 1 and 2 | `k_aud = 2` from ρ 250; the private deferred audit → K1′ → exclusion and level 0 | Q + B | at `k_aud = 2` no fraud passes with one compromised host; at `k_aud = 1` the first fraud the deferred audit refutes excludes the host and drops the network to ρ 1 at the next boundary |

## 13. Staged ramp and gates

**Gates** (each step passes all):
* **G1** the attack battery A1–A16 passes at the step's scale (K × ρ).
* **G2′ (finding 9)** the audit's detection per (class, strategy) is 1000‰, measured where it can be
  measured without public cost: **in the salted drill** with the shipping binary and live operator
  auditors, ≥ 50 adversarial claims per credited class and strategy (naive, garbage, borrowed), licensed by
  the drill's own red-team seats (the panel compromised — "judges down"); pass = every one refused before
  `Final` and convicted while its covering seats' locks are live (AS-5), the licence → conviction histogram
  recorded, no honest credited claim unaudited past `W_aud`. **On public t12** (Stage 0) only the operator's
  auditors replay planted corrupted captures of each strategy off-chain (the shadow harness feeds them to
  the auditor path; nothing is signed, filed or minted; detection must be 100%) and log the would-be audit
  latency of real claims (p99 ≤ `W_aud`/2 at the live rate). No red-team seat on public t12.
* **G3** twin, restart and reorg battery green (byte identity below every fence; re-derivation at load; the
  price record).
* **G4** a crossing drill with the shipping binary: every capacity fence at one height.
* **G5** shadow = fold on the drill's states.
* **G6** no honest-loss event: no honest bond forfeited or frozen by another's act; honest losses ≤ the
  stage commitment of griefed claims; A14's bound; A15's "nothing burned".
* **G7** the network limiters of the step's target rate (T9) measured with ≥ 2× margin: seat capital,
  operator-only carriage, anchors (`B_bind`), the attempt lane, audit throughput and receipt carriage (T13),
  fold ms per block, `state_root` ms.
* **G8** the breaker drill: planted trips fire at the boundary and restore after two clean epochs; A11's
  costs hold.
* **G9 (new)** the D-5 determinism burn-in has measured the honest mismatch rate of kinds 5/12 ≪ 1 per pile
  (AG-6 arms with the first credited step).

| stage | where | what arms | ρ / credit | battery | gate to the next stage |
|---|---|---|---|---|---|
| 0 | **public t12, node update** | shadow v3 (op 201), the operator's would-be audit latency, planted-capture detection, D.10 filers, the breaker computed off-chain | — | planted captures per strategy (off-chain) | ≥ **2,000 DAA** of shadow data; planted detection 100%; audit latency p99 ≤ 30 DAA; no false breaker trip |
| 1 | salted t12 drill (`--palw-drill-genesis-salt`, `--palw-drill-fence-at`, the shipping binary) | F-W F-L F-E F-R F-B F-S F-N F-K F-Q at one low height | 10 uncredited, then 10 credited (floor) | A1–A16 at K × 10; G2′ | G1–G9 |
| 2 | drill | append 25 | 25 | K × 25 | G1–G9; N2 limiters measured |
| 3 | drill | append 50, then 100; F-B lists | 50, 100 | K × 100 (13k × 203) | G1–G9; M-0a measured |
| 4 | drill, **floor only** | append 250, 500, 1000 with `k_aud = 2`; F-M0, F-M1 if built | 250 → 1000 | K × 1000 (13k × 2,030) | G1–G9; N4/N5 gates only if D-19 is decided |
| 5 | **public t12, `H_cap` flag day** | F-W F-L F-E F-R F-B F-S F-N F-K F-Q at `H_cap` | ρ 10, credited for the floor only if Stage 1's credited drill, G2′ and G9 passed; else uncredited | Stage 1's battery on the release candidate's drill | the user's go; re-pin; the crossing drill; deploy with the user's confirmation |
| 6 | public, later flag days | append one step at a time (credit-only steps included) | never above the highest step a drill passed | ≥ 1,000 DAA of shadow at the prior step: audit latency, seat capital, share fairness, breaker quiet | G1–G9 on a drill of that step; the user's go |

## 14. Decisions

### 14.1 Earlier decisions, now

| # | question | status |
|---|---|---|
| D-1 | W_cap scale | **decided**: 2 FCW per 13k |
| D-2 | class-neutral unit | **decided**: FCW |
| D-3 | C7 Final ceiling at 8k raw | **decided** |
| D-4 | price per weight | open; recommendation unchanged (uniform p0; J-1 makes it moot) |
| D-5 | scope of whole-bond forfeiture | **amended by decision 1**: intent = `CourtFraud`, 9, 10, 11, 13 |
| D-6 | freeze trigger | recommendation unchanged (conviction) |
| D-7 | unattributed charge | recommendation unchanged (stage commitment) |
| D-8 | q credit rule (half rule) | **retired with the sampled q** (§5.3): the credit is the audit door's `d = 1000‰` per class and strategy |
| D-9 | public ramp | one flag day per step, the lower-only breaker (§8.3) |
| D-10 | anchor_delay | recommendation unchanged (20) |
| D-11 | floor room and stake share | built (F-R); made work-conserving by lane N |
| D-12 | the attempt lane | lane M (§9.3); M-1's economics is D-19 |
| D-13 | 2M before ADR-0153 | **decided**: closed (C7 cap 1) |
| D-14 | count the seat's automatic S4 in the credit (v2) | **superseded by D-23**: the S4 is margin under the audit door |
| D-15 | AG-6 and AG-7 (v2) | **AG-7 withdrawn**; AG-6 → D-15′ |
| D-16 | AG-8's threshold | unchanged: off at `H_cap`, a flag day after the burn-in |
| D-17 | W-8 tie convergence (v2) | **superseded by D-17′** (the weight lane's shallow-tie rule, §5.2) |
| D-18 | per-class credits | floor first; 8k when the operator's GPU audit capacity is measured (`A_max(8k)`) |
| D-21 | the breaker's thresholds | v3 set (§8.3); tuned only in drills (G8), changed only by a new fence |
| D-22 | lane N reads operator attempts; the audit pool is the operator set | both re-derived by a fence when the structural P0-10 fix retires the stopgap |

### 14.2 Decisions for the user (each with a recommendation)

1. **D-23 — the credit rests on the operator audit door.** Every credited claim is replayed by an operator
   card not on its panel before `Final` (two from ρ 250). Under that rule the design bar holds literally
   (T7: the max run-away with an honest pool is 0 at 1,000 and at 2,030 claims), assuming the operator pool
   is honest — the trust the operator anchor already carries for J-1 — and the replay is complete (G2′, the
   burn-in). **Recommend yes.** Consequences: one operator replay per credited claim (12.5 a DAA per host at
   100 claims a DAA, 125 at 1,000); credited claims wait (never burn) while the operator's auditors are down;
   8k credit waits for operator GPU audit capacity; before the stopgap retires, the credit needs a new design
   (bonded auditors and, if audits are sampled, a campaign reduction). The alternative — accept an
   expected-value reading with sampled audits — is not recommended (T4b; the CRITICAL finding).
2. **D-15′ — AG-6: automatic covering-signer charge on a refutation of a credited claim** (as DA-7 already
   charges on a default), from the first credited step, after the D-5 burn-in. **Recommend yes** (margin
   against colluding seats; uncredited claims unchanged).
3. **D-17′ — the tie rule and the published confirmation depth.** Adopt the weight lane's rule (a shallow
   all-economic tie ≤ 2 DAA ticks goes to GHOSTDAG's order) and publish "an economic-tie reorg cannot
   reverse a transaction buried more than 2 DAA ticks (≈ 9 chain blocks, ≈ 4 min)". **Recommend yes.**
   Related (outside this ADR): the DAA-500 release's strict-win alone splits honest siblings on public t12;
   the int-4 release should take the same rule under its own fence.
4. **D-19 — emission at network ×N.** Each claim-bearing block pays one subsidy (4,445.62 MSK), so network
   ×N in claims is ×N emission (4.45M MSK a DAA at 1,000 a DAA). Options: (a) accept; (b) M-1 riders share
   one subsidy; (c) a per-DAA emission budget (a separate ADR). **Recommend deciding before any public step
   targets more than 10 claims a DAA**; per-bond concurrency ×1000 needs no emission change.
5. **D-20 — `H_cap`.** **Recommend** §10.3's rule (e.g. 3,601 if the shadow is live at DAA 1,100), with F-Q
   in the set and the first step credited only if Stage 1's credited drill, G2′ and G9 passed.

## 15. What does not change

Raw PWU and every reader of it; the E amount (except under M-1); the withheld-reward and vesting mechanics
(V-1…V-8); the 500‰ ceiling; the producer (13,000) and seat (130,000) floors; the DA court and its
attribution objects; C7's cap; ADR-0152's charging for old-rule claims; the operator-DA judges' duty for
claims relying on non-operator signers; every rule below each fence; every shipped pin (`b8564b88…`) until
`H_cap` is armed; the DAA-500 and DAA-750 releases (V02's lock life included: its code is untouched; AS-5
applies only to credited claims, behind F-Q).

## Appendix A — the arithmetic

Every number above is printed by `docs/adr/0160-capacity/capcalc_v3.py` (pure integer and exact-fraction
arithmetic; well under a second) into `docs/adr/0160-capacity/numbers-v3.md`: T1 per-bond concurrency; T2
q needed per route (corrected formula, v2's beside it); T3 decision 2's literal; T4 the audit door's campaign
EV and break-even auditor pass rate (k_aud 1 and 2, 1 and 10 producers); T4b the sampled campaign without a
reduction; T5 lane S's caps; T6 network limiters with the DAA-500 lock life; T7 the uncovered-loss table,
literal and expected; T8 the pile; T9 network stages with anchors and operator-only carriage; T10 riders; T11
the breaker's forced-trip costs; T12 lane N's work-conserving room; T13 the audit load. Constants and their
sources:

| constant | value | source |
|---|---|---|
| subsidy a block | 444,562,014,000 sompi | `params.rs:17486` |
| worker carve | 720‰ | `params.rs:17491` |
| E | 320,084,650,080 sompi = 3,200.8465 MSK | `params.rs:17498` |
| w (floor / 8k / 2M) | 10,752,660 / 2,471,600,230 / 5,974,294,206,820 sompi | capacity map §2.2 |
| FCW; collateral per FCW; w_FCW | 604,250,611; 6,500 MSK; 10,752,660 sompi | `rcore/cap-weight@3aa4abec4:…/palw_weight_cap_v1.rs` |
| shallow-tie depth | 2 DAA ticks | `…/palw_weight_cap_v1.rs:139` |
| exposure ceiling κ | 500‰ | `palw_fp_devnet_v3.rs:420` |
| producer floor; seat floor | 13,000 MSK; 130,000 MSK | `palw_fp_devnet_v3.rs:902`; `params.rs:25063` |
| windows: bind, receipt, challenge (`W_disclose`), court, anchor delay | 600, 600, 1,200, 3,000, 20 | `palw_fp_devnet_v3.rs:165–170` |
| lock life (DAA-500 release) | 1,000 | `rcore/int-4@a067f490d:consensus/core/src/palw_state_v2.rs:1477` |
| action multiple; S1/S2; S3/S4; strike epoch / window / threshold; reporter reward | 3; 100‰; 250‰; 1,000 / 7,500 / 3; 1,000 bps | `palw_state_v2.rs:949–970` |
| `q_seat` (liab, now only an audit trail) | 250‰ | `rcore/cap-liab@deccda81b:…/palw_aggregate_liability_v1.rs:219` |
| max steps (v2 → v3) | 8 → 16 | `…/palw_aggregate_liability_v1.rs:102` |
| P* | 500‰ | `rcore/cap-escrow@d6a058249:…/palw_escrow_funding_v2.rs:73` |
| operator judges; turn; deferred audit fraction | 3; 30 DAA; 100‰ | `rcore/int-4@a067f490d:kaspad/src/palw_operator_da.rs:119, :109`; `rcore/cap-verify@29b1cb442:kaspad/src/palw_operator_da.rs:226` |
| max block mass; coverage carrier | 500,000; 125,768 | `body_validation_in_isolation.rs:157`; capacity map §6.4 |
| live operator attempts a DAA | ≈ 2.6 | capacity map §6.5 |
| claim retirement | `max(terminal + 3,000, last session close + 1)` | `palw_state_v2.rs:3637` |

Key derived values: `m_c = ⌈E/ρ⌉` = 320.08 / 128.03 / 64.02 / 32.01 / 12.80 / 6.40 / 3.20 MSK; `S4 = min(25% ×
130,000, 3E)` = 9,602.54 MSK (13 fit a 130k seat); the seat-charge break-even `E/(E + 3E)` = 250‰; seat
capital per floor claim 3,992,004 MSK·DAA with F + 3,000 locks, 1,591,004 with the lock life.

## Appendix B — changelog

**v3 (2026-09-26 night, this text), against v2 (`93395bafa`)** — the second review's twelve findings
(Appendix D):
* **The credit's basis** moves from a sampled detection rate to the audit door (§5.3, §5.9; lane Q, fence
  F-Q): a credited claim reaches `Final` only with `k_aud` receipts from non-panel operator auditors;
  `k_aud = 2` from ρ 250. The EV formula is corrected (X on the licensed branch only; T2); T3 shows
  decision 2's literal row met. D-8's half rule is retired; D-14 superseded by D-23.
* **Withdrawn:** AG-7 (the taint), v2's W-8 (merge-the-sink tie rule), K1, K5, B3.
* **New:** the price record (finding 4), AS-5 — the pre-`Final` lock life and the answer right to maturity
  (findings 2 and 12), the work-conserving share (finding 5), K1′ and
  K6 with K2's distinct-bond floor (finding 6), `PALW_CAPACITY_MAX_STEPS_V1 = 16` and credit-only steps
  (finding 11), G2′ and G9 (finding 9), A15/A16, the anchor and operator-carriage columns of T9 (finding
  10), A14's exact bound (finding 12); §10.4's three observations on the DAA-500 release (strict-win's
  sibling split; DA-7's lost seat charge and X7's producer-only answer past `F + 1,000`), open risks there.
* **Corrected numbers:** seat capital with the DAA-500 lock life (4.46 a DAA past DAA 500; ×ρ with a
  credit); T11's trip costs; T12's per-bond room.

**v2 (2026-09-26 evening, `93395bafa`), against v1 (`e64348af0`):** the four-way split (§3) and separate
lifetimes (§4); decisions 1–4 applied; route pricing with the covering seat's automatic S4; AG-6…AG-10,
AS-1′, W-8, W-9; lanes S, N, B, M; the findings that the fold clones the whole state per merged attempt and
re-hashes it per block, that emission is one subsidy per claim-bearing block (D-19), and that a campaign
without a reduction is profitable (T4b). Its full text is `git show
93395bafa:docs/adr/0160-claim-capacity-separation.md`.

**v1 (2026-09-26 11:29, `e64348af0`):** J-1 staged weight and per-bond cap, `E` off the bond with `m_c =
max(m*(q), ⌈E/ρ⌉)` at `L = 3G`, aggregate liability AG-1…AG-5, seat re-pricing AS-1…AS-4, the verification
lane (F-R, F-B), node-only shadow accounting, five fences at one `H_cap`, attacks A1–A8, a six-stage ramp.

## Appendix C — v1's summary (history, condensed from `e64348af0`)

> **目的。** bond 1 枚あたりに出せる claim の数を、今の数百倍から数千倍にする。ただし次の 2 つは増やさない。
> fork choice の力（claim を 100 倍出しても、fork choice に効く重みは 100 倍にならない）/ 回収できない金銭リスク。
> **何を分けるか。** 今は 1 つの数字に 4 つの役割が乗っている。C：生の PWU（検証に必要な計算量、今のまま）/
> E：報酬の escrow（3,200.85 MSK）/ W：fork choice の重み / m：licence 前の金銭リスク。
> **W（lane weight、最優先）** claim の重みを段階的にする：作成時 0 → anchor 時 1% → licence 時 100%（S2 は 25%）→
> Final で全量。Final 前の重みの合計を bond ごとに `⌊C/6,500 MSK⌋` floor-claim 分で頭打ちにする（13k なら 2 本分）。
> **E（lane escrow）** E は bond から予約しない。bond が持つのは licence 前の義務 `m_c` だけにする。
> **集約責任（lane liab）** 1 件の有罪で bond をまるごと没収する。最初の有罪で bond を freeze する。void しても
> `m_c` は 600 DAA 保持し、その間も有罪にできる。帰属できない失敗（S0′）で取るのはその段階の予約分だけにする。
> **検証容量（lane verify）** batch licence：1 block 3 本 → 30 本以上（×10）。class の room の計算を作り直す。floor にも
> room を設ける。licence 前の速い検証と、後からの深い audit を分ける。
> **shadow（lane shadow、node のみ・先行出荷）** 新しい式での予約・重み・容量・帰属率を RPC とログに出す。
> **数字**（13k / 100k / 1M）今日：2 / 15 / 156。ρ = 100：203 / 1,562 / 15,620。ρ = 1000：2,030 / 15,620 / 156,208。

## Appendix D — the second review of v2 (twelve findings) and their dispositions

| # | severity | finding | disposition | evidence / where |
|---|---|---|---|---|
| 1 | CRITICAL | AG-7's lazy taint burns honest producers' rewards across ≈ 5/8 of the pile; any seat can trigger it for ≈ one S4 | **fixed by removal**: AG-7 withdrawn; the audit door refuses each credited fraud individually, so no campaign reduction exists to cascade. The review's hold-never-burn form is recorded as the requirement for any future sampled design | §5.5 AG-7; §5.3; Proposition 3; A10, A15 |
| 2 | HIGH | `palw_final_lock_life` ends the seat S4 the credit and AG-7 rest on; N1–N3 seat capital assumed F + 3,000 locks | **fixed**: the credit no longer rests on S4 (the audit door); AS-5 keeps a credited claim's pre-`Final` lock at `L + window_court` so the refusal's DA/AG-6 charge lands; T6/T9 recomputed with the lock life (4.46 a DAA past DAA 500) | §5.6 AS-5; T6; T9; §10.4 (ii) records the same gap for today's DA-7 |
| 3 | HIGH | q unobservable on-chain, correlated on ≈ 3 hosts, the breaker blind; T7's 0 only in expectation | **fixed**: audit receipts are consensus objects; an unaudited credited claim waits (no escape during an outage); K6 sees the outage; T7 shows the literal max (0 with an honest pool, N·E if the pool is compromised) beside the expected value; D-23 asks the user to confirm the basis | §5.9; §4.4 / T7; §8.3 K6; A15; D-23 |
| 4 | HIGH | "ρ_eff at accepted_daa, for life" has no storage; release ≠ reserve after a trip; the boundary block's order is unspecified | **fixed**: the price record in the claim's rcore record, read by every later site; the breaker evaluates at the end of the boundary block | §5 (the price record); §8.3; D-T-price |
| 5 | HIGH | lane N's share is not work-conserving and ignores newcomers; the per-bond headline is unreachable | **fixed**: registered unmet demand reserves its share, idle capacity is lent, one unit per piece; T12 shows the headline reached under the live mix and the stake share under contention | §7.2; T12; A12 |
| 6 | HIGH | breaker ≈ 3× cheaper than T11 (19–21k MSK); B3 trips network-wide; a legitimate trip breaks S-I1 | **fixed**: K1 and K5 withdrawn (K1 collects nothing and is near-unreachable), K1′ and K6 added, K2 needs ≥ 3 distinct bonds (≥ 39,000 MSK); B3 withdrawn; S-I1 restated as an admission property | §8.3; §8.4; T11; A11 |
| 7 | MEDIUM | the EV formula charged S4 on unlicensed frauds; true q_needed 220–250‰ | **fixed**: formula corrected (225–250‰ with the conservative licensed-branch accounting); the credit no longer depends on q (d = 1000‰ meets every row) | §5.3; T2; T3; T4 |
| 8 | MEDIUM | W-8 contradicts the weight lane's 2-DAA-tick rule; the published "2 chain blocks" would be wrong | **fixed / rebutted**: v2's W-8 withdrawn — a refused sibling leaves the virtual's parents, so its "next block merges both" never comes; the lane's measured rule adopted; publish "> 2 DAA ticks (≈ 9 chain blocks)" | §5.2; D-17′; W-T12 |
| 9 | MEDIUM | Stage 0's public G2 measures judges-up q or burns genesis seat capital | **fixed**: G2′ measures judges-down detection in the salted drill; public t12 only replays planted captures off-chain | §13 G2′; Stage 0 |
| 10 | LOW | T9 omits the anchor bind limit and uses all blocks where lane N counts operator attempts | **fixed**: T9 carries anchors and operator-only carriage; N5 needs `B_bind ≥ 385` (770 at 2×) and ā_op ≥ 2.86 for 2× list carriage; G7 measures both | T6; T9; §7.1; G7 |
| 11 | LOW | 8 steps are exhausted by the ladder | **fixed**: 16 named slots, credit-only steps | §8.1; D.3 |
| 12 | LOW | A14 understates an honest offline producer's loss | **fixed**: ≤ the 500‰ ceiling (6,500 MSK at 13k) in stage commitments + at most one S1 tier (1,300) + unearned rewards, never the rest of the bond; checking it surfaced a post-`Final` exposure the DAA-500 lock life opens (X7 answerable only by the producer past `F + 1,000`), closed for credited claims by AS-5 (2) and reported for the rest | §5.5 D1; §5.6 AS-5; A14; §10.4 (iii) |
