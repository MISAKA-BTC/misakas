# RFC18 / PALW DAG並列性のコード評価

## 結論

関連fenceが有効という前提で、主にPALW Attempt/V2の生成・merged-work・Panel binding・Gate C・frontier経路を追った。

**モデルごとのminerが並行して生成し、claimごとのPanelが並行して検証し、BLUE/REDを含む複数の仕事を一つのDAGへ取り込む方向は、仕様だけでなく実コードに接続されている。全ネットワークから一人のproposerを選び、他モデルを待たせる構造ではない。**

ただし、**全モデルが互いの進捗に一切影響されず、実負荷でもFinal・認証・支払いまで完走できる完成状態は確認できない。** 共有資源の上限、共有safe frontier、未接続のDAG幅schedule、受入試験の不足が残る。単なる「testnetで無効」を未完成理由にはしていない。

対象ZIP: `misakas-pre (1).zip`
SHA-256: `0c356b39f40d57d8c4d4d69321f38d38b766865024255f38897bfe8206c3b8b0`

証拠は `source_evidence.md` のD01–D18に、元ファイルの行番号付きで保存した。

## 判定表

|観点|判定|証拠|
|---|---|---|
|同じ親から複数モデルのブロックを生成|構造上可能。各attemptのticket閾値・bond予算を判定。全体一名の選出ではない|D02/D03|
|複数tipを後続ブロックに結合|実templateから複数親をヘッダへ接続|D02|
|RED siblingの仕事を採用|DAA窓内で適格なmerged workをBLUE/RED両方から読む|D04|
|各claimの帰属|元のcarrying block・bond・classを保持|D04/D14|
|モデル間の公平な採用順|同一mergesetのattempt枠でmodel round robin。selected parent既採用分を巻き戻す競争ではない|D05|
|各claimのPanel|claim ID別seedとclass readinessを使う。Panelメンバーの重複は必ずしも不正ではない|D06|
|REDの報酬|適格in-DAA REDのworker shareを本人へ。escrow控除・entitlement条件付き|D07|
|独立した合意credit|モデルの通常Panelだけで完結せずglobal Gate Cを使う。複数claim行をsweepする|D08|
|認証待ちでもDAG進行|仕様に分離。両forkのcreditゼロならGHOSTDAG比較を用いる実分岐あり|D01/D13|
|全モデル完全独立の最終確定|そうではない。最古unresolvedが共有resolved prefixを制限|D09|
|任意数モデルへのスケール|未確認。共有capacityと固定幅、幅schedule reader不足|D10/D11|
|実モデル/実Panel/実報酬の全経路受入|不足。現在のsibling試験は主として構造・claim記録・順序整合の検査|D14/D15/D16|

## 重要な区別

### 1. 並列生成と、状態を処理する順序は別

DAGに同時生成のブロックを残しつつ、全ノードが同じ状態に収束するため、最終的な受理・状態更新は決定的な順序で行う。selected parentが一つあること、foldが順番に動くこと自体は、モデル別minerの並列生成を否定しない。

同一の担保・同一の実行権利の二重使用まで独立扱いするわけではない。共有予算を消費した結果、後続の適格attemptでも容量不足で採用されない場合がある。D04/D10。

### 2. 通常PanelとGate Cは別

通常Panelはclaim別に結び付き、モデルの能力・readinessを読む。一方Gate Cはproducer operatorを除くglobalな適格bond poolを使い、モデル保有者だけに閉じない。これは一つのモデル関係者だけで合意creditを自己認証させないための別段階である。D06/D08。

外部正直1主体が客観不正を発見・告発できるというG14の性質と、Gate Cの抽選・肯定数の成立は別条件である。「正直Panel1つがオンライン」から全ネットワークの認証・収束を直接結論しない。

### 3. shared frontierの遅延範囲

`palw_state_v2.rs:15043–15065`は、最も古いunresolved claimより前までをresolved prefixとし、その中の認証済みFinalをfrontier候補にする。

