# RFC-0047 — Batch court time contract: one door, separate retention, public 6,880

- 状態: 2026-11-07 のユーザー再検討。時間契約の切片を記録する。上位の採択方針は [公開裁定の方針 v0.1](../design/palw/consensus/public-adjudication-policy-v0.1.md) である。P0–P3 には試験がある。P4 の debug 印刷は class の受理ではない。P5 は未了。出荷 preset は武装しない。main へは push していない。T12 の upgrade はない。条件 2–4 を完了とは書かない。
- 関係: [設計原則 §8](../PRINCIPLES.md#8-モデル計算の暗号学的証明を使用しない2026-10-15ユーザー恒久決定)、[設計原則 §9](../PRINCIPLES.md#9-モデルの二階層モデル単位の検証は不採用2026-10-15ユーザー決定)、[設計凍結](../PRINCIPLES.md#design-freeze)
- 番号: ローカルの `docs/rfc` で RFC-0046 の次。15 本の RFC v2 には混ぜない。

## 再検討

Panel が発行する Signed Deadline Attestation / `OffchainDialogueReceiptV1` を G14 の受理経路にする案は不採用である。その receipt 型は置かない。Panel の署名は署名者を識別する。定足数は評決ではない。裁判所の時計を、後から受理した receipt の DAA へ動かさない。完了 receipt の受理から 10,000 を始める案は拒む。アンカーと Panel 署名は補助記録にすぎない。それらだけでは Final も slash も出さない。

時計は元の claim 受理 DAA `a` に固定する。最初の notice は `a + 3,120`。係争の hard deadline は `a + 10,000`。最後に許される最初の notice のあと、残る最悪予算は 6,880 DAA である。`DISPUTE_WINDOW_DAA` と `NOTICE_WINDOW_DAA` は変えない。`palw_collection_door_within_route_hard_v1` も変えない。10,099 と 10,100 は収まり、10,101 と 1,798,149 は拒む。

オフチェーンの短い経路だけでは足りない。保留、最も遅い応答、公開経路での回復も収まるか、受理の時点で claim を拒む。受理したあとで 306,108 DAA の局所化経路へ戻さない。

306,108 は現行の再試行契約である（64 × acquisition 4,096 = 262,144 とその残り）。測定したモデル実行時間ではない。所有者キュー 1,476,360 は現行の出席積であり、代わりの公開提出機会なしには消さない。filing 203 と collection 7,104 は和ではなく max である。キューと局所化を除いた残りは 15,581 DAA で、なお 10,000 を超える。受理 100 では 1,476,360 + 306,108 + 15,581 = 1,798,049、`collect_until` は 1,798,149、hard は 10,100 である。

旧資料の保持は collect に reorg_retention 360 を足す。clock v2 が求めるのは `a + 10,000 + max(W_disclose 1,200, reorg)` である。キューと局所化を 0 にしても、この旧い組は 840 DAA 短い。この不足はその組に限る。family material v4 は collect の終わりと保持を分けている。Batch はその考え方だけを新しい版で使う。family の述語は Batch に写さない。

`palw_g14_values_v1` の batch 予算（max_elements 2、max_terms 3、max_target_requests 1、max_source_reads 1）は測定したモデルでも、測定した Qwen 再実行でもない。fixture が収まったことは公開 T12 の準備完了ではない。306,108 DAA の局所化経路は、この設計の代替経路ではない。

計算の暗号学的証明は使わない。ZK、SNARK、STARK、新しい proof VM はない。ハッシュ、署名、Merkle opening は同一性のためであり、計算全体の正しさの証明ではない。`PredicateHolds` は計算全体の証明ではない。有界の native 検証は、Merkle / 同一性の opening と、局所化した一つの primitive の決定論的再計算までである。

## 完了条件

順に四つある。この切片は条件 1 と、コードが実際に変える条件 2 と 3 の一部だけを始める。条件 2–4 を完了とは書かない。

1. Batch の時間契約は一つである。計画、資源見積、Studio が署名する frame、`install_batch_frame_v1` / `install_original_frame_rows_v1`、Final の条件は同じ規則を使う。署名される `collect_until` が hard を過ぎる計画は、install の前に Incomplete として拒む。署名済みの期限は切り詰めない。
2. この新しい Batch 版だけ、収集の完了と資料の保持を別欄にする。裁定は既存の hard deadline で終わる。保持は後の義務であり、10,000 DAA の係争窓を延ばす理由にしない。`collect_until` を hard の先へ置かない。既に署名された claim の保存済み期限はそのままである。
3. この新しい経路では、所有者ごとの直列キュー積を、既存の共有要求と非独占の資料受理、および有界の native 検証に置き換える。最後に合法な最初の notice からの依存を含む公開グラフが残り 6,880 DAA に収まらなければ、受理を拒む。受理後に 306,108 DAA の局所化へは戻さない。Panel は取得、再実行、局所化をするオフチェーン作業者である。止まった Panel や結託した Panel のあとも公開提出経路は残る。包含の前提が無い供給失敗は `NoSlashHold` のままで、`PartyDefault` にしない。私的な Panel transcript から slash も Final も出さない。
4. 保留、最も遅い公開応答、公開経路の回復を、その同じ公開グラフの残り予算の中で価格づけし、Final は公開証拠に置く。この切片はその価格づけを終えない。

## この切片が変えること

新しい時計版は `PALW_TIR_FRAME_CLOCK_BATCH_PUBLIC_V5` である。family v3 / v4 の述語には入れない。出荷 preset、`PALW_TIR_FRAME_COST_RECORDS_V1`、`palw_batch_court_v1` はこれを選ばない。

Batch の署名 fixture は G14 の暫定見積を一つの契約として使う。`network_v1()` と `palw_tir_frame_test_only_cost_v1().clock` を混ぜて G14 契約とは呼ばない。見積の `collect_until` が `a + 10,000` に収まるときだけ Complete になる。保持は `material_until` であり、`collect_until` をその保持期限まで広げては作らない。

古い混合で出る長いドアは、計画時に `DoorBeyondLiabilityHorizon` として拒む。`required` は計算したドアのままである。1,798,149 を 10,100 へ書き換えて署名可能な計画にはしない。

Batch v5 で、最後の合法な最初の notice のあと必要な公開スパン（局所化パスと、filing と collection の max）が 6,880 を超えるときは `PublicRecoveryExceedsNoticeBudget` として拒む。所有者キュー積はそのドアに足さない。共有要求と非独占受理のモジュール自体は、この切片では置き換えない。それらが公開提出機会として残る。

family material v4 を Batch fixture に載せて通した経路は、この版へ移す。family 時計を family 見積で再計算する fold の分岐は、family の契約として残す。Batch v5 はその述語に入らない。

`install_batch_frame_v1` は `install_original_frame_rows_v1` を通る。Batch v5 の install は、見積と同じ二つの述語を読む。`collect_until` が `a + 10,000` を過ぎていれば拒む。局所化と collection 窓の和が 6,880 を過ぎていれば拒む。保存済みの期限は書き換えない。Final の床は既存の `palw_claim_final_floor_v1` が `collect_until` を使う。`material_until` は保持であり、その床を hard の先へ押さない。

ライブの T12 測定は未了である。この切片は公開 T12 の準備完了を主張しない。P5 の、固定バイナリと変えない一つの Studio 署名 TFP1 claim が G14、公開裁定、Final、および方針の trace 欄を持つブロックを通る実行は、まだない。その前に main へ push しない。巻き戻しは、新しい受理を止め、既にある claim は続けることである。T12 の障害回復は、この規則の有効化とは別である。regenesis しない。

Batch v5 の計画時検査は、`collect_until <= hard` と clock-v2 の排他終端 `hard+1` を別の値のままにする。保持は `material_until` であり、`collect_until` を動かして作らない。306,108 DAA の局所化スパンにだけ収まる公開スパンは計画時に拒む。1,798,149 は切り詰めない。異議アーティファクトが受理経路の大きさまたは期限に収まらない計画は Complete にしない。hold の終端は Batch v5 の `batch_public_hold_end_v5` が abort と未使用予約手数料の決済と定義し、Final ではない。その決済を台帳へ書く関数 `public_supply_failure_outcome_v1` は、この RFC の切片では未接続である。hold の終わり、abort、費用の決済は、採択の前に指定する。台帳への接続が無いことは、hold を Final にしない。

## 測定の印刷（class の受理ではない）

`public_remedy_on_a_real_class` は、`/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/early-declared.palwtir` について profile debug を印刷した。最悪 leaf は `commit:post:33:Clamp`、leaf 45096 である。印刷値は `cpu_ns` 1094223000、`primitive_cpu_ns` 764632000、`recovery_cpu_ns` 329591000、`peak_rss_bytes` 609337344、`evidence_bytes` 1070720、`io_bytes` 1038080、`shared_material_bytes` 1047935、`submission_bytes` 22785、`shared_material_daa` 16、`primitive_daa` 1、`submission_daa` 9、`recovery_daa` 1、`ordered_public_door_daa` 27、`door_fits_6880` true、`allowed_daa` 6880 である。

この 27 DAA は、debug profile の局所化した一つの primitive である。class 全体の敵対的な最悪ケースではない。P4 の測定した受理プロファイルではない。`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` は空のままである。この RFC は測定 class の記録を追加しない。preset を武装せず、main へ push しない。

それより前の debug の全フレーム door は、upper 6 で 829,744、upper 257 で 23,741,483 のままである。どちらも 6,880 より大きい。その class は、測定して受理したプロファイルではない。詳細は [公開裁定の方針 v0.1](../design/palw/consensus/public-adjudication-policy-v0.1.md) に置く。
