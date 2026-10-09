# ADR-0177 — Model-bond coinbase allocation without model-availability consensus

* **Status:** Accepted design direction at the user's request, 2026-10-10, after the Seeder/FPR amendments. Allocation parameters, economic validation, implementation and activation remain pending.
* **Scope:** Future PALW model distribution and model-specific coinbase allocation, with Panels or the separately gated Panel=0 mode.
* **Supersedes:** MISAKA Torrent adoption/integration, dedicated Bonded Seeder roles/rewards, and the model-availability parts of [ADR173 D9–D11](0173-public-verifier-dispute-completeness-is-misaka-purpose.md), [RFC14 §16](../rfc/0014-panel-independent-fraud-prosecution.md), RFC15 and their related amendments. Mandatory Torrent, independent Bonded Seeder requirements, PoR/Full Fetch leases, TRDC/FPR model-disclosure gates and availability-based reward/weight suspension are withdrawn.
* **Preserves:** [ADR175](0175-registered-models-are-permanently-immutable.md)'s permanent model identity, [ADR176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)'s per-bond production/reward/Final-weight ceilings and common DAA hold, authenticated computation adjudication, existing monetary issuance constraints and old-claim rules.
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
