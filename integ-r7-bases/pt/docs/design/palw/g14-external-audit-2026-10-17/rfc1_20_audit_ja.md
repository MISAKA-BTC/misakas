# RFC1–20 / RFC14 G14 コード監査

## 結論

**有効化済みと仮定しても、現ZIPを「RFC1–20すべて完全実装、かつG14の実効安全性も完成」とは判定できない。**

ただし、G14が設計書だけで未実装という意味ではない。区間の不一致を追及するservice、認証済み証拠による客観裁定、実producer bondのslash、claimの失効、関連権利・creditの拒否に対応する実装がある。全panelが不正にPassを出していても外部bondが有罪にするテストソースもある。

残る重要問題は、(1)少数区間検査の検出率が攻撃の分布に強く依存すること、(2)共有court枠やTIR取得枠の競合が局所化を妨げること、(3)TIRの現行価格・時計が実用的な追及を保証できていないこと、(4)実モデル・実ノード・本番設定での全経路確認が終わっていないことである。

## 1. 前提・監査範囲

対象はユーザー提供の `misakas-pre.zip`。SHA-256は `c13be519c006ac707d2c3f2ae1dcb468b50695844121f1e4c6e0321b92e9f22e`。

DAA 16000で関連fenceが有効になるというユーザー指定を採用した。現testnetの無効presetや空のarming recordを欠陥の理由にしていない。既存コードが有効になった場合を読むものであり、ZIPに含まれない将来の価格改定・容量改定まで実装済みとは仮定しない。

最新の許容度は、不正claimの見逃しをおおむね1–10%許容する方針として扱う。これは「10件または100件ごとに必ず1件以下」という決定的上限ではなく、定義した攻撃モデル・抽選・検査範囲に対する確率である。不正claimの発生頻度とは区別する。

監査はRFC14の主要経路を中心とした静的コード読解、テストソースの確認、独立したPython再計算である。RFC1–20のすべてのMUST要件とすべてのモデル・経路について完全な要件追跡を完了した監査ではない。Rustのビルド・テスト・実ノード起動・32K実モデルの実行はしていない。本環境にはcargo/rustcがない。リポジトリに記録されたPASS/FAILを今回の実行結果とは扱わない。

以下のE番号は、同梱の `rfc14_source_evidence.md` の原本行番号付きソース抜粋を指す。

## 2. 目標の正確な定義

提案する受入条件は次のとおり。

> 対象classの登録済みモデルと検証能力を持つ正直な検証主体が少なくとも1つ存在し、規定claim rate・資源・担保・資料到達性・carrier inclusion条件を満たす。定義した不正集合に対する不正claimの検出率を90–99%以上にする。期限内に受理された追及は、他panelの賛成なしに、計算不正の客観裁定または資料不開示の客観defaultへ到達し、対象claimの経済的利益とconsensus creditを適用規則に従って失効させる。

「onlineである」だけではモデルの保有、全対象claimの検査、証拠取得、告発料、期限内収録を保証しない。RFC14もこれらを条件にしている。正しいpanelがネットワーク上に1つ存在することと、問題のclaimをそのpanelが実際に検証することも異なる。抽選されなかったclaimには、public verifierとしてのdiscovery・検査が必要となる。E01/E05。

検出対象は登録された計算仕様と不一致なclaimであり、「そのminer自身が物理的にGPUを動かしたか」ではない。外注や別主体からの正しい計算の取得まで、値の不一致だけから証明することはできない。同一仕事の二重計上拒否などは別のbinding・権利管理の問題である。

## 3. 区間で不正を発見した後、slashできるか

### 3.1 検出から告発への実装はある

`consensus/core/src/palw_fp_interval_v1.rs:25–70` は1seat・1claimあたり4つの異なる区間を抽出する。区間数が4以下なら全区間になる。`kaspad/src/palw_fp_seat.rs:391–428` は各openingをbackendに渡して再実行し、`Fault` / `FaultInRange` を返す。未回答をCertifiedとはしない。E02/E03。

ここでのFaultはまだ客観的convictionではない。`kaspad/src/palw_panel.rs:11738–11786,16393–16415,17644–17756` は発見済みの不正位置を保持し、不正を発見したclaimに後からValidを署名しない。必要なleaf証拠を追及し、裁定可能なら署名済み `ShardCourtAccused` をcourtキューへ送る。末端証拠が正しい計算に一致した場合には、ローカル再実行の不一致だけで誤って告発しない。E23。

