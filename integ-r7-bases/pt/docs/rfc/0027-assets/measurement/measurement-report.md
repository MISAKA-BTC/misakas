# RFC-0027 追記測定 — 最低10万MSK、Panel追及費用とclaim継続性

**日付: 2026-10-20 / Status: Proposed・Review Required / 経済gate: 未達。**

この記録は、ユーザーの「実測して追加して記述」に対応する。[RFC-0027](../../0027-miner-account-bond-and-panel-incentives.md) / [ADR-0193](../../../adr/0193-miner-account-bond-for-panel-incentives.md) の補助証拠であり、合意コード・稼働料金・fenceを変更しない。

## 0. 結論と、今回「測れたこと」の境界

**最低100,000 MSKだけで十分だとは確認できなかった。前回の12,000 MSK/claimという説明値は、長期責任と実用的な継続発行を組み合わせる候補値としては重い。** 一方、低い費用単価を仮定した一部の局所作業は10万の中へ収まる。ただし、これはF02/F03の全合法分岐・全責任を積算した`L_c`ではなく、十分性の実証ではない。

ここで行ったのは **既存実測の再集計と、新たに実行したshadow再生・合成負荷実験** である。新Account Bondのnative node実装はこの入力にない。この環境にはRust toolchainもなく、元の大型モデルartifactも同梱されていないため、新規node・実モデル再検証・ネットワーク測定を実行したとは記載しない。起動環境と実コマンドの結果は[environment.json](results/environment.json)にある。履歴の実測時点と今回の解析時点を分離する。

| 証拠ラベル | 今回の処理 | 件数・範囲 | それで証明しないこと |
| --- | --- | --- | --- |
| **M_ARCHIVE** | 添付された過去の実測生ログを読み直して再集計 | node時系列17 run・24集計群・2,563 claim。入力JSON19個中2個は静的studyでrun数に含めない。外部検証21 sample / 10群 | 新Account Bond下のnode性能、現時点の稼働ネットワーク、全モデル |
| **R_TRACE** | 実測済み受付列へAccount Bondを加えるshadow実験 | 1,320条件 | 却下後のPanel抽選や合意状態まで再実行したchain replay |
| **S_MODEL** | 資本・保持・要求到来の離散イベント実験 | 326条件、各120,000 DAAの合成時間、先頭40,000 DAAを定常集計から除外 | 実際の経過時間、block生成、採用REAL、heartbeat |
| **D_CONDITIONAL** | 実測CPU/bytesと明示仮定から費用・資本を導出 | 資本150行、費用120行 | 実際のMSK価格、電気代、人件費、承認済み料金表 |

再集計・保存則は **1,365/1,365 PASS**、解析ハーネスの単体試験は **32/32 PASS**（そのうち1試験にseed固定の1,000 trace検算を含む）。これはAB01–AB16のnode-level合格数ではない。[summary.json](results/summary.json) / [tests.log](tests.log) / [reaggregation-checks.json](results/reaggregation-checks.json)。

## 1. 入力と再現性

node時系列は、添付の[2026-09-26 claim-capacity測定](../../../evidence/palw/measurements/palw-claim-capacity-map-2026-09-26.md)と、その`runs/`にある実測記録を用いた。元のharnessはnative template、ML-DSA署名、Panel導出、duty receipt等を実行したものとして記録されている。今回そのnodeを再起動したものではない。`instant2.json`と`study.json`は静的検査資料として区別し、17 runに足さない。

検証時間は[OPV測定記録](../../../evidence/palw/measurements/opv-measurements.md)に対応する、2026-10-09/10の`qwen25-p3.jsonl`、`qwen25-sweep.jsonl`、`gap07-verify.jsonl`を用いた。対象はQwen2.5-0.5Bの小さいclaim、機材は混雑したApple M1 Max / 32 GiB、local fileまたはlocalhost providerである。これは外部検証者のreference経路の実測であり、全drawn Panel・全対話を実測したものではない。

各入力の相対パス、サイズ、SHA-256は[source-manifest.json](results/source-manifest.json)にある。元のraw logは変更しない。再集計結果には元ファイルと行番号を記録する。21 sampleを独立な21台の測定とは扱わない。

