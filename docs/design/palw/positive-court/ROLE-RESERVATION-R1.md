# Positive Court — 役割別の残予約を保護する接続条件

2026-11-01。採択済み [RFC-0044](../../../rfc/0044-positive-court-full-coverage.md) の責務分離と全分岐予約を実装へ接続するための条件。新しい価格、wire、状態schema、役割、登録審査を割り当てる文書ではない。現時点のpure replayとtoy mapperは、以下のnative予約保護を実装・検証した実績にはならない。

## 分けて扱う量

| 量 | 用途と境界 |
| --- | --- |
| Miner/providerのlocal meter | 実際のread、認証、送信、保持、合意済みの任意処理。Panelの再計算やNodeの算術検査をMinerの仕事として加算しない |
| Caseのcanonical usage | 受理済みeventによる共有状態と累積量。保存した値だけで資料履行・計算合格・actorの違反を認定しない |
| 役割別の未完了予約 | 受付済みsource duty、Court試行、検査、保持、復旧の残需要。使用済み量と区別し、同じ上限の下で同時に保護する |
| physical lease/workspace | 実RAM・stack・I/O・保持・処理枠。logical quote、Vec capacityやcanonical retained bytesと同一視しない |

`Service.budget.fee`は資料サービス料金だけである。Court/Panel料金は別勘定とし、現在のsource replayではfirst source receiptだけがCase feeを増やす。

`additional_work`、`retained_bytes`、`transmitted_bytes`のsourceとwhole-Caseの射影は、field名や共通structだけから導出しない。現r1はCase/Courtとsourceを同じ集計へ投影する経路を持つが、それはMinerの担当作業を拡張する根拠ではない。native profileは各counterの源泉・単位・source上限・Case上限・残予約を明示し、rooted policy/quoteとして解決する。未割当の射影や換算を推測で埋めず、実native portを供給しない。

## admissionと遷移の条件

同じcurrent view、Case epoch、policy/schedule、既存の受付履歴から、役割別のcold最悪値と全未完了dutyを導出する。peer DTO、保存したPassed、payerの署名だけでは予約を作らない。payerと実際の計算担当は区別する。

各保護dimensionについて、次をchecked arithmeticで維持する。

```text
消費済み量 + 今回認める消費 + 遷移後のsource残予約 + 他役割の残予約 <= 対応する上限
```

Courtのeventを認める前にも、この条件を検査する。例えば `U + C <= B < U + C + R`（既受付応答に必要な `R > 0`）では、単にCourt消費 `C` が現在の余白へ収まることは十分ではない。Courtが残りのsource応答機会を失わせる遷移を受理しない。

累積work/送信量と、retention・physical leaseの区間占有は分けて評価する。上の総量条件だけでなく、全future service intervalについて利用できるcapacityと包含/応答機会を保護し、各actorのnotice/response/check等のexclusive cutoffより前に履行可能な順序であることを確認する。総量が収まっても、Courtが締切前のslotを使い尽くす遷移は受理しない。予約の寿命とdeadlineは元の契約・native規則から導出し、新たな期限数値を推測しない。

- 新source要求を受け付けるときは、全Panel・全providerを合計した正規単位で残需要と既存dedupeを確認し、応答に必要なwire/work/retention/fee等を先に保護する。供給不能なら要求・claim受付段階で拒否する。
- 応答が受理されたときは、対応する予約の消費・解放、source/Case counter、first receipt/due、履行status、historyを同じ遷移で更新する。counterへ同じsource deltaを二度足さない。delivery/readが完了しても、合意済みの保持・告発window・責任horizon・reorg/recoveryの残需要は、そのhorizon終了または正規取消まで保護する。Completeだけを理由に全retentionを解放せず、providerが署名した義務を勝手に延長もしない。
- 完全NoOpは新たなCase消費・予約・historyを作らない。Minerの実local spent I/Oは別寿命であり、chain undoやPanel交代で復活しない。
- Panelの失敗・交代・Positive完成だけで、既受付source dutyの予約を解放しない。応答完了、合意済みの義務終了、正規取消など、当該dutyの規則に従う。
- canonical保管量へworkspaceやcallerの一時Vec capacityを混ぜない。全同時保持・再encode・署名・取得・復旧の費用は、別途実lease/work quoteへ含める。

現在のA2にはfirst feeの予約がある。これをwire/work/retentionの残予約まで実装済みと扱わない。A6の53件はglobal差分とsource履歴・失敗隔離の検証であり、全役割のnative予約保護の検証ではない。

## 帰責と再生

NodeまたはCourt側が必要な枠を保護しなかった結果、正当な応答がBudget拒否された場合、その事実をMiner/providerの不提出へ読み替えない。期限内に適格な応答をNodeへ届けた履歴と既に記録されたCompleteを、Panelの失敗で取り消さない。未検証のclaimへ成功報酬・DAG/決済信用を与えず、実際に提供済みのsource料金と安全な返金は既存の別条件で扱う。

restart/reorg/IBDでは、使用済み量だけでなく、未完了duty・役割別残予約・履行・支払・責任を同じ履歴から復元する。保存されたcounterや予約flagをnative権限としない。actual source material、履歴完全性、現在のscope/epoch、資金・leaseを確認し、既存typed row delta、UTXO、acceptanceと同じcaller batchへ確定する。途中失敗で一部だけ公開しない。

各raw eventは、そのevent直前のhistorical accepted-prefixのguard下で、役割・provider委任・duty・cutoff・実event時計・予約・policy/料金authorityを再認証する。admission時のverifierや現在tipの役割を全履歴へ使い回さない。その履歴検査と、最終current head/root/epoch/generation・actual時計・資金の照合は別段階である。復旧は [native journalのreadonly再結合](NATIVE-JOURNAL-R1.md) に従い、live admission/prepare/funding commitを再呼出しせず、真正なreadonly receiptを再結合する。

source・historical・current native guardとpruning/backing保護、実leaseは、それぞれ必要な認証・quote・prepare・stageからDB write結果まで保持する。guardを失った場合はwriter排他内で全期待値を再resolveし、selected parent/root/prefix/generation/Case epoch/old head/役割/policy/未完予約/actual DAAをCASする。変化があればprepared capabilityを失効させる。同じroot/headへ戻るABAや、再確認直後にguardを解放してからwriteする経路を認めない。physical leaseは再起動時に再取得し、出力と抽出partsの必要寿命まで保持する。IBD・pruning・資料欠損の扱いはRFC-0044 §17の履歴・未検証・pending義務・料金・reorg/archive保持条件に従う。

## 実装完了と区別する範囲

現在の`NativeCommonUsagePolicyV2::map_usage_delta`はrequiredなlocal mapperで、実native実装は未供給である。source replay helperへの成功mock接続は、この文書の保護を満たすnative ownerではない。actual admission/transitionが上記の射影・予約・履行機会を同viewで検査するまで、native接続・有効化の資格を与えない。

受入試験には、正常なsource残需要を残した状態でのCourt消費、重複・再送・provider/Panel交代、応答受理と予約消費の原子性、検査/保管/再起動の失敗、全未完了dutyの復元を含める。Nodeのlocal capacity不足やPanelの証拠生成放棄をMinerの不正にしないことを、実native経路で確認する。