TIR側でも `kaspad/src/palw_tir_public_owner.rs:356–385` はseatがFaultを発見したclaimを優先して追及する。ただしowner admissionと資源上限を満たす必要がある。E24。

### 3.2 客観証拠から実担保を減らす実装はある

kernel経路では `misaka/palw/kernel/src/ledger.rs:2600–2680` が適格bond・期限・費用・court予算を確認し、`adjudicate` の成功時に `convict` へ進む。panel多数決の承認をここで要求しない。

`convict` はclaimを有罪にし、予約担保に基づいてproducer collateralを実際に減らし、`SettlementKindV1::SlashFraud` を発行する。native foldの `consensus/core/src/palw_kernel_route_fold_v1.rs:1103–1127` がそれを `builder.slash_bond` に接続する。strict処理では徴収不足を架空の徴収として記録しない。E06/E07/E08。

`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/c4r3.rs:104–131` は、全interim seatがPassを出した不正claimを、外部bondが公開資料から告発し、実producer collateralが `claim_collateral` 分減ること、reporterの権利、replay一致をassertする。134–160にはPanel=0でFinal前後に有罪にするテストもある。**これは今回実行したテストではなく、テストソースで確認した内容**である。E09。

### 3.3 開示拒否は、計算不正そのものと区別する

claim固有の適格資料を開示しない場合にはDA/default処理へ進める。`consensus/core/tests/g14_close_legacy.rs:485–530` のテストは、他bondが従来共有DA予算を使っていても独立予約の告発が受理され、claimをpauseし、未回答で `ProducerWithholding` としてvoidされることを記述している。E15。

「計算が間違っていた」という証明と「開示義務に応じなかった」という証明は違う。後者から計算不正や全Valid署名者のFalseValid責任を自動的に導いてはいけない。

### 3.4 すべての不正署名panelを一律slashする実装ではない

上記kernel E2Eは、interim seatのcollateralが変わらないことも明示的にassertする。他方TIR公開convictionでは、正確にcreditedされたValid custodianの未消費lockを確認して責任追及し、producerの共通有罪処理も呼ぶ。対象や署名scopeによって責任が異なる。E09/E13。

ユーザーの中心目標である「不正producerを処罰する」ことと、「不正にPassしたすべてのpanelを必ず処罰する」ことは分けて評価する必要がある。

## 4. ブロック生成・報酬拒否の範囲

`consensus/core/src/palw_state_v2/transition_builder.rs:3560–3670` の共通処理には、live claimのvoid、Final後の責任追及、当該claimに結び付く権利の失効、reporter処理がある。TIRでexecution faultが証明された場合には共有execution root側の権利失効もある。E10/E13。

`consensus/core/src/palw_state_v2.rs:21475–21493` は、producer floor不足またはfrozen bondからの新attemptを拒否する。ただしコメントにもあるとおり、この拒否はown attemptをskipする非致命的処理であり、あらゆる物理ブロックを無条件にinvalidにするものではない。E11。

Gate Cでは、対象claimのSecurityCertifiedがなければ確定weightを与えず、liveのprovisional weightも0にする。Voided claimはcertifiedにならない。genesis/annual forkの1.0、登録モデルの0.3というtierは配分係数であり、不正claimを検証から免除する条件ではない。Gate C自体には抽選verifierによる正の承認など、追加の参加・quorum条件がある。E12。

kernelのFinal処理は `convicted` またはconsumer hold下のjobには報酬を出さず、EXECの報酬releaseもconvictedなら拒否する。一方で一般kernelの処理にはFinal時の支払いと、その後に続くliability期間がある。**Final後slashができることから、既に払出し済みのあらゆる報酬の取り戻しや過去の全ブロックの巻き戻しまで保証されるとはいえない。** E21/E22。

よって確認できる主張は「対象claim・その仕事に結び付く報酬や権利・適用されるbondの能力を失効させる」である。「一度不正した主体は別bondでも永遠にブロックを作れない」や「過去ブロックを全部撤回する」とは異なる。

## 5. 4区間検査で見逃し1–10%を達成する条件

### 5.1 計算

総区間数をN、不正を実際に検出できる区間数をD、正直な1検証主体が調べる重複なし区間数をkとする。不正箇所が抽選前に固定され、一様抽選なら、

```text
P_miss = C(N-D, k) / C(N, k)
P_detect = 1 - P_miss
```