同じbalanceラベルへ2口のbondがまとめられた`G_8k_split_13k_13k`と`H_8k_100k_100k`は、全体集計には残し、アカウントを一意に分けられないためshadowからだけ除外した。都合のよいmodel選別ではなく、識別情報の欠損である。初期実装のチェックでは「各phaseのCPU時間の和 = process全体CPU時間」と仮定して21件失敗したが、元harnessは全体をprocess rusageで採るためsetup/teardown差分がある。ソース照合後、差分を保存し、非負phaseが全体CPU以内という適切な検査へ修正した。[初回失敗log](run-initial-failed.log)。時刻データや元ログは合わせて書き換えていない。

## 2. Panel相当の追及費用 — 生ログの再集計結果（M_ARCHIVE）

時間はprocess全体のCPU秒とwall秒である。元記録にある`check`単独の時間と区別する。料金自己申告や将来の本番tariffではない。

| Qwen2.5-0.5Bのcase | n | CPU秒 p50 | CPU秒 標本p95 | wall秒 p50 | claim公開material | 結果 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 3 positions / honest | 5 | 141.85 | 155.39 | 191.93 | 178.65 MB | Clean |
| 3 positions / late | 2 | 165.37 | 166.34 | 293.48 | 178.65 MB | ローカル裁定で違反確定 |
| 8 positions / honest | 2 | 250.71 | 252.34 | 486.79 | 477.10 MB | Clean |
| 16 positions / honest | 2 | 373.48 | 376.17 | 712.00 | 956.44 MB | Clean |
| 32 positions / late | 1 | 628.31 | 628.31 | 756.14 | 1,921.82 MB | ローカル裁定で違反確定 |
| 3 positions / gather | 2 | 15.06 | 15.12 | 21.37 | 178.65 MB | wire上限で拒否 |

`p95`はnearest-rankで再計算した**標本記述値**であり、サービスの95%保証ではない。n=1や2では最大値に等しい。小標本・共有ホスト負荷・モデル範囲の制限を付したまま報告する。`decode`/`early`/`elem`も[全群のJSON](results/archive-panel-summary.json)と[全sample](results/archive-panel-samples.json)に残す。

21件中、ローカルの`court_node`で違反確定が記録されたものは8件、wire上限で拒否されたものは4件であった。残り9件はhonest検査である。この内訳は試験caseの構成であって、不正率・検出率・成功確率ではない。

**証拠が大きすぎて進めないケースを成功扱いしない。** `gather`のfilingは272,271,336 bytes、当該ログのwire上限は67,108,864 bytes。files 2件、localhost 2件の計4件が拒否されている。このログに新しい分割・対話経路を実行した証拠はなく、「100,000の担保があるから裁定可能」と読み替えない。元harnessのslash/reporter金額も試験用設定であり、新料金候補の根拠には使わない。

今回実モデル全量検証の根拠があるのは上記クラス・範囲に限る。他クラス、大context、F02/F03の全合法choice、指定custodian、署名付き全手番、post-Final追及、通信妨害下の配送、復旧・長期保持の費用は未計測である。これらを0円にして完全な`L_c`にしてはならない。

## 3. 既存nodeのclaim列へ新担保を重ねた結果（R_TRACE）

旧測定では、floorの早期escrow解放の中央値は21 DAA、8kの一部は23 DAAであった。観測できたFinalまでの時間は典型例で142/144 DAAだが、未観測・右打切りもある。これは2026-09-26の旧規則の結果であり、**新Account Bondの解放時刻ではない**。新しい責任を旧licenceで解放しない。

次は旧`P`を維持し、別に`A=100,000`を足し、`L=12,000`、観測期間中に新責任が解放されない条件で、元の受理列を再生した結果である。各行の受付列は201 DAA区間。新たな申立てやblockを生成せず、元の受付機会だけを残す・止める処理を行った。

| 元のrun / 既存P | 元ログの受理claim数 | shadowで残せた機会 | Account不足で止めた機会 |
| --- | ---: | ---: | ---: |
| floor / 13,000 | 20 | 8 | 12 |
| floor / 100,000 | 150 | 8 | 142 |
| 8k / 13,000 | 10 | 8 | 2 |
| 8k / 100,000 | 27 | 8 | 19 |

完全な表は[trace-shadow.csv](results/trace-shadow.csv)。3120/10000 DAAの仮定がどちらも観測区間より長いため、この短いtraceでの結果は同じである。そこから実際の責任の長さが同じだと推定しない。shadowで止めたclaimが元のPanelを消費しなくなった反作用や、別Minerの補充生産は再計算していないため、本表をネットワーク全体のREAL減少率には使わない。

## 4. 10万が支えられる継続頻度（D_CONDITIONAL / S_MODEL）

