# ADR-0177 — Model-bond coinbase allocation without model-availability consensus

* **Status:** Accepted design direction at the user's request, 2026-10-10, after the Seeder/FPR amendments. Allocation parameters, economic validation, implementation and activation remain pending.
* **Scope:** Future PALW model distribution and model-specific coinbase allocation, with Panels or the separately gated Panel=0 mode.
* **Supersedes:** MISAKA Torrent adoption/integration, dedicated Bonded Seeder roles/rewards, and the model-availability parts of [ADR173 D9–D11](0173-public-verifier-dispute-completeness-is-misaka-purpose.md), [RFC14 §16](../rfc/0014-panel-independent-fraud-prosecution.md), RFC15 and their related amendments. Mandatory Torrent, independent Bonded Seeder requirements, PoR/Full Fetch leases, TRDC/FPR model-disclosure gates and availability-based reward/weight suspension are withdrawn.
* **Preserves:** [ADR175](0175-registered-models-are-permanently-immutable.md)'s permanent model identity, [ADR176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)'s per-bond production/reward/Final-weight ceilings and common DAA hold, authenticated computation adjudication, existing monetary issuance constraints and old-claim rules.
* **2026-10-12 誤読防止:** 本ADRが外すのは**モデル配布**の合意管理であり、Miner・Panelのモデル保有義務と個別claimの検証・裁定・Final条件は外さない。[D1補足](#d1補足--役割の区別2026-10-12誤読防止)と[D7注記](#d7--非強制取得の下での検証経済安全性)を先に読むこと。Miner＋Panelのclaim bond合算と1.3〜2倍の配分係数は末尾の[2026-10-12改定](#改定--2026-10-12minerpanelのclaim-bond合算と1320倍の配分係数提案)（提案・未決）。
* **Implementation:** No code, wire id, Params field, activation DAA, fingerprint or deployment is assigned by this document. The earlier availability/FPR document checks record a withdrawn design, not current acceptance or economic evidence.

## 1. 採択する原則

**チェーンはモデルを入手できるかどうかに一切干渉しない。** モデル配布は参加者のP2P・市場活動とし、
公開を促す中心手段を、モデルへ予約・拘束されたminer bond元本に連動するcoinbase配分とする。
モデル別の報酬プールを固定せず、参加資本を集めたモデルの配分weightを拡大する。

目指すのは、閉鎖集団が自己資本だけで採掘するより、モデルを第三者へ公開して多くの参加資本を集める方が、
大きな配分倍率と利益を得る構造である。外部参加による増加を**圧倒的に有利にする**ことを経済設計の目標とする。
ただし、bond金額だけで資本の実質所有者・独立性を識別できるとはしない。
この目標を達成したとの宣言には、倍率曲線と敵対的経済評価が必要であり、現時点では未立証である。

## 2. 規範的決定

### D1 — モデル配布を合意の管理対象から外す

**MISAKA Torrentの採用・統合方針、専用Bonded Seederのrole/契約、Seeder報酬の概念を廃止する。**
新しいPALW制度としてこれらを実装・activationしない。一般的な任意の配布手段はoff-chainで選べる。
合意はSeederの登録・選出・人数・IP・地域・独立性・保有コピー数・転送量・速度を管理しない（MUST NOT）。
モデルのPoR、全量取得監査、availability lease、TRDC、FPR、全weightsの強制公開費用予約や
モデル配布用のCHALLENGED/SUSPENDEDを新しい報酬条件として要求しない（MUST NOT）。
モデル配布停止、取得失敗、低速化、第三者への提供拒否だけで、登録・claim受理・報酬・Final weightを
無効化し、bondをslashし、または合意上のサービスペナルティを課してはならない（MUST NOT）。
取得監査の失敗を算術的な不正証拠にせず、モデル未取得だけで反証期間を無限延長しない。

一般的なP2P配布、DHT/PEX、HTTP mirror、任意の保存・共有契約はoff-chainで選べる。
MISAKA固有の取得プロトコルやSeeder制度は設けず、Panel/miner/一般peerの共有は任意である。
外部の配布契約をchainのminer bond責任へ自動変換せず、専用Seederへの固定15%配分も採用しない。

### D1補足 — 役割の区別（2026-10-12、誤読防止）

D1は**モデルの配布**を合意の管理対象から外す決定であり、**計算・検証に参加する者の保有義務**や**個別claimの検証条件**を外す決定ではない。次の3つを区別する。

1. **Miner・Panel:** PALW計算に参加するMinerおよび検証を担当するPanelは、適用されるrulesetに従い、正しい固定Model ID/root/specに対応するモデルを保有し、モデル同一性を確認でき、必要な実行・検証能力とreadiness条件を満たさなければならない（MUST）。
2. **モデル登録者・所有者:** 第三者へのモデル配布、取得成功、継続的な公開提供を合意上保証する義務を負わない。モデルを配布できない、または配布を拒否したという理由だけで、登録モデルの報酬資格を停止したり、登録者に制裁を科したりしない（MUST NOT）。
3. **個別claim:** 実際の計算・検証・裁定・Finalに必要な条件は満たさなければならない。モデルの配布自由と、未検証の計算への無条件支払いは別問題である。

必要なPanelが成立しない個別claimを受理しないことは、モデル登録者の配布義務違反や計算不正の認定を意味しない。モデルの取得不能だけを理由とする登録資格の取消し、既存の有効claimの報酬停止、slashは行わない。
モデルへのアクセス結果、peerの接続状態、配布者の応答状況を、合意判定への暗黙の入力としてはならない。Miner・Panelの資格は、適用rulesetで定義された認証済みの状態・証拠によって判定する。
仕様上は「**第三者がモデルを取得できなかった場合**（登録者の義務違反でも不正でもない）」と「**必要なPanelが成立しなかった場合**（そのclaimは検証条件を満たさないため受理・Finalされない。非不正の終了）」を区別して扱う。
RFC-0002 Proposal Aの `class_seated`（executor以外のモデル保有operatorを `seat_count` 以上、registrant・executorを除いた独立operatorの下限）は、この保有義務と両立し、claim受理・報酬資格・登録者の責任の関係を同時に満たすよう検証する。
詳細は[RFC/ADRレビューの整理（2026-10-12）](../design/palw/rfc-adr-review-resolution-2026-10-12.md)。

### D2 — 入手性と登録モデルの同一性を分離する

Model ID、Canonical Artifact Root、canonical manifest、weights・tokenizer/config・実行仕様の固定bindingを維持する。
登録後の内容変更は別のModel IDとして登録する。claimは同じ固定Model ID/root/specを参照し、
取得したbytesはpiece/shardの認証と全量復元後のcanonical-root照合を行う。
任意のTorrent descriptor/infohashを登録する場合、そのrootとの対応もimmutable bindingとするが、
infohashの登録やpeer到達性を報酬資格の必須条件にしない。root一致は実行正当性の証明ではない。

既存claimのstate/trace/output/opening等のcommitment、適格な有限court要求とその回答義務は維持する。
これは**個別claimの認証済み証拠を裁く責任**であり、全モデルの取得・公開を強制する権限ではない。
新routeの要求対象はclaim固有のwitness/state等に限定し、モデル取得を目的とするweight-file/range要求、
全weightsを列挙する要求、反復要求によるモデルの再構成を禁止する。
実装前に許可unitと累積scopeを固定し、既存unitでmodel bytesを要求できる場合は新rulesetで制限する。
court側が必要なmodel operandをどう認証・検証するかも別途定義し、この制限でterminalが成立しないprofileを
「裁定可能」と表示しない。claim固有証拠の正当な要求・署名付き虚偽・客観的計算不正の責任と、
off-chainの配布拒否を混同しない。

### D3 — モデルごとの拘束元本を重複なく集計する

配分期間/epoch `t`の受理済みsnapshotから、producer bond `b`が固定Model ID `m`へ割り当てた
適格な拘束元本を`C_{b,m}(t)`として定義し、次をMUST満たす。

~~~text
0 <= C_{b,m}(t)
sum_m C_{b,m}(t) <= C_b_effective_locked(t)
S_m(t) = sum_b C_{b,m}(t)
~~~

claimの予約件数・予約額の単純合算をモデル資本として扱わない。同じ元本から1000claimを発行しても
`S_m`を1000倍にしない。rho、再送、root/slice/rider、bond/key分割、モデル複製でも資本を重複計上しない。
model ownerの市場seed、AMM流動性、Position購入、Panel/配布契約の担保をminer元本へ自動算入しない。

新しい配分権に使える実効元本、既存責任控除、slash/退出、model間移動、期間途中の増減を定義する。
一つの本物の元本が責任と配分の両方に現れる場合も別の追加元本を作らず、回収可能性を維持する。
共通DAA拘束と整合したsnapshot/積分方式を選び、短期借入・epoch直前だけの拘束・移動による二重配分を防ぐ。
7日間は説明例であり、配分epochとADR176の`W`の具体値・対応は未決である。

### D4 — 固定された総発行予算の中でcoinbaseを配分する

非負の単調な配分関数`f`を用い、`f(0)=0`とする。正の分母があるときの設計形は次とする。

~~~text
A_m(t) = f(S_m(t))
R_m(t) = R_PALW(t) * A_m(t) / sum_j A_j(t)
sum_m R_m(t) <= R_PALW(t)
~~~

`R_PALW`は採択済みcoinbase/subsidy・DAA発行制約内のPALW予算であり、新しい追加mintではない。
整数算術・overflow・丸め残余・zero denominatorを規範化し、分母0ならモデル配分は0として
未配分残高の処理を既存発行上限内のversioned規則へ渡す。未配分・未使用予算を架空の報酬として発行しない。
使い切れないモデル枠をどう扱うかもrelease前に固定し、miner capを超える自動再配分を許さない。

他モデルの資本が固定なら、当該モデルへの資本追加で相対配分は非減少とする。
他モデルも増資する場合に絶対額が必ず増えるとはしない。全モデルの配分合計は一定の総予算内にある。
資本預託だけでは支払わず、当該モデルの有効claim、固有の計算仕事、Finalと適用会計を満たしたものにだけ支払う。
既存のowner/Panel等のlegとの接続と帰属を明示し、同じcoinbaseを別legから二重支払しない。
**2026-10-14改定:** モデル配分のcoinbaseにowner/登録者のlegは置かない。マイナー報酬は登録者へ直接分配せず、登録者の収入はModelPositionの売買手数料とする（末尾の2026-10-14決定）。

### D5 — モデル配分とminerの採掘枠を別々に制限する

モデル別coinbaseは集合の**配分予算**であり、個々のproducerの共通上限を増やす規則ではない。
各bondのclaim/block/reward/Final-weight上限、受理時予約、最早回復`d + W`、残存責任をADR176に従って維持する。
公開により外部minerが増えればモデル集合の資本・配分は増え得るが、既存minerの最大採掘枠は増えない。
計算省略、閉鎖、model変更、倍率上昇でも同額bond・同期間の上限を増やさない。
新claim受理時に適用model/snapshot/配分権を固定し、coinbase・未払escrow・成熟・Finalの全経路で
ネットワーク予算、モデル予算、個別bond予約/上限を同時に検査する。

実際の支払はmodel予算とbond上限の両方以下とし、配分上限と実収入を区別する。
可変model予算から過去claimへ無予約の追加権利を遡及付与しない。
coinbase配分変更だけでは物理block頻度・ticket/DAA/難易度・fork choiceを変更しない。
将来それらをモデル資本に連動させるなら、別の明示的設計・review・upgradeを要する。

### D6 — 公開の経済優位を評価し、所有者識別を偽装しない

**同じ`S_m`と他モデル状態なら、自己資本か外部資本かによらず配分は同じである。**
ウォレット数、IP数、別bond、参加者の自己申告を独立資本の証明として倍率に使わない。
permissionlessな金額会計だけでは閉鎖集団と独立した参加者を識別できない。
したがって「外部資本だから別の高倍率」を合意上保証する式を本ADRで発明しない。

公開の利益は、公開によって追加の実資本・有効な計算参加が集まることから生じるように設計する。
`f(S)=S`だけでは既存minerの収入増を保証しない。superlinearな曲線も、資本集中・自己増資・Sybil・
報酬予算の飽和を悪化させ得る。modelを公開しただけのボーナスや取得成功票による倍率は導入しない。

採択候補の`f`、上限・逓増範囲、model内配分、他モデル競争と個別capを一体で評価する。
公開後の既存miner/ownerの実利益を、外部参加者との取り分、自己増資の資本費用、配布費、
検証費、slashリスク、資本移動と外部利得まで含めて比較する。
圧倒的な優位の数値基準・対象資本範囲・市場仮定を評価前に固定し、成立しない条件も報告する。
全資本を自前で用意できる攻撃者まで、公開が常に利益最大になるとは保証しない。
閉鎖・同額自己増資に対する優位が示せない場合は「問題を塞いだ」とは記載せず、未解決として残す。

### D7 — 非強制取得の下での検証・経済安全性

G14は、検証者が正しい登録モデルを実際に保有/取得できた条件で、Panel外の認証証拠から
不正をlocalizeし、有限のexact courtへ進める能力を検証する。全所有者が配布を拒否しても第三者が
必ず取得できるという旧保証は撤回する。modelの未取得は計算不正の証明でも資格停止条件でもない。
必要なmodelを得られず監視できない場合、実効検出確率`p`は0にもなり得る。
`p * (R_risk + L_collectible_net) > C_saved`は実効`p`・期限内包含・徴収可能額の仮定付きであり、
model資本増加やclaim capだけで成立したとしない。閉鎖して検出を避ける利益もD6の評価へ含める。

モデル入手性による停止を外しても、有効な計算反証、claim固有witnessの客観的default、
未解決の適格court、rulesetのFinal条件、予算違反はそれぞれの規則で処理する。
Panel=0は別の実装・fresh-verifier試験・経済評価・協調activationを要する。
閉鎖モデルへの実効検出が未立証である間は、その経済安全性も未立証と報告する。

**D7注記（2026-10-12、誤読防止）:** 上の「必要なmodelを得られず監視できない場合、実効検出確率`p`は0にもなり得る」は、**モデルを保有する独立した検証者が一人もいない場合**の話である。
D1補足のとおりMiner・Panelはモデル保有が必須であり、独立したモデル保有Panelが検証できる場合、閉鎖モデルでも`p`は0とは限らない。「閉鎖モデルなら必ず`p=0`」とは読まない。
残る課題は、Panelの共謀・独立性の偽装・検証能力不足をどの程度抑止できるかであり、operator IDが別でも実質所有者が同一である可能性は残る。

## 3. 完成条件と移行

| Gate | 必須の証拠 |
| --- | --- |
| Non-interference | 同一chain/claim/proofを入力し、全peer停止・配布拒否・ローカル取得結果を変えても合意判定、報酬資格、weightが変わらない。モデル要求ではcourt/Slashを起動できない |
| Immutable identity / court scope | 別weights/configを拒否。claim固有証拠とモデル取得要求を型・累積scopeで分離。許可scopeから実際のterminalへ到達できる |
| Distinct capital | 同じ資本の多claim、多model、key/bond/model複製、rho変更、退出/移動/epoch境界・借入で二重集計しない |
| Allocation conservation | model増資/競合増資、分母0、整数丸め/overflow/残余、未使用枠、旧新rulesetで総coinbase・各model・各bondの予算を維持 |
| Same-bond opportunity | 正直な計算と高速偽造で同じQ/B/R/F最大枠と共通DAA時計。model倍率変動で予約や枠を回復/増額しない |
| Open versus closed economics | 閉鎖自己資本、同額自己増資、公開外部資本、全資本自前、Sybil/model分割、借入/退出、資本集中、競合増資、既存minerのcap飽和、閉鎖によるp低下を比較 |
| Recovery / activation | snapshot・元本帰属・配分権・支払・courtをreorg/restart/IBD/pruningで一致再生。既存責任/消費履歴を保持し別upgradeで有効化 |

現行[palw_reward_v2.rs](../../consensus/core/src/palw_reward_v2.rs)のsubsidy carveと
[ADR167](0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md)の
DAA emission budgetは総発行制約の参考であり、`S_m`/`f(S_m)`の実装証拠ではない。
以前のSeeder/FPR、ADR176、t12監査のPASSを、この配分式・公開優位・Panel=0の完成証拠へ流用しない。
詳細は[RFC14 §16](../rfc/0014-panel-independent-fraud-prosecution.md)と
[RFC15 §8.5](../rfc/0015-panel-free-permissionless-verification.md)へ接続する。

この改定はdocsの設計変更であり、既存ネットワーク・旧claimの取得/DA/court規則を遡及的に解除しない。
既存model operandの扱いを含むversioned移行、全受入gateと未決の倍率式を確定するまで新modeを有効化しない。

## 目的の改定 — 2026-10-10（bond集約優位）

ユーザーの決定により、本ADRの経済目標を**「公開そのものを優遇する」から「モデルへ実際に拘束された適格miner bondを多く集めたモデルを強く優遇する」**へ変更する。

> **モデルへ実際に拘束された適格miner bondが多いほど、モデル別coinbaseの配分を強く優遇する。公開は、他のminerが参加してbondを集められるようにするための手段であり、公開という申告自体には報酬を与えない。自己資本・外部資本、運営者・一般参加者を区別しない。**

- **D6の改定:** 「同額の自己増資より公開が常に有利」の証明はリリース要件から外す。ECONの反例（自己資本は任意の`f`で公開に勝ち、`p=0`では閉鎖が有利）は旧目標の限界を示す記録として保持する。
  新しい受入要件は「多くのbondを集めたモデルの優遇」の定量化である（対象資本範囲、倍率、配分予算・実支払・純利益のどれについて成立するか、個別cap未到達の領域）。
- **配分曲線の候補:** `A_m = S_m^α`（`α > 1`、比較候補`α = 2`）、`R_m = R_PALW · A_m / Σ_j A_j`。増えるのは固定予算の取り分であり、総発行量ではない。`α`は未承認のpolicy値である。
  ECONは「大きな公開モデルへの参加」と「小さな閉鎖モデルでの自己採掘」の単位資本利益を、個別cap・費用・モデル内配分・資本集中を含めて比較する（単純化した比`(S_public/S_closed)^(α−1)/d`は成立可能性の説明であり、証明ではない）。
- **大口資本の受容:** 悪意ある主体でも本物の資本を大量に拘束した場合、それだけを理由に配分から排除しない。資本集中による報酬集中と支配リスクは残余リスクとして明記する。
  2乗配分は資本集中を積極的に優遇し、運営の初期資本も有利にする（身分の特権がないことと、初期資本の優位が小さいことは別である）。
  少ない実資本を大きく見せる行為（複数モデルへの重複計上、epoch直前の移動、鍵・claim分割による増幅）はD3のとおり引き続き禁止する。
- **p=0の分離:** モデルを閉じた参加者が計算しないclaimを報酬まで通せる問題は、配分曲線の変更では解消しない。ADR-0176 D6の`p(R_risk + L_collectible,net) > C_saved`は`p=0`で成立しない。**2026-10-14改定:** これは有効化を止める安全性gateではなく、ADR-0176のbond単位のBlock / Reward / Final Weight上限・共通拘束期間・Final前払い出し禁止で被害を有界化する残余リスクとする（[PRINCIPLES §0](../PRINCIPLES.md)）。
  資本集約優位の成立をその解消証拠に流用しない。報告は「正直なverifierが機能する範囲（G14・裁定・抑止）」と「全所有者が閉鎖し実効`p=0`となる範囲（配分曲線による利益比較と、残る不正収益・合意への影響）」に分ける。
- **範囲:** 変更するのはモデル別coinbase配分だけである。個別bondのQ/B/R/F上限、元本の重複禁止、共通拘束期間、総発行予算はADR-0176・D3–D5のまま維持する。
  bond集約倍率をblock発行数・beacon・Final weight・fork-choiceへ掛けることは含めない。
- **検証承認gate（ECON round 2の設計）の位置づけ:** 補助金を検証承認に条件付ける機構として休眠実装を続けてよいが、`p=0`の解消証拠には数えない。

## 改定 — 2026-10-12（Miner＋Panelのclaim bond合算と1.3〜2倍の配分係数：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。
ユーザーの追加方針として、**モデルに実際に拘束されたMinerおよびPanelの適格claim bond元本の和**に応じた**1.3〜2倍の配分係数**を、モデル別の報酬配分（とブロック生成機会の配分）に反映する設計を提案として採録する。全文と攻撃試験C01〜C18は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3A。

> モデル別のブロック生成機会と報酬配分には、そのモデルに実際に拘束されたMinerおよびPanelの適格claim bond元本の和を使用する（MUST）。同じ実元本を複数claim・role・modelで重複計上してはならない（MUST NOT）。
>
> 適格元本に応じた1.3〜2倍の配分係数を設計候補とし、公開によって実参加資本を集めるモデルを、自己資本だけの閉鎖的な運用より相対的に有利にすることを目的とする。倍率曲線・閾値・snapshot・評価期間・整数丸め・model内配分はrelease前に確定する。
>
> claim bondは、正常claimの検証・Finalおよび共通の枠回復・責任保持条件を満たす前に再利用してはならない。Finalの故意の遅延、challenge、Panel交代で評価期間・採掘枠・絶対期限を延長してはならない。
>
> 配分係数は検証とFinalの代替ではない。network/model/producer-bondの上限を同時に維持し、既定の個別絶対capや総発行を後付けの乗算で増やしてはならない。モデル資本にはPanel元本を含めても、producer元本として二重に扱わない。
>
> モデルの非公開や第三者への提供拒否だけで登録者を罰しない。同じ適格元本・条件なら、自己資本と外部資本を同じ規則で扱う。公開の利益は追加の実参加・実資本から生じるという仮定を、敵対的な経済試験で評価する。
>
> 通常claimのPanel=1、必要な正の検証証拠、non-seatの客観反証、N1の実効検出・経済評価は維持する。モデル拘束資本倍率を追加しただけで、Miner/Panel共謀に対する安全性を証明済みと宣言してはならない。

候補式（未採用）: `S_m = C_miner_claim(m) + C_panel_claim(m)`（資金単位で重複なし）、`S_m > 0` のとき `A_m = S_m · μ(S_m)`、`1.3 ≤ μ ≤ 2.0`、`S_m = 0` のとき `A_m = 0`。倍率は配分重みの段階で一度だけ反映し、実支払・ブロック数へ重ねない。全モデルへ一律の係数を掛けて正規化しても相対配分は変わらない（資本に依存する曲線が必要）。

**既存決定との関係（未決として明記）:**
- **曲線:** 2026-10-10の目的改定と同日のユーザー裁定で暫定採用した `A_m = S_m^α`（α = 1.5、BUDGETの休眠実装 `palw_model_bond_allocation_v1`）と、本提案の `S_m · μ(S_m)`（1.3〜2.0の有界係数）は別の曲線である。どちらを採るか、または `μ` をどう定めるかは未決であり、本改定は暫定のα = 1.5を置き換えない。
- **集計対象:** 2026-10-10の改定はminer bondのみを集計した。本提案はPanelのclaim bondも合算する（ただしproducerのQ/B/R/F上限の元本としては二重に扱わない）。新しいpolicy/versionと移行規則を要する。
- **範囲:** 2026-10-10の改定は「変更するのはモデル別coinbase配分だけ。block発行数・Final weight・fork-choiceへは掛けない」とした。本提案はブロック生成機会の配分への接続を含むが、物理block頻度・DAA・fork choice・Final weightの変更は決定しておらず、Final weightへ係数を自動乗算しない。ブロック機会への接続はRFC-0008/0010/0012で別review・versioned activationを要する。
- **評価:** 閉鎖攻撃者が同額の実資本を拘束できれば同じ係数を得る。倍率だけで全Panel共謀や`p=0`の計算省略益を排除できるとはしない（2026-10-10改定の「p=0の分離」を維持）。

## 最終Goalと達成条件の改定 — 2026-10-14（ユーザー決定）

[PRINCIPLES §0](../PRINCIPLES.md)に従う。本ADRの配分は、MISAKAの最終Goal「**モデルを非公開にしておくことが経済的に不利になる仕組み**」を担う。不正の完全排除は目標にしない。

- **非公開は不利になる。** chainはモデルが共有されたかを判定せず、拘束されたbond資本と検証の成立だけを読む。非公開は二つの経路で不利になる。(1) 公開モデルへbondが集まる結果として、閉鎖モデルが相対的な配分を失う。(2) 検証attestation gate（ECON round 2、readiness §3f）は維持し、bondから抽選された検証者が自分のコピーで検査して所定数が承認しないmodel-epochにはモデル配分を払わない。誰にも共有されないモデルは検証が成立せず配分を受けられない。理由は「検証が承認されなかった」であり、D1の「配布を合意で管理しない」と両立する（2026-10-12改定の「非公開や提供拒否だけで登録者を罰しない」とも両立する。罰するのではなく、検証が成立しないmodel-epochに補助配分を払わないだけである）。配分曲線は末尾の「モデル配分の確定 — 2026-10-14」で`α = 1.5`に確定した。配分曲線は現行の暫定値`α = 1.5`（readiness §3g）のままとする。
- **共有されたモデル:** 1人にでも共有され、正しいPanel（正直な検証者）を建てられれば、claim固有の公開・認証済み証拠から不正を客観的なcourtで立証しslashできる（ADR-0173）。
- **非公開モデル（`p = 0`）:** 上の「p=0の分離」の改定のとおり、有界化する残余リスクであり、未解決のgateではない（2026-10-12改定の「p=0の分離を維持」もこの意味で読む）。
- **達成条件（D6の受入要件に追加）— 二段で評価する。**
  - **(a) モデル全体:** 非公開のときと、共有して外部bondを集めたときで、モデルの総bond・配分割合・経済規模の差。例（他モデル900M、自己100M、外部200M参加）: 総bond 100M→300M、配分割合は`α = 1.5`で3.6%→16.1%（`α = 1`なら10%→25%）。
  - **(b) 所有者個人:** 元の所有者の取り分が共有で増えるか。モデル内の配分と、自己資本を追加してbondを増やす場合の資本コスト・期待収益の差を含める（自己資本の追加は禁止しない）。
  - モデル全体の伸びを所有者個人の伸びとして報告しない。

## モデル配分の確定 — 2026-10-14（ユーザー決定）

2026-10-12改定の提案のうち、**Miner＋検証者の元本合算**と**ブロック生成機会への接続**を採用し、配分曲線は**`α = 1.5`**に確定する。`S_m · μ(S_m)`（1.3〜2倍）は採用せず、比較試験用に保持する。正式式・表・試験は[RFC-0015 §8.5.5](../rfc/0015-panel-free-permissionless-verification.md)に置く。

- **式:** `S_m = C_miner_claim(m) + C_verifier_claim(m)`（資金単位で重複なし）、`S̄_m`は30日相当DAAの平均、`P_m = S̄_m^1.5 / Σ_j S̄_j^1.5`（分母0なら0）。
- **適用先:** `P_m`をモデル別のブロック生成機会とcoinbase予算の共通目標にする。1 block当たりの報酬に再び掛けない。network全体のblock頻度・DAA・難易度・fork-choice・Final weightは変えない。上の「範囲」の「block発行数へ掛けない」は、モデル間のblock機会の配分についてこの決定で改める。
- **検証者の元本:** 2026-10-12改定の「Panelのclaim bond」を、Panel=0に合わせて「claimに実際に紐付いて拘束され、検証上の責任を負うpermissionless verifierの元本」へ一般化する。固定seatは要らない。待機bondは数えず、MinerとVerifierで二重計上しない。Verifier元本はMiner個人のQ/B/R/F capを増やさない。参加手順・責任・拘束期間・報酬条件と「預けるだけで検査しない」対策はrelease前に定義する。
- **個別上限:** 共通拘束期間`W`は7日相当DAA。各bondのQ/B/R/F capはモデル配分で増えない。
- **マイナー報酬はモデル登録者へ直接分配しない。** モデル別のcoinbaseとブロック報酬はclaimを生成したMinerと検証責任を果たしたVerifierにだけ帰属し、登録者・ownerのlegを置かない（[ADR-0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)の「報酬からのowner service feeはNo」を維持）。登録者の収入は**ModelPositionの売買手数料**（売買ごとのBILI額の5 %、[ADR-0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md)。fence `palw_model_leg_v2`でADR-0087の1 %から引き上げ。別に5 %をburn）であり、モデルが使われPositionの売買が増えるほど増える。報酬の5 %による買い上げ（ADR-0091）はlegなしで価格を上げるだけである。登録者が自らMiner/Verifierとして参加する場合は、その役割の報酬を同じ規則で受ける。
- **二段評価の(b)所有者個人:** 所有者の収入は、Position売買手数料と、自らMiner/Verifierとして得る報酬の和で測る。モデル配分の伸びが直接登録者の収入になるとは扱わない。
- **必須の受入試験:** `α = 1.5`は資本集中を強める。新規モデルが採掘機会を失いすぎないことを有効化前の必須評価とする。

## 休眠実装 — 2026-10-15（上の2026-10-14決定の実装）

**休眠実装（未build・未検証、`palw_model_bond_allocation_v1`は全presetで`None`、有効化は拒否）:** V3 allocation policy（`PalwAllocationAverageV1`: 30日、verifier元本、block機会。validationで`α = 1.5`と30日を固定）。`roll_epoch_v3`がepochごとにminerの成熟した割当とPanel seatが生存claimに拘束した額（`seat_exposure`）をbondごとに合算・元本でpro-rata clipして`S_m(k)`を記録し、30日分の平均`S̄_m`から`A_m = ⌊S̄_m^1.5⌋`を書く。`model_blocks_available_v1`は1 DAA = 1 block機会として`⌊accrued · A_m / ΣA⌋ − reserved_m`を返し、claimのplanが不足を拒否（clipしない）、commitが予約する（riderは先頭blockで計上済み）。待機bond・登録料・AMM/Position・未払報酬は数えない。所在: `consensus/core/src/palw_bond_budget_v1/allocation_v3.rs`、`palw_bond_budget_fold_v1.rs`（epoch roll、plan/commit）、試験source `consensus/core/tests/adr0177_allocation_v3.rs`。

**未了:** 利用者escrowをmodel予算で削らない修正（RFC-0016 P4、`eee1710ce`）はmerge済み。coinbase由来のclaimについて、V3のmodel別報酬対象block権利とcoinbase側のR clip（`palw_model_available_v1`）が`P_m`を二重に適用しないよう解消する必要がある（ユーザー判断待ち）。実装したblock機会は報酬対象ブロック権利の層（claimが拒否されても物理blockは残る）であり、RFC-0016 D-Q1の物理層（V+、Draft）は未実装。新規モデルが`α = 1.5`で採掘機会を失いすぎないことの評価、verifierの参加手順・責任・拘束期間・報酬条件（預けるだけで検査しない対策を含む）の定義は有効化前の必須事項（ユーザー）。
