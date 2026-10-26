# lane VAL 報告 — G14 値 V1〜V12（2026-10-26）

Branch `claude/g14-values`（base `55510b56a`）。状態はすべて **目安（未実測）**。push・preset 変更・fence 武装はしていない。実測・drill は実行していない。

## 0. 縦切り slice（kernel route・Panel=1・genesis 1 class・job 1 形）に要る値

統括の計画変更（縦切り、17,000 ではその slice だけ武装）を受けて、slice が Final・決済・court に届くのに要る値を先に確認した。V1・V2・V3・V4・V11 はこの lane の目安で揃った。V6（ClaimReserve）・cert m/k（75/41）・V12 は 2026-10-22 pin で決まっている（V12-c `palw_material_income_v3` は値の対象外で、slice が monetary material を使わないなら前提にならない）。V5・V7〜V10 はこの slice では後回しでよい（V7 は D2 を使う場合、V10 は X3 を武装する場合だけ要る）。slice 専用の値は無い。全部の値は Panel=1・責任署名者 2 を前提に導いた。

## 1. V1〜V12 の現況

| # | 状態 | 出典 |
| --- | --- | --- |
| V1 court service profile | **本 lane で目安** | `palw_g14_values_v1.rs` |
| V2 funded original / admission | **本 lane で目安** | 同上 |
| V3 escrow 精算の取り分 | **本 lane で目安**（取り分の比率。dues 導出の production 実装は未） | 同上 |
| V4 外部者 court 受付料 | **本 lane で目安**（内部者も払う、R = ⌊cap/2⌋ 維持） | 同上 |
| V5 material 保管料・TTL | **本 lane で目安** | 同上 |
| V6 Final 後 default slash | 決定済み（ClaimReserve、コードに既存） | 2026-10-22 pin |
| V7 D2 draw cap・料金・slash | 決定済み（64 / 1.3 M sompi / 3 MSK） | 同上 |
| V8 TIR witness・lookahead | 決定済み（83,333 B / 3,072） | 同上 |
| V9 staged court 保持 | 決定済み（7,104 + 360 DAA） | 同上 |
| V10 X3 Δ・c_retain | 目安で pin 済み（実測で置換） | 同上 |
| V11 ρ_court・session weight | **本 lane で目安**（目安で pin、実測で置換） | `palw_g14_values_v1.rs` |
| V12 前提 fence 3 本 | 2026-10-22 pin のまま（bond budget は目安、model allocation は決定、material_income_v3 は値の対象外） | 同上 |

## 2. 本 lane の値と経済監査

値・式・見直し条件は [パラメーター台帳](../../../../governance/parameter-register.md) の「2026-10-26 lane VAL」節。要点:

- **V1**: packet 34,000 B（最大 court object 33,821 B）、**1 packet/DAA**（2026-10-26 統括判断の条件を満たすため 2 から変更。block の 27.2%）、12 packet/signer、span 833 DAA。各区間に 1 original あたり 2 packet、12 区間は a+9,997 で終わり、区間 0〜3 が notice a+3,120 を覆う。持続 1/24 original/DAA（名目 30/日）。超過は受付で拒否され、期限超過にはならない。DoS: 予約は fee と capital を伴う（V2）。
- **V2**: fee 0.816 MSK = 予約 packet 24 個の運搬の 2 倍。capital 0.816 MSK。
- **V3**: work 800‰ / relay 100‰ / burn 100‰。自分で払った claim は F の 10% を失い、補助金が無いので自作自演は無益。
- **V4**: 1.1 M sompi / session（opening の運搬 828,550 以上）。内部者も払う（共謀者が非予約枠を無料で埋めない）。
- **V5**: 50 sompi/B（P3 `c_retain` の前提と一致）、draft TTL 720 DAA。
- **F02 inclusion_delay_daa**: 4 DAA（目安、実測で置換）。B_carrier の考え方: 1 packet 34,000 B を観測最悪の空き 13,051 B/DAA で運ぶと 3 DAA、1 packet/DAA の予約で同じ区間の 2 個目が 1 DAA 待つので 4。184 s/DAA で約 12 分（最大 240 s で 16 分）、turn 42 DAA と span 833 より十分小さい。`CourtServiceInclusionPremiseV1` は 9803e043d にあり本 base には無いので、数値の const だけを置いた。
- **V11**: ρ_court 3,125 sompi/DAA/unit（session-DAA が court 予約運搬 34,000 B の持分を払う。旧 6,250）、session weight 13・op_work [5,13,1,11]（P1 M-RHO）。全 slot を 10,000 DAA 占有すると 171 MSK。正直な起訴の費用 0.28 MSK は最小 slash（D2 3 MSK）の報告者 49% = 1.47 MSK で回収できる。ρ を bond 相当（≥ 238,096）にする案は正直な起訴が約 10 MSK になり回収不能なので採らない。外部者の保証は予約 slot と期限順 queue による。