### 4.1 保持期間を無視しない

均等に`g` DAAごとに1件を出し、**全責任が適法に終了するイベントを仮に`H` DAA後へ置く**と、必要な同時account責任数の最小値は次になる。同一DAAでreleaseを先に処理する試験上の楽観境界を用いる。

```text
n = ceil(H / g)
A_required = max(100,000, n × L_c)
追加額 = max(0, A_required − A)
10万だけで支えられる L_c の上限 = floor_sompi(100,000 / n)
```

`H=3,120 / 10,000 / 20,000`は感度試験の入力であり、実測済み平均保持ではない。**3,120はnoticeの窓、10,000はhard deadlineであって、自動返還イベントではない。** 本試験でHを置いたのは、責任終了を仮定した資本の楽観境界を見るためであり、合意の返還規則を新設するためではない。後続責任が長ければ所要額は増え、未解決なら解放しない。

前回の説明値`L_c=12,000`では、10万に入るのは`floor(100,000/12,000)=8`件である。新accountだけの連続発行上限は、H=3,120で約1.846件/720 DAA、H=10,000で0.576件/720 DAA、H=20,000で0.288件/720 DAAとなる。離散到来、既存予約、他のgateによって実際の受付はさらに少なくなり得る。

720 DAAを「1日相当」と書く箇所は、120秒slotの**名目換算**である。今回720 DAAが24時間で進んだと測定したものではない。120,000 DAAの合成実験も166.7日間の実運転ではない。

### 4.2 同時責任と、必要額の具体値

`H=10,000`、均等発行、他の責任なし、同DAA release先行という条件。発行目標は感度分析用であり、ユーザーが採択した「実用性の基準」ではない。

| 目標claim数 / 720 DAA | 必要な同時account責任数 | 10万に収める1件責任上限MSK | 1件12,000の場合の必要Account MSK | 最低10万を超える追加MSK |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 14 | 7,142.85714285 | 168,000 | 68,000 |
| 5 | 70 | 1,428.57142857 | 840,000 | 740,000 |
| 30 | 417 | 239.80815347 | 5,004,000 | 4,904,000 |
| 90 | 1250 | 80.00000000 | 15,000,000 | 14,900,000 |

従って、例として30件/720 DAAを検討する場合、10万の範囲で全責任を賄うには`L_c ≤ 239.80815347 MSK`が必要である。これは**この額に料金を下げろという提案ではない**。全branchの必要費用・固定bounty・burnがそれを超えるなら、追加担保、目標頻度または別の明示承認された設計条件をレビューする必要がある。未導出の費用を消して帳尻を合わせない。

同じ30件で1件12,000ならAccountだけで5,004,000 MSKとなり、最低10万より4,904,000 MSK多い。別に、既存予約をADR-0176のA16 8k合成価格照会3,200.87121681、利用率50%、旧側の拘束を楽観的に720 DAAと置いた比較例では、旧Pは192,052.27300860、合計5,196,052.27300860 MSK以上となる。feeや他責任は別である。

**この旧予約例はA16 8k、§2のCPU費用はQwen2.5-0.5Bであり、同一profileではない。二つをつないで「同一profileの経済gateに合格」とはしていない。** 同profile・同ruleset・同jobでの追加測定が必要である。旧予約値を全claimの共通料金としても採用しない。

### 4.3 総資本固定と新資金追加を分ける

合成実験では目標30件/720 DAA、H_account=10,000、H_legacy=720、旧利用率50%、旧予約3,200.87121681、他のQ/B/R/F・計算・Panel・block制約なしを固定した。旧資本20万を基準に(A)旧側へ20万、(B)総額20万の内10万をAccountへ、(C)10万を追加し旧P20万を維持、を比較した。

| 仮の1件account責任 | A: 旧P20万・Accountなし | B: 総20万をA10万/P10万へ分離 | C: 10万を追加してA10万/P20万 |
| ---: | ---: | ---: | ---: |
| 200 MSK | 29.997 | 14.985 | 29.997 |
| 1,000 MSK | 29.997 | 7.200 | 7.200 |
| 12,000 MSK | 29.997 | 0.576 | 0.576 |

数値はwarmup後80,000 DAA区間での**合成admission数 / 720 DAA**。29.997と30の差は集計区間端点による。全326条件と理由別停止数は[synthetic-admission.csv](results/synthetic-admission.csv)と[特殊条件JSON](results/synthetic-special-cases.json)。