ただしk>N-DならP_miss=0。これは非復元抽出の超幾何分布のゼロ検出確率である。参考一次資料: SciPy公式ドキュメント `scipy.stats.hypergeom`。

Dは「省略したFLOPs」「不正tokenの数」ではなく、その区間を登録済み仕様で検証したときに不正と判別できる区間の数である。後続を偽状態から整合的に計算した場合など、省略量からDを直ちに推定できない。

**以下はN=1000とした説明用の厳密計算であり、genesis実モデルの測定値ではない。**

| 検出可能な不正区間D | k=4の検出率 | 90%以上に必要な最小k | 99%以上に必要な最小k |
|---:|---:|---:|---:|
| 1000 | 100.0000% | 1 | 1 |
| 500 | 93.7875% | 4 | 7 |
| 100 | 34.4338% | 22 | 43 |
| 10 | 3.9462% | 205 | 368 |
| 1 | 0.4000% | 900 | 990 |

したがって、全域に不一致が広がる雑な偽造には4区間でも強い。少数区間だけに不一致を残す攻撃まで同じ精度で防ぐとは言えない。`palw_fp_interval_v1.rs:25–27` の「計算をskipすればすべての区間が誤る」というコメントを、任意の不正に対する保証として採用しない。

この表は**区間の抜き取り方式**の評価である。全必要関係を覆う直接代数検査や全再実行には別の検出確率・コストがある。また、すべてのgenesisモデルがこのlegacy k=4経路を使うとは限らない。TIR、kernel、constraint suiteごとに別途評価する。

### 5.2 同じ検査の再試行と新しい抽選は違う

legacy drawはnetwork、beacon、claim、seatと内部draw counterに対する純粋関数である。同じ入力で100回実行しても同じ4区間であり、400区間を検証したことにはならない。E02。

同梱Pythonはchecked-in golden vector `[645, 20, 372, 227]` と一致し、同一入力100回のdistinct sample setが1であることを確認した。これはRustの実行結果ではなく、純粋関数の移植による再現である。

### 5.3 実効処罰率

概念上、

```text
P(期限内処罰)
  = P(正しい主体が対象claimを検査する)
  × P(不正を検出する | 検査した)
  × P(局所化・受理・裁定・徴収が完了する | 検出した)
```

これは条件付き確率の分解であり、各要因の独立性を仮定していない。samplingだけ99%でも対象claimの半分しか検査できないなら、他が完璧でも実効処罰率は49.5%以下である。court拒否や期限切れはsamplingの見逃しに混ぜず別計測する。

## 6. 検出後の処理で残る重大問題

### 6.1 G-07: 共有court枠の飽和

`palw_state_v2/objects.rs:2002–2010` と `palw_state_v2/da_sessions.rs:752–760` はネットワーク共通のcourt session数が上限に達すると `CourtSessionsAtCapacity` を返す。上限は `turn_deadline_daa × 1 court move/block` から導く。現RCの42枠について残件テストがある。E27/E28/E29。

`g14_close_legacy.rs:533–564` は、連合が囮claimのcourtを占有し、外部検証者が不正claimのdissectionを開始できない状況を固定している。ソース自身がNOT closedと記す。ただしこのテストでは独立予約が対象claimのFinalを保留しており、**攻撃者がそのまま報酬を得たというテストではない**。E14。

影響するのは共有interactive courtを必要とする経路であり、既に完成した全direct proofが同じ理由で拒否されるという意味ではない。必要な修正は、実測済みper-block CPU/bytes予算、期限に間に合うスケジューリング、占有時間の負担、claim admissionの上限の組合せである。単に42を増やすと、既存のrung deadlineを正直な当事者が守れなくなる危険がある。per-bond上限だけでも複数bondの連合を排除できない。

### 6.2 TIRの8 owner枠

`palw_tir_public_fold_v1.rs:618–639` は8 owner枠を持ち、idle ownerは追い出してdepositをburnする。idle回収は既に実装されており、未実装と数えない。しかし全ownerがactiveなら新しいOpenを拒否する。E13。

新しいownerとして資料取得・局所化を始める必要がある検証者について、active占有下で期限内追及できる保証は確認できない。公開済み資料の共有・再利用で回避できる場合を含めて敵対的E2Eが必要である。完全な証拠を既に持つdirect filingまで一律に妨げられるとは断定しない。

### 6.3 TIR frameの費用と時計

