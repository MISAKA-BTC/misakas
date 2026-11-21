# ADR-0177 — モデル配分・owner収入とMOE2の分類別報酬

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)。
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0177-model-bond-allocation-without-availability-consensus.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. 維持する配分・責任の境界

モデル別の資本配分と個別Minerの採掘枠を区別する。固定された総PALW発行予算、model予算、個別bondのclaim/block/reward/Final-weight上限、共通期間、責任・退出・元原資を同時に守る。資本預託だけで支払わず、有効な実行・検証・Finalと適用会計を満たした仕事だけに支払う。

同じ拘束元本を複数model、role、claim、key、root、receiptへ重複計上しない。Position購入・市場reserve・owner seed・allowanceはminer bond元本ではなく、送金可能化から担保・合意weightを生成しない。

配分関数、α=1.5、平均期間、tier係数、κ、縮小分等は、[RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)と現在のparameter policyの版に従う。過去の30日例を、後続の平均期間改訂を無視して復活させない。今回の30%購入・20%永久LPでこれらの未解決会計を勝手に補完したり、追加mintしたりしない。

継承する資本集計は、同じ資金単位を重複なく数えた`S_m = C_miner_claim(m) + C_verifier_claim(m)`である。複数claimのreserved exposureの単純和やkey数を資本へ数えない。現行の採択済み平均期間は7日相当DAAであり、実時間7日を保証する時計ではない。平均資本を`S̄_m`、階層係数を`c_m`とすると、縮小規則適用前の重みは`A_m = c_m · S̄_m^1.5`である。coreの`c_m=1.0`、additionalの`c_m=0.3`をこの段階で一度だけ適用する。

この重みから求めるモデル別機会・予算は、その版の分母、登録tier上限κ、縮小・未使用分、丸め、立ち上げ時の例外に従う。これらに残る不整合をMOE2で再決定しない。同じ配分目標をモデル別のblock機会とcoinbase予算へ接続しても、個別claimのRへ再び重ねて掛けず、network全体の物理block頻度・DAA・難易度・fork-choice・Final weightを本改訂で変更しない。

## 2. モデル同一性・配布・検証

固定model IDとartifact rootは[ADR0175](0175-registered-models-are-permanently-immutable.md)に従い保持する。off-chainのモデル配布を、IP数・取得ACK・保持者の自己申告による合意票へ変換しない。撤回された専用Seeder報酬やモデル公開審査を、MisakaOptionを理由に復活させない。

これは未検証claimへの支払を許す規則ではない。新profileは[FOW1](../rfc/0062-palw-function-output-and-work-admissibility.md)の適格性・実行・費用条件と、[TB1](../rfc/0061-palw-internal-threshold-beacon.md)の正規乱数・元claim固定・選出Panel・資料・Courtを満たす。モデルの経済tierと数学的なFOW1適格性は別であり、genesisも検証免除しない。

## 3. MOE2の経済配分

### 3.1 モデル分類

GENESIS/FORK_DESIGNATED/ADDITIONALをnativeのmodel ID、artifact root、有効catalogとEconomicsSnapshotから一意に決める。自己申告、アドレス分割、token名、EVM facade、モデル複製を分類変更の根拠にしない。

### 3.2 Miner報酬

- ADDITIONAL: RからB=floor(300R/1000)でPositionを購入し、A=floor(200R/1000) BILIと購入Positionの必要分ellを同じpairへ永久LP化する。余剰q−ellだけをburnし、D=R−B−Aを受理時のreward planでMiner/Panel及び適法なほかのworker legへ配分する。
- GENESIS/FORK_DESIGNATED: MisakaOptionは無効。購入・永久LP・owner・BILI burn・R内Panel控除は0、Minerの権利はRの100%。検証役務費は別の事前資金契約へ確保する。
- R全額は、認証・maturity・hold・export・責任の既存条件を満たして支払う。ネットワーク全体の報酬を個々のMinerへ100%与える意味ではない。

### 3.3 Owner/登録者の収入

マイニング報酬をowner/登録者へ直接分配しない。追加モデルの通常protocol AMM売買に限り、grossの5%をBILI burn、3%をowner総leg、残差を買原資又は売却者受取とする。contributor分配を採用する場合もowner総leg3%の内訳であり、追加手数料ではない。

報酬購入Bと永久LPのAには追加BILI burn・owner直接legを課さず、両額を実reserveへ一度ずつ投入する。購入qの全量を先にretireせず、ellをcurve在庫へ戻し、余剰q−ellだけをburnする。永久LP receiptは拠出履歴でありownerの資産・引出権・手数料請求権・追加bondではない。直接送金・受取・譲渡・allowance操作にAMM税を課さない。登録者が別途Miner又はVerifierとして参加する場合は、他の参加者と同じ役割規則・認証・原資に従う。

### 3.4 評価すべき経済効果

owner収入は実際の通常AMM取引と自ら履行した役務等から生じる。model配分の増加、買上げ額、市場価格上昇がownerの確実な現金収入であるとは扱わない。公開による外部参加、資本集中、Sybil、閉鎖自己資本、借入、検証費、市場操作、相対取引によるAMM fee回避を評価する。

同じ認証資本なら、自己資本と外部資本をウォレット数等で区別して架空の高倍率を作らない。資本上限又は0.3係数だけで、安価な正しい実行や全ての不正が不利益になると証明したとはしない。FOW1の反復費用検査や実効的な反証機会は別に必要である。

## 4. 不変条件と移行

`sum(model_allocation) <= PALW_budget`、各bondの元本・責任上限、`R = B + A + D`、coreの`R = Miner`、通常取引の`gross = burn + owner_total + net`を同時に維持する。市場へ移すBとAの両方にRのexport cap・holdを適用し、coinbaseとの二重払いを禁止する。

MOE2では追加登録と一意市場の生成を同じnative遷移に含め、初期virtual/real reserve・在庫・owner参照を固定する。外部seedがなければ実reserveは0でよく、後続open又はseed/deepen待機を存在条件にしない。登録済みADDITIONALのpair欠落はstate invariant違反として拒否する。正常な市場準備待ち又は報酬分岐とせず、100%Minerへfallbackしない。既存市場の新規buy停止も、既受理の購入・永久LP・余剰burn義務を免除しない。LiquidityDustは原有限期限内だけ全工程未commitで保留し、解消しなければ既定の未成功終端とする。

永久LP記録済みmodelはcore指定不可であり、解除・後継移植・同root再登録で迂回しない。永久LP未記録モデルの分類変更では、genesis/fork指定の旧Position市場を新profileへ残さず、切替前に旧残高・allowance・claim・支払を明示winddownする。過去の配分実装、倍率提案、取得方針、Gate Cの改訂履歴は保存原本に残す。

MOE2は新しい配分関数や物理block頻度を採択するものではない。新経済policy、全consumer、別役務原資、移行・回復・資源上限の実装と検証がそろうまで有効化しない。
