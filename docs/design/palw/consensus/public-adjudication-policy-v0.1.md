# 公開裁定の方針 v0.1

> **後続採択 2026-11-08:** 新しいclaim版のNode非保有・モデル秘匿／寡占・非対話positive・検証先行Finalは[公開裁定の方針v0.2](public-adjudication-policy-v0.2.md)と[RFC-v2-0016〜0020](../../../rfc-v2/README.md)に従う。本v0.1の原本文と既存Batch v5の時計・履歴は保持する。

- 状態: 2026-11-07 のユーザー採択。この文書が公開裁定の上位設計である。配置は有効化ではない。出荷 preset は武装しない。P0–P3 には試験がある。P4 の debug 印刷は class の受理ではない。P5 は未了。main へは push していない。T12 の upgrade はない。
- 関係: [設計原則 §8](../../../PRINCIPLES.md#8-モデル計算の暗号学的証明を使用しない2026-10-15ユーザー恒久決定)、[設計原則 §9](../../../PRINCIPLES.md#9-モデルの二階層モデル単位の検証は不採用2026-10-15ユーザー決定)、[RFC-0047](../../../rfc/0047-batch-court-time-contract.md)
- 対象: 新しい Batch 時計版 `PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5` だけ。既に署名された claim は古い規則のままである。

## 何をするか

MISAKA はモデル計算についての claim を固定し、公開の異議を出し、認証された資料と決定論的な検査から、結果、責任、資金を決める。合意ノードはモデル計算のすべてを端から端まで再実行しない。ブロック、署名、通常のトランザクション、決済の検査は残す。

## 一つの鎖

鎖は一つである。分割するのはモジュールである。TIA、Celestia、外部 DA、ブリッジ、第二の鎖は置かない。

## 保証の範囲

保証するのは、登録されたモデル、プログラム、入力、実行規則についての実行の完全性である。世界の真実も、人間の選好も、保証しない。

## 異議がないこと

異議がないことは、正しさの証明ではない。`PredicateHolds` は claim 全体の証明ではない。公開資料、容量、独立した検証者の前提が欠けるときは、安全な hold または abort であり、通常の Final ではない。hold は 10,000 DAA 以内の成功ではない。hold の終わり、abort、費用の決済は、採択の前に指定する。

## モジュール

- Studio はチェーンの外で署名する。
- G14 の受理は、類、期限、bond、保持、容量を検査し、公開の異議が収まる claim だけを入れる。対象には、保留、最も遅い応答、公開提出、回復を含む。異議の証拠をそれ自体として受理できない claim は、その claim 自体を拒む。
- Panel と worker は、取得、再実行、局所化、証拠の作成をする。Panel の署名は署名者を識別する。Panel の多数も定足数も、評決でも finality でもない。止まった Panel や結託した Panel のあとにも、bond 付きの第三者経路が残る。
- 共有資料は重複を除き、独占しない。
- Batch と court は枠を予約する。
- native court は、有界な局所検証、責任、無効化、bond、報酬、Final を行う。

## 検証と受理

検証は、署名、ハッシュ、Merkle opening、および元の claim に束縛した局所化した一つの primitive の決定論的再計算である。検出対象、資料、局所化、証拠の大きさ、資源の上限に収まる類だけを受理する。秘密モデルと、公開できないプロファイルは、初期集合に入れない。

計算の暗号学的証明は使わない。ZK、SNARK、STARK、再帰証明、新しい proof VM は、将来の作業としても置かない。ハッシュ、署名、Merkle opening は同一性のためであり、計算全体の正しさの証明ではない。

## 時計

原点は元の受理 DAA `a` のままである。notice は `a` から `a+3,120` までである。係争のある claim の hard deadline は `a+10,000` である。後からリセットしない。異議のない claim を 10,000 まで引き伸ばさない。拒む設計は、完了 receipt の受理から 10,000 を始めることである。最後に合法な最初の notice のあと、残る予算は 6,880 DAA であり、現実のオフチェーン時間を含む。notice の前の 3,120 も検証の時間であり、ゼロではない。

`collect_until <= hard` と、clock-v2 の排他終端 `hard+1` は別の値である。一方を他方へ書き換えない。`palw_collection_door_within_route_hard_v1` は緩めない。`NOTICE_WINDOW_DAA` 3,120 と `DISPUTE_WINDOW_DAA` 10,000 は延ばさない。

収集と保持は別である。保持を `collect_until` の延長で実装しない。`collect_until` を hard の先へ置かない。306,108 DAA の局所化経路へ戻さない。1,798,149 を切り詰めない。

所有者の直列取得は、当事者ごとの notice と責任を消さずに、共有資料になる。並列にするには資源の予約が要る。プロセスを分けることは OOM の修正ではない。未公開または秘密のモデルプロファイルは初期集合に入れない。

## 順序

1. P0。pin。
2. P1。時間契約は一つ。
3. P2。共有経路。
4. P3。worker。
5. P4。測定したプロファイル。公開救済の最悪ケースが収まるもの。
6. P5。固定バイナリと、変えない一つの Studio 署名 TFP1 claim が、G14、公開裁定、Final、およびこの方針が列挙する trace 欄を持つブロックを通る。

古い claim は古い規則のままである。巻き戻しは、新しい受理を止め、既にある claim は続けることである。T12 の障害回復は、新しい規則の有効化とは別である。regenesis しない。同じ claim の実行が上のブロックを通る前に、main へ push しない。

そのブロックが持つ trace 欄は、元の受理 DAA `a`、notice 窓、hard、`collect_until`、`material_until`、時計版、公開スパンの拒否理由（拒否したとき）である。hold を Final としてこの欄に書かない。

## 実装状態

P0–P3 には試験がある。それは有効化でも、測定した受理プロファイルでもない。`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` は空である。この文書は測定 class の記録を追加しない。`palw_public_activation_v1.rs` の活性化は武装していない。T12 の upgrade はなく、main へは push していない。

RFC-0047 と Batch 時計版 5 はある。それらは計画時に 1,798,149 のドアを拒む。G14 の batch 予算（`palw_g14_values_v1` の max_elements 2、max_terms 3、max_target_requests 1、max_source_reads 1）は、測定したモデルでも、測定した Qwen 再実行でもない。fixture が収まったことは、公開 T12 の準備完了ではない。

### debug の局所 primitive（class の受理ではない）

`public_remedy_on_a_real_class` は、`/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/early-declared.palwtir` について profile debug を印刷した。最悪 leaf は `commit:post:33:Clamp`、leaf 45096 である。

| 欄 | 印刷 |
| --- | --- |
| `cpu_ns` | 1094223000 |
| `primitive_cpu_ns` | 764632000 |
| `recovery_cpu_ns` | 329591000 |
| `peak_rss_bytes` | 609337344 |
| `evidence_bytes` | 1070720 |
| `io_bytes` | 1038080 |
| `shared_material_bytes` | 1047935 |
| `submission_bytes` | 22785 |
| `shared_material_daa` | 16 |
| `primitive_daa` | 1 |
| `submission_daa` | 9 |
| `recovery_daa` | 1 |
| `ordered_public_door_daa` | 27 |
| `door_fits_6880` | true |
| `allowed_daa` | 6880 |

この 27 DAA は、debug profile の上の、局所化した一つの primitive である。class 全体の敵対的な最悪ケースではない。P4 の測定した受理プロファイルではない。preset を武装する許可でも、main へ push する許可でもない。

それより前の debug の全フレーム door は、upper 6 で 829,744、upper 257 で 23,741,483 のままである。どちらも 6,880 より大きい。その class は、測定して受理したプロファイルではない。

### 履歴（P1 の時点の読取。class の受理ではない）

RFC-0047 と Batch 時計版 5 が 1,798,149 を拒むことは、P5 を閉じない。下の表は、その時点の所有ファイルの読取である。

P1 をこの作業ツリーの所有ファイルで見た結果は次である。

| 門 | コード |
| --- | --- |
| `collect_until <= hard` を `hard+1` に書き換えない。`hard+1` を `hard` に書き換えない | 真。`palw_collection_door_within_route_hard_v1` は `collect_until <= hard`。`palw_court_clocks_v2` の排他終端は `hard+1` のまま |
| 保持を `collect_until` の延長にしない | 真。Batch v5 の `material_until` は `collect_until` より後であり、`collect_until` は動かない |
| 306,108 DAA の局所化へ戻してだけ収まる計画を、計画時に拒む | 真。`batch_span_fits_only_retired_localization_v1` と `batch_public_span_exceeds_notice_budget_v1` が計画時に Incomplete にする。1,798,149 は `DoorBeyondLiabilityHorizon` のまま拒む |
| 受理した Batch v5 の計画が、受理経路が大きさまたは期限で拒む異議アーティファクトを持たない | 真（計画と install）。`batch_public_objection_refused_v1` が `palw_close_fits_court_v1`、carriable、root claim、6,880、`collect_until <= hard` を読む。収まらない計画は Complete にしない |
| hold を期限内 Final にしない | 規則は Batch v5 だけにある。`batch_public_hold_end_v5` は abort と未使用予約手数料の決済であり、Final ではない。旧版はエラーであり、古い行を再分類しない |
| その hold を台帳の claim 行へ適用する | 未了。欠ける関数は `public_supply_failure_outcome_v1`（`consensus/core/src/palw_batch_public_budget_v1.rs`）。これは `NoSlashHold` を返すだけで、abort も未使用手数料の決済もしない。このファイルは今回の編集対象外である。`palw_claim_final_floor_v1`（`consensus/core/src/palw_state_v2.rs`）も hold 終端を読まない。所有ファイルの外なので、ここでは止めている |

収まる Batch v5 の見積は、なお Complete になり得る。その見積は測定した Qwen 再実行ではない。

## P0

この節は pin の時点の記録である。下の印刷は、その後の debug 測定でも、class の受理でもない。

HEAD は `2afa6ccf47efca499fc95fb7f3bb1468698c7837` である。この作業ツリーの `git status --short` は次である。他の作業者の変更を含む。

```
 M consensus/core/src/lib.rs
 M consensus/core/src/palw_batch_court_v1.rs
 M consensus/core/src/palw_court_clock_v2.rs
 M consensus/core/src/palw_court_shared_tir_fold_v1.rs
 M consensus/core/src/palw_tir_frame_cost_v1.rs
 M consensus/core/src/palw_tir_public_allocation_v1.rs
 M consensus/core/src/palw_tir_public_claim_v1.rs
 M consensus/core/src/palw_tir_public_fold_v1.rs
 M docs/design/palw/consensus/README.md
 M docs/maintenance/catalog.md
 M docs/maintenance/decision-catalog.json
 M docs/maintenance/numbering.md
 M docs/rfc-v2/README.md
 M docs/rfc-v2/registry.json
 M docs/rfc/INDEX.md
 M docs/rfc/TOPICS.md
 M kaspad/src/palw_tir_cone_owned_consumer_tests_v2.rs
 M kaspad/src/palw_tir_owner_replay_owned_tests_v2.rs
 M kaspad/src/palw_tir_public_filer.rs
 M kaspad/src/palw_tir_public_owner.rs
 M kaspad/src/palw_tir_source_worker_owned_tests_v2.rs
 M kaspad/src/palw_tir_source_worker_owned_v2.rs
?? consensus/core/src/palw_batch_public_budget_v1.rs
?? docs/design/palw/consensus/public-adjudication-policy-v0.1.md
?? docs/design/palw/tir/evidence/
?? docs/evidence/
?? docs/rfc/0047-batch-court-time-contract.md
```

`g14_native_funded_batch_candidate_v1` は、このツリーでは計画時拒否ではない。`cargo test -p kaspa-consensus-core --offline --lib -- g14_native_funded_batch_candidate_v1` は ok だった（フィルタ 5 件が 19.21 秒で通り、そのうちこの 1 件も ok）。fixture は Batch v5 の見積を Complete として受理し、テストは最後まで戻った。これは測定した Qwen 再実行ではなく、ライブ fleet の fingerprint でもない。

ライブの fleet fingerprint は測っていない。パラメータ関数は `palw_t12_shipped_params`（`consensus/core/src/config/params.rs`）である。ruleset の識別子関数は `palw_ruleset_id_v2` である。ここには測った fingerprint を書かない。