最新のリポジトリ記録は、frame構築の旧不足を修正した一方、現行network_v1の価格・時計の問題を未修正としている。例示corpus・custodian上限6では、外部owner rent約2.2×10^15 sompi、Openまで少なくとも約0.9×10^6 DAA、collect_untilが約6×10^7 DAA以上という計算記録がある。これは**今回の実測でも、すべての32Kモデルに共通の値でもない**。E17/E26。

同文書§9.7では本番経路のT-N1 node harnessは未作成、最新残作業文書でも価格・容量・drillがWIPとされている。E16/E17。

有効化するだけで、外部検証者が現実的な担保・費用・時間で証拠を提出できる状態になるとは言えない。逆に、短いtimeoutや低い料金を根拠なく設定して負荷上限を失う修正も不適切である。実モデルのparser work、資料サイズ、replay回数、同時追及数から測定する必要がある。

## 7. 検出精度・完遂率を上げる実装案

### 優先P0: 発見した不正を処罰へ運べる容量

(1) G-07のshared court飽和を、本番に近いCPU・bytes予算で閉じる。期限付きスケジューラ、継続占有の負担、bounded claim admissionを同時に設計する。無制限資金・無制限流入・完全検閲まで1台で対処できるとは主張しない。

(2) TIRのactive owner占有に対して、既存公開資料の再利用、共有進捗、限られた総負荷の下での公平な開始機会を設計・実証する。idle回収を実装済みの出発点とし、単純な枠増加だけを解決としない。

(3) TIR pricing/clockを実モデルで測る。acceptedな追及の最大時間と、Final、報酬の払出可能化、bond exit/lock期限を整合させる。既存hold・失効処理を再利用し、すべての経路に一律未実装と決めつけない。

### 優先P1: class単位の検出率policy

`k=4`を単純に全クラス共通の大きな定数へ変えるより、class/model/kernel版に結び付くpolicyで、検査方式、N、k、必要な故障モデル/D_min、目標ε、検査bytes/CPU、期限、局所化方法を拘束する。

claimがNや作業量を過少申告して検査を軽くすることは避ける。現在も `palw_fp_seat_draw_at_the_claims_price_v1` に分母をclaimの価格に照合する防御がある。これを維持・拡張する。E03。

kを増やすと、1claimの取得bytes、`pool_interval_opening` の512 pair、同時claim数、DA/court予算、期限も変わる。`k`定数だけを変更すると、検査したいopeningがローカル上限で落ちるおそれがある。E03/E23。

### 優先P1: 新しい乱数・重複なしcoverage

commit後の信頼できるchallengeを使い、claim/model/job/round/seatの拘束を維持する。新しいroundを設ける場合は、同じ乱数の再利用を「独立検査」と数えない。既検査区間集合を保持・再導出し、restart/reorg後も重複分をcoverageに加算しない。

共有 `misaka/palw/challenge/src/seed.rs` にはrejection sampling、distinct_indices、staged roundの拘束がある。新しい独自beaconを増やすのでなく、既存契約を基礎に新しいpolicy版へ移す。legacy方式のmodulo reductionには微小な偏りがあるが、k不足に比べた主要リスクではない。E19。

receipt・policy版・sampling domainを変える場合は、旧claimの検査意味とreplay互換性を保持する。高さ16000前後で新旧を区別し、同じドメインのままgolden vectorを変更しない。

### 優先P1: Sparse faultへの対応を明示する

区間を単に抜き取る方式のままで単一不正区間を99%検出するには、Nの99%を調べる必要がある。これを低コストで満たせるとは約束しない。

選択肢は、必要なkを受け入れる、対象故障モデルを限定してその限界を開示する、既存の全再実行を高リスクclaim/影検証に使う、またはRFC07の全関係を覆う直接検査suiteを用いることである。

`seg_detect.rs` とnode serviceには独立全再実行が既に存在する。これを未実装として作り直す必要はない。ただし、担当モデルとclaim rateを現実に処理できるかを測る。E04/E05。

RFC07は認証済みoperandを読む直接batched Freivaldsを行列積に使い、非線形、量子化・丸め、routing、入出力、memory、境界も直接検査/再実行で覆う方針を持つ。MatMulだけに高い検出率があっても、残りの演算が未検査ならclaim全体の検出率にはならない。これはSNARK等の導入提案ではない。E30。