1件200なら新資金を追加するCでは当該資本制約による減少を起こさないが、総資本固定のBでは旧Pが半減して失速した。1件12,000ではPを維持しても新accountが先に詰まる。総資本10万しかなく、それを全て新Accountへ置く場合、旧Pがゼロなので旧側の参加条件も満たせない。**最低Account=10万と、最低必要wallet資本=10万を混同しない。**

### 4.4 未解決と補充の試験

未解決でreleaseなしの合成条件では、5,000回の提供機会中、最初の8件だけを受理し、その後4,992回がAccount不足となった。hard相当10,000を通過しても担保を返さない。有限担保で未解決責任を際限なく受けられるとは約束しない。

別の試験では、時刻1000 DAAに外部資金4,904,000 MSKの明示的補充イベントを入れ、Accountを5,004,000へ増やした。次の提供時刻1008 DAAで受理が再開した。無断wallet引落しではなく、合成資金注入である。補充後に実nodeがREAL blockを採用したという試験ではない。

## 5. Panelに利益を残すといくら必要か（D_CONDITIONAL）

### 5.1 token換算は実測ではない

今回、MSKの市場価格・Panelの請求書・電気代・回線代・人件費は取得していない。よってCPU秒を実際のMSK原価へ一意に変換できない。[plan.json](plan.json)に次の**仮定単価**を固定し、結果を数値の承認に転用しない。

| シナリオ | MSK / CPU時間 | MSK / GiB転送 | MSK / GiB・名目日保持 | 1 jobの運用対応MSK |
| --- | ---: | ---: | ---: | ---: |
| 機械処理のみ | 100 | 0 | 0 | 0 |
| 運用費仮定あり | 100 | 10 | 1 | 100 |
| 運用費10倍ストレス | 1,000 | 100 | 10 | 1,000 |

転送・保持の対象はclaim公開materialとfilingであり、モデルは取得済みと仮定する。元のCPU計測に入っているlocal artifact読取はそのまま含める。モデル取得費、実トランザクションfee、全対話、第三者relay/custodianの重複費、Panel自身の担保費、未計上復旧などは別途必要である。保持は10,000 DAA、作業原価の立替は年率10%・支払待ち720 DAAを仮定した。これも料金の実地見積りではない。

### 5.2 作業料と成功報酬を別に確保した参考表

費用感度を出すためだけに、作業料=`1.25×原価`、固定bounty=`1×原価`、burn=`0.1×(作業料+bounty)`と置いた。これらの率は未採択で、前版の固定額・配分を復活させる決定でもない。将来のtariffは事前に固定し、実行者の自己申告原価でその都度計算しない。

Qwen2.5-0.5B / 32 positions / lateの1 sample（628.313403 CPU秒）を参考にした**1つの局所作業**の見積り:

| 仮定シナリオ | 推計原価MSK | 作業料MSK | 固定bounty MSK | burn MSK | account側の部分責任MSK |
| --- | ---: | ---: | ---: | ---: | ---: |
| 機械処理のみ | 17.46 | 21.82 | 17.46 | 3.93 | 43.21 |
| 運用費仮定あり | 160.26 | 200.32 | 160.26 | 36.06 | 396.63 |
| 運用費10倍ストレス | 1,602.55 | 2,003.19 | 1,602.55 | 360.57 | 3,966.31 |

「運用費仮定あり」では原価160.26に対し作業料200.32で、適格に完了した局所作業に約40.06 MSKの利益を確保する設定となる。違反確定時は別に160.26のbounty候補がある。**利益を残す式にしたから仮定内で利益が出たのであり、実際のPanel採算を測って合格したのではない。** 人の対応費・支払リスクが違えば結果も変わる。

この1-jobの部分責任396.63だけでも、H=10,000・30件/720 DAA・Account10万の上限239.81を超える。条件が同じならAccountの増額が必要である。一方、5件/720 DAAの上限1,428.57には部分的に収まるが、合法な別jobが8個あると約3,173.05になり、そこにも収まらない。全job数を1と測定した証拠はない。1/8/32/128 jobの感度を[panel-cost-sensitivity.csv](results/panel-cost-sensitivity.csv)へ残した。異なる合法branchを、同じbodyの再利用と誤って統合しない。

wire拒否の`gather`は成功bounty適格にしない。正常claimでは不正成功報酬は発生しない。FeeEscrowの作業料の後に、Miner accountから費用負担者へ償還する場合も、Panelへ同じ作業料をもう一回払わない。自己告発の同一経済主体内のfee・償還・bountyを相殺するとburnと外部費用が残る会計を単体検査したが、既存leg・外部利得まで含む不正抑止の実証ではない。