例: score100でAが未解決、score110でBが認証済みFinalなら、resolved_through=99でBはそのprefix外になる。ただしこれはBのブロック生成、Bの個別Final、全支払いが一律停止するという意味ではない。

FinalだがGate C未認証のclaimはresolved扱いで、未認証であることだけではprefixを止めない。D09。

### 4. 幅と上限

2分目標の設定ではk=1、親上限10、mergeset上限180、merge depth30。k=1は「ネットワークで一人しか生成できない」ではなく、DAGの色分けに関係する。親上限10もモデル数上限10ではない。D02/D11。

幅scheduleは型・ID・構造検査があるものの、GHOSTDAG/header pipelineが各blockで読むreaderは未接続である。単にfence有効ではここは完成しない。動的幅変更自体を初期目標から外し、固定設定で実負荷の受入率・遅延を測る選択もある。D11。

emission fenceは適格claim-bearing workのescrow carveを同じ受理DAAで最大16枠に抑える。これは全物理ブロック数の上限16でも、16モデル分の無条件保証でもない。riderなど条件分岐がある。D10。

## 試験不足

1. `rfc0018_dag_siblings.rs`の3テストは、同じ親の3モデル、元のclass/bond/carrying block、6到着順での状態一致をassertする。fixtureはclass/readinessの設定・人工的なroot・PoW skipを使う。実モデル推論、実Panel検証、Gate C、実支払いまでの試験ではない。D14。
2. `rfc0018_dag_drill.py:363–370`は、Panel RPCが席を返さなくても`[]`、claimIdがなくても要求したIDをdefaultにする。Panelのnonempty・bound・検証完了を必須assertせず、`panelsDistinctByClaim`も記録だけである。Panelの集合が互いに異なることより、各claimへ正しく拘束された実際のPanelと証拠が重要。D15。
3. 同梱の2026-10-15/16検証記録にはsibling試験3件の失敗が残る。ただしこれは過去の記録で、今回の再現結果ではない。D16。
4. 一Panel serviceのmain carrierはMAX_INFLIGHT_CARRIERS=1で、funding chainを直列化する。replayは別のbounded task、readinessの別funding laneもある。これは全ネットワーク一人制限ではないが、多モデルの同時担当時には実測すべき局所的な制約。D17。

## 完成と呼ぶための受入

対象genesisモデルを通常の登録・runtimeで用い、別minerから共通parentsへの同時生成を行う。REDを実際に発生させ、全適格claimについて元class/bond/carrying blockが保たれること、各claimへ実Panelがbindされることを確認する。

そのままFinal、必要なGate C認証、frontier/weight、本人への実支払いまで追う。arrival order、分断再接続、restart、IBD/reorgを変えても結果が一致することを確認する。

一つのモデルのPanel/DA応答だけを遅延させ、他モデルがどの段階まで進むかを測る。共有frontierの制約は仕様として明示し、不必要に新claim・他モデルの個別検証まで止めないことをassertする。タイムアウト・公開追及・客観裁定で遅延を有界にする。

不正claimと正常claimを同時投入し、不正分の実slash・関連権利失効と、無関係な正常分の処理・支払いを確認する。共有担保や同一execution rootなど正当に連動する責任は別扱いにする。

負荷を上げてmodelごとの採用率、待ち時間、bond lock、CPU/RAM、DA bytes、court、Gate Cのdraw消費を測る。同一mergeset内round robinだけで長時間の公平性を保証したと扱わない。

## 今回実行したこと・していないこと

実行: ソース抽出・静的追跡、Python独立検査。3class×2attemptの720並べ替えでcanonical order一致、nonattempt枠保持、Panel guardの空RPC仮定反例、resolved-prefix算術。

未実行: Rust compilation / Rust unit or integration tests / kaspad実node / 実モデル推論 / 実並列throughput benchmark。cargo/rustcが本環境にない。独立Python検査はRust本体やネットワークのPASSを意味しない。

`check_parallel_rules.py`と`rule_check_results.json`に検査範囲と出力を保存した。