既存 `soundness.rs` はrelationごとの誤差、反復、retry、grinding、adaptive queriesを会計するが、入力された前提の正しさ自体を証明しない。また現コードには128-bitの承認下限があるため、今回の「監査の見逃し許容1–10%」を、そのまま全suiteの既存承認条件に置き換えてはならない。区間監査の検出率と既存suiteの条件付き誤受理会計は別の指標として扱い、変更するpolicyの対象範囲を明示する。E31。

### 優先P1: fault corpusと実効率の計測

少なくとも、全面偽造、1区間だけの不正、区間末尾/先頭、prefill/decode境界、KV・再帰state、量子化・丸め・overflow、認証した別job/別modelの混用、整合的なborrowed trace、資料非開示を分ける。直接有限体検査では整数誤差が法の倍数になるケースも対象にする。

指標は `eligible_claims → honestly_checked → mismatch_detected → evidence_localized → filed → accepted → convicted/defaulted → collateral_collected → reward/rights_invalidated` とし、capacity refusal、費用不足、資料不足、期限切れ、未担当を独立に記録する。

乱数を変えた多数の試験で検出率を測るだけでなく、抽選前のcommit、grinding、選択的abort、複数bond、共通operator相関を含む。無作為な雑な偽造だけで高い率が出ても、任意の攻撃に一般化しない。

## 8. DAA16000向けの受入テスト

A. 全panelが不正claimにPassする。producerと独立の1public bondが、公開read API、保有した正しいmodel、認証済み資料だけで不正を発見・局所化する。producerの私有実行インスタンスをfixtureとして渡さない。

B. 32K genesisの各familyで、単一点・区間境界・全面不正のそれぞれについて目標検出率を満たす。sampling方式で満たせない対象は限定を明記するか別suiteを使う。

C. court共有枠満杯と継続的な埋め直し、TIR owner全activeでも、宣言した資源・admission条件の下で適格追及が期限内に開始/完了する。既に完成したdirect proofと、対話局所化が必要なケースを分ける。

D. 資料提供なら客観算術裁定、不開示なら適格DA/default、偽告発なら棄却となる。検証者自身の実装差異をproducer fraudへ誤分類しない。

E. 有罪後に実担保が減り、reporterへの支払いは実徴収で裏付けられる。同じ罪で二重徴収しない。失効対象の報酬/round rights/weightを用いた新規生成を拒否する。FalseValidの責任は署名scopeごとに確認する。

F. Final直前、Final後liability内、liability終了後、bond exitと追及の競合、reorg、再起動、IBD、pruned importで同じ判定になる。既払報酬の扱いは別途仕様化し、未払取消と混同しない。

G. 不正のない32K claimが正常にFinal・報酬・ブロック機会に到達する。安全性のために全claimが停止するだけでは完了としない。fence直前・ちょうど16000・直後に受理されたclaimを区別する。

## 9. RFC1–20の確認表

以下は対応実装と確認できた残件の一覧であり、すべてのMUSTを全件テストした合格表ではない。RFC05の不採用VM、RFC17で廃止された旧モデル審査など、撤回された設計を未実装欠陥に数えない。