**没収されていない10万は通常Panelの売上ではない。** 不正ゼロ、FeeEscrowもServicePoolも資金なしなら、Accountが10万でも1億でも、新Account由来の確実な通常収入は0である。通常報酬の資金化、申立費用を払う主体、その資金需要と供給は今回未測定である。

## 6. heartbeatについて言えること・言えないこと

今回のR_TRACE/S_MODELで確認したのは、**新担保が原因で受理機会を止める条件と、その数**である。そこからheartbeatの増加率を計算しない。実際のheartbeatは他MinerのREAL、抽選・需要・Panel状態、採用chainとDAAの動きに依存し、この入力の集計データには新規則での区分がない。

結果JSONでは`accepted_REAL_blocks`と`heartbeat_dependence_daa`を`null`、gateはUNKNOWN/未達としている。nodeの受理・採用REAL・heartbeatだけの期間をそれぞれ収集し、同一条件のA/B/Cで比較するまでG-REALをPASSにしない。元のpost-fill hold理由を実際の拒否attempt数と読み替えることも避けた。

## 7. この結果からRFC/ADRへ反映する判断

**最低10万は「参加の最低値」であって、「実用頻度・十分なPanel報酬の保証値」ではない。** 100,000というユーザー指定要件と、責任超過分を追加する構造は保持する。料金を検証せず12,000/claimを実用候補へ昇格させない。

料金・責任額・必要資本は、同一profileの全合法jobを基に `L_c → 実際の保持H → 重なる責任数 → A_required → 旧Pとfeeを足した総額` の順で導出する。先に10万に収まるbountyを決めて残りの義務を消す手順は採らない。短いslot回転や早期Valid、別profileを借りて実用性を達成したことにしない。

| Gate | 今回の更新状態 | 残る条件 |
| --- | --- | --- |
| G-Collectible | 整数・合成会計の部分検証。node未達 | 全claim・全合法branch・全horizon・全支払legの原資 |
| G-Panel | 履歴の外部検証CPU/bytes再集計済み。金銭的十分性は未達 | 同profileの全対話、実費単価、資金化・実支払、Panel純利益 |
| G-Entry | 長期責任と旧P切出しの反例を数値化 | 実用目標の承認、同profileでの総必要資本と持続率 |
| G-REAL | **未実測 / UNKNOWN** | node A/B/C、採用REAL、bond起因停止、heartbeat依存、補充後復帰 |
| G-Concentration | 未実測 / UNKNOWN | 実参加者の資本分布・退出・役割兼任費 |
| G-Compatibility / G-Court | 未達を維持 | 新実装、旧claim、F02/F03供給・inclusion、reorg/IBD |

**有効化保留を維持する。** 一方、「何も数値がない」状態からは進み、旧実測での費用規模、12,000の長期拘束による制約、30件なら239.81以下という条件付き責任上限、総資本固定/追加の違いを再現可能な資料として追加した。

## 8. 再実行と実地追試

元ZIPに前版の文書差分と今回の追記を適用したrepository rootで、Python標準ライブラリだけで再実行できる。`--out`は別ディレクトリを指定し、証拠の元ログを上書きしない。

```sh
python3 docs/rfc/0027-assets/measurement/measure.py --repo . --out /tmp/rfc0027-measure
python3 docs/rfc/0027-assets/measurement/test_measure.py
python3 scripts/drills/adr0176_stage1_measure.py --self-test
python3 docs/rfc/0027-assets/reproduce_examples.py --json-out /tmp/rfc0027-examples.json
python3 docs/maintenance/check_docs.py --json-out /tmp/rfc0027-docs.json
```

解析の実行時間は測定ツールの速度であり、Panel作業費には使わない。環境時刻と解析所要秒以外の決定的出力について二回の一致を別途確認する。

実地追試では、同profileのartifact、ruleset/fence、閉じたjob集合、受付/採用blockの識別子、Account/旧P/fee各残高、予約と合法release理由、Panel実作業・実支払、admission拒否理由を記録する。旧`adr0176_stage1_measure.py`は取得口の基礎になるが、新account列と責任状態の実装が必要であり、既存self-testだけで測定済みにしない。独立node、全owner共謀、全branch、供給欠損、境界、post-Final、restart/reorg/IBD、heartbeatからREAL復帰の試験をこの資料で代替しない。