## 3. 独立検証

- 計算 script `g14_values_v1_check.py`（stdlib のみ）: 全項目 PASS。cert（f=0.25, m=75, k=41）の誤認証 4.0e-8/draw、正直認証 0.9926。D2 draw 64 は嘘の割合 ≥ 3.54% で検出 ≥ 90%、≥ 6.95% で ≥ 99%。
- Rust 試験 `palw_g14_values_v1::tests`（7 本）: V1 の packet が a+10,000 内に収まり空の book に予約できる、V2 の schedule が組める、V3 の保存則、V11 の policy が `validate` を通り cap 42 を保つ、V4 の R=21、V5 と c_retain の一致、本番の置き場が typed-missing のまま。結果は §6。
- PRINCIPLES §6 の 7 条件・ADR-0176 の Q/B/R/F は本 lane の値で変わらない（報酬・weight を与える値は無い）。Gate C の承認は m/k の再計算のみ。

## 4. 発見（統括の判断が要る）

- **cert m/k（訂正）**: 最初の報告の「上限まで使うと 75/41 は 0.06/年」は誤り（4,096 は position の数で、claim は ⌊4,096/75⌋ = 54/日）。正しくは 7.9e-4/年 < 1e-3 で、75/41 は既に条件を満たす。この誤りを根拠にした 89/50 への変更は実施せず、統括の再判断を待つ。89/50 にすると 1 claim あたりの再実行が 75 → 89 回に増え、上限下の認証数は 54 → 46 件/日に減る。
- production constructor 3 つを追加し、目安を `palw_g14_values_v1` から組めるようにした（preset と `palw_escrow_fee_schedule_v1` は未配線）。

## 5. 統合時に `None` 前提で壊れる試験

- `consensus/src/pipeline/virtual_processor/tests/g14_t_campaign/value_gates.rs` 24・29・54 行（V1・V2・V5 が未採択であることの assert）
- `consensus/core/src/palw_g14_gates_v1.rs` 492〜511 行（`ServiceOpportunityIncomplete` の拒否、F02 を前提にする fence の validation）
- `consensus/core/src/palw_funded_original_coordinator_native_test_block.rs` 142・200 行（`Missing(Schedule)`/`AdmissionSchedule`）
- `palw_court_service_binding_v1.rs`、`palw_court_paid_ingress_v1.rs`（`r5_f03_ingress_schedule_is_typed_missing_until_injected`）、`palw_court_shared_material_intake_v1.rs`、`palw_funded_escrow_v1.rs`（`SettlementScheduleMissing` 8 箇所）、`palw_funded_original_coordinator_v1.rs` の typed-missing 試験
- `palw_g14_values_v1::tests::val_production_sites_stay_typed_missing`（配線時に削除）
- funded original / admission / escrow は production constructor が無い（private field、`#[cfg(test)]` のみ）。配線には constructor の追加が要る。

## 6. ビルド結果

下の commit 時点の targeted test 結果を記す（§3 の Rust 試験）。

`cargo test -p kaspa-consensus-core --lib palw_g14_values_v1`: 初回 7 passed、改定後（1 packet・constructor・inclusion delay）**9 passed / 0 failed**（2026-10-26）。試験名 `val_v11_..._prices_a_full_jam_above_a_bond` は ρ 改定前の名前のまま（中身は「session-DAA が運搬持分を払う・全窓占有 ≥ 300 MSK・正直な起訴が 1.47 MSK で回収できる」を確かめる）。計算 script は全項目 PASS。full suite・clippy は回していない。

## Addendum (lane ARM-LEGACY, 2026-10-27): V11 against a measurement

The legacy one-move court verdict was measured on an M1 Max under load (about 11), dev opt-level 2: median 133.9 µs, p95 141.7 µs, 62 samples. At cap 42 that is about 6 ms per block. The measurement does not contradict the V11 estimate, so it stays as is (cap 42, weight 13, ρ_court 3,125). The opening, dissection and close classes are not measured, and neither is the minimum-spec node; this remains an estimate and is no record. Evidence and the conditions for revisiting it: `docs/evidence/palw/g14-17000/measurements/M-CAP/arm-legacy-2026-10-27/README.md`.