| RFC | 主な対応コード/範囲 | 今回の判定 |
|---|---|---|
| 01 | `consensus/core/src/palw_freeprompt_v3.rs`、生成制御・backend群 | 実装群あり。全サーフェスの個別完了は未認定。 |
| 02 | `misaka/palw/tir/{core,exec,lower}`、`misaka/palw/kernel` | 多数の実装あり。TIRの費用/時計、native経路、実規模の検証に残件。 |
| 03 | `misaka/palw/gen/src/output.rs`、generative class群 | 対応コードあり。全class/profileの完了は未認定。genesis text-only外を直ちに欠落扱いしない。 |
| 04 | `consensus/core/src/palw_improve_{v1,fold_v1,state_v1,artifact_v1}.rs` 等 | model improvement実装群あり。全改善・昇格・複合経路の完了は未認定。 |
| 05 | `docs/rfc/0005-palw-ml-vm.md` | VMは明示的不採用。VM未実装を欠陥としない。versioned kernel方針。E25。 |
| 06 | `palw_tir_shard_v1.rs`、`palw_tir_shard_fold_v1.rs`、watch | sharded検証の実装あり。全familyの独立局所化・実負荷完遂は未認定。 |
| 07 | `misaka/palw/challenge`、検査/receipt/selected-approval adapter | 実装あり。最新記録ではadapter追加後fixture移行・m/k測定が残る。全被覆と誤差前提の確認が必要。E16/E30/E31。 |
| 08 | `palw_exec_v2.rs`、fold、verify、rewards | EXEC実装あり。kernel/EXEC seamなど最新残件記録あり。全終端未認定。E16。 |
| 09 | `misaka/palw/remote/src/{attempt,register,relay,pursuit}.rs` | node-less登録・claim・告発コードとE2Eあり。全対応経路の実ノード完了は未認定。 |
| 10 | `misaka/palw/panel`、`palw_panel_v3_fold_v1.rs` | permissionless binding/foldあり。旧「referenceのみ」という記述だけでは未実装扱いしない。完了未認定。 |
| 11 | `palw_onboarding_v1.rs`、`palw_onboarding_fold_v1.rs`、conformance | onboarding実装あり。実モデルとsuite前提・資源測定の完了未認定。 |
| 12 | `palw_fork_choice*`、`palw_native_readiness_v1.rs`、`evm/` | native/PALW-only関連実装あり。最新記録に統合試験の失敗・未再実行あり。E16。 |
| 13 | `misaka/palw/sdk/src/tir_{merkle_index,rows}.rs`、runtime_pack、preflight | layout/工具実装あり。現実の全genesis artifact再現・運用は未完了記録。E16。 |
| 14 | 区間検査、seat pursuit、public owner、court、slash、replay | 中核実装あり。ただしG-07、active owner、価格/時計、実ノード経路の残件により全経路完成ではない。 |
| 15 | `misaka/palw/kernel/src/opv.rs`、`palw_panel_free_v1.rs` | Panel=0実装・テストあり。G14と外部検証者の資源・経済条件に依存。 |
| 16 | `palw_rfc0016_v1/`、`palw_rfc0016_fold_v1.rs` | bond/物理生成機会/Final容量の実装あり。係数・容量の実測と統合完了未認定。E16。 |
| 17 | `palw_model_tier_v1.rs` | 現行はtier方式。撤回されたモデル審査の未実装を問題視しない。claimの真偽とは別。E12。 |
| 18 | `palw_rfc0018_v1.rs`、`palw_security_certification*` | Gate C等の実装あり。m/k、参加・収束・統合試験に残件記録。単一正直panelだけで全合意を保証する仕様ではない。 |
| 19 | `palw_annual_catalog_v1.rs`、release/catalog/drill群 | annual catalog関連実装あり。判断項目、R-7/R-8・展開drillの未完了記録。E16。 |
| 20 | `misaka/palw/tir/lower/src/ram_account.rs`、float_ref/stream、convert/fidelity | 一部implemented/unverified。attentionタイル化/chunk observer、KV disk履歴は設計のみ。ただし全項目がgenesis text 32Kの必須障害とは限らない。E18。 |

最新の `remaining-work-2026-10-16.md:44–47` は一部greenと一部redを併記し、最新統合headで全suiteを再実行していないと記録している。この記録は今回の実行結果ではなく、また旧base由来の失敗もあるため、列挙された失敗をすべて新規security bugとは判定しない。

## 10. 今回実際に実行したもの

`python3 rfc14_sampling_check.py --output rfc14_sampling_results.json`

- 小さなNについて全組合せを列挙し、超幾何のゼロ検出式と505条件で一致。
- legacy drawをPythonへ移植し、checked-in golden vectorと一致。
- 同一入力100回でdistinct sample setは1。
- N=1000の説明用表の確率と90%/99%に必要な最小kを有理数演算で計算。

これらはRustのビルド、実際のnode/consensus fold、乱数源の操作耐性、モデルの検査率、32KのCPU/RAM/bandwidth、実担保の送金を実行した証拠ではない。

## 最終判断

ユーザーの見逃し許容を採用すると、目標は現実的な確率的監査として定義できる。しかし、**「見逃しを許容したから現行の4区間検証で十分」「fenceを有効にすれば全経路で処罰まで到達する」とはならない。**

先にG14追及の容量・費用・期限の残件を閉じ、その上でgenesis各familyの検査方式と攻撃モデルに合わせてkまたは検査suiteを決める。最後に、正直な1検証主体、他panel共謀、実モデル、実ノード、本番相当設定で、検出から実徴収と対象権利の失効までを測定する。この順序で初めて「1人の正直な検証主体を基点に、宣言した確率と条件で不正を排除できる」と評価できる。
