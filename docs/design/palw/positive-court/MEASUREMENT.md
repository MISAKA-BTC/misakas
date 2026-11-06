# Positive Court R2 — 三者別の実測・認定票

**測定計画であり結果ではない。** [JSON template](measurement-record.template.json)は未測定/未承認。有効化可能profileは0。

## 共通条件

model/program/tokenizer/input roots、dtype、量子化/丸め、context、prefill/decode、prefix、output長、各binary digest、compiler/OS、CPU/GPU/RAM/SSD/network、試験時のsource・資料配置を固定する。実際のminimum Miner / Panel / Nodeを別に指定する。

## Miner: 有用推論に対する追加負担

| 項目 | 必須記録 |
|---|---|
| 通常推論baseline | 推論だけのelapsed/CPU/GPU/VRAM/IO、同じmodelとjob |
| 結果commit | 署名・root固定・hashの追加time/bytes |
| artifact index | 事前準備、一回限り、償却、毎claimの内訳 |
| data応答 | unique/repeated bytes、追加hash/IO、要求数、全Panel合計round |
| 保持 | 保持object、最大RAM/disk、期間、削除・委任条件 |
| optional再計算 | opt-in checkpointのみ。off/onを別記録 |
| 内訳禁止 | 全trace/witness生成呼出しが標準Miner経路で0であるtrace/計器 |
| 体感影響 | 推論latency・throughput・energyの増分、p95/p99/max、重複要求攻撃時 |

modelをPanelが既に持つ場合、providerが供給する場合、Minerだけが保持するcold caseを別々に測る。成功caseのMiner負担が軽いことと、sole holderが全weightを送る最悪ケースを混同しない。

## Panel: 証拠を作る計算

model/data取得、全再実行、trace spill、tree/provenance構築、codec/opening束、署名・搬送・再送、失敗候補/再開/交代のCPU/GPU/RAM/disk/bytesを分ける。推論と同等以上になり得ることを隠さない。獲得feeと費用、完成しないケースも計測する。

## Node: 独立検査の総量

全coverage、exact/field/hash/codec、全operand bytes、cursor/state、台帳、同時case、保持、IBD/reorg、cold-cacheの全量を測る。EXACTとFULL_RELATION_DIRECTを同一意味/全関係で比較する。小片化でRAMが減っても、総量が減ったとは扱わない。matrix-vectorで高速化しない結果も保存する。

## 合格判断

三者別のper-request/per-claim/per-epoch/concurrency/retention caps、Miner追加負担の許容比率・絶対値、最大正常応答遅延、独立carrier、帰責機会記録を**承認済みの数値**として埋める。未測定を0で代入しない。平均だけでは認定しない。raw logsと再現command、独立レビューをdigestで結ぶ。

network全体・複数model・正常/負のCourtが同時に最大発生する試験、data拒否、Panel応答隠匿、Panel交代、request budget復元を含める。実行不可能なprofileを軽い抜き取りへ変えて「合格」にしない。

現状: Rust E2Eなし、実LLM計測なし、複数Node実行なし、mainnet認定なし。参照Python試験は役割/帰責の小例であり本票を充足しない。
