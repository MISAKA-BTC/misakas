# G14 family lane FPCP（`FpConstraint`・`FpPrefix`）報告 — 2026-10-26

branch `claude/g14-fam-fpcp`（基点 `d8aded6de`）。push・配備・activation 変更なし（本番 preset の高さは `None` のまま）。

## 段ごとの状態表

凡例: 実装 = source にある／native = native 試験あり／未 = 欠落・未測定。記録（record）は全 fence で空。

| 段 | FpConstraint（Job V6） | FpPrefix（job 11・12） | 根拠 |
|---|---|---|---|
| 受付（option (b)・G-05） | 実装・試験済み: 経路未武装なら `G14_JOB_VERSION_UNJUDGEABLE` で拒否、constraint は Selection＋Constraint＋Tokenizer 全武装＋frame 付きで受理 | 同左（Selection＋Prefix＋Tokenizer） | `palw_g14_route_fp_refusal_v1`、`g14_close_gates.rs` T-N10 |
| duty・供給 | staged Constraint court の source session、Float32 seed（別 fence `palw_staged_float_constraint_v1`）実装 | PublicPrefix source・custodian responder・tag 169 layout 実装（TIR prefix-state relation は matrix 上 refused が残る） | `g14-completion-matrix.md` §2i、`prefix-material.md` |
| 正直 Final・報酬 | 機構は共通（Final hold、liability horizon）。family 固有の node E2E は未 | 同左 | — |
| 嘘 → 外部者の有罪・slash | fresh service 試験 3 件（forbidden／premature stop／beaten token）PASS（2026-10-11、旧経路）。staged court での全員共謀 node E2E は未 | 実 Prefix honest/mismatch 再生 PASS（selected）。全員共謀 node E2E は未 | matrix 1471 行、ledger 63 行 |
| 沈黙 → default | 共通 DA default（LG14-A/F02）を使う。family 固有の試験は未 | 同左 | — |
| reorg・restart・pruned IBD | 未（family 固有） | 未 | — |
| 保持期間（V9） | 決定済み: `network_v1` collection 7,104・reorg 360 → a+11,200 以上を覆う。今回 native 試験で確認 | 同左 | `palw_rendering_resources_v1.rs`、本 lane 試験 |

## 実装（この branch）

1. `central:` `PALW_T12_FP_CONSTRAINT_PREFIX_FENCES_V1`（`full_activation_v1.rs`）: constraint/prefix job が報酬を回復するのに要る fence の部分列（int-13 の 4 本、LG14-A、staged court、staged TIR route、TIR container identity、TIR public conviction、route-required）。全体 list と同順。LG14-B は含まない（FP family は読まない）。
2. `central:` `PALW_FP_CONSTRAINT_PREFIX_ROUTE_EVIDENCE_V1`（`palw_g14_gates_v1.rs`）: staged court／LG14-A の V2〜V7 record が含むべき family 固有の事例。record は fence 単位のまま、空。
3. 試験 `consensus/core/tests/g14_fam_fpcp.rs`（4 件）: list の被覆と順序、17,000 で FpConstraint/FpPrefix/FpTokenizer が武装（16,999 は不可）・Selection は frame 付きのみ・dormant に戻すと ids がバイト同一、record 要件、V9 の被覆。

新しい値は追加していない（V9 は 10-22 決定値が `network_v1` に既にある）。

## 試験

`buildslot.sh … cargo test -p kaspa-consensus-core --test g14_fam_fpcp` — exit 0、4 passed / 0 failed（同一 invocation 1 回）。fleet identity script・full suite は未実行（dormant の ids 同一は試験 2 で確認）。

## 残件（武装までに要るもの）

- staged court の全員共謀 node E2E（V2/V3）を Constraint・PublicPrefix それぞれで、honest twin（V4）、restart/reorg/pruned IBD（V7）。
- V5 実測（最大 profile で staged court の資源）と出荷 binary の fence 跨ぎ drill。
- TIR prefix-state relation の refused 残（matrix §2i）。prefix は FpSelection が frame を要するため、TIR FP 実行 backend（RFC-0002 lane）にも依存。
- `palw_staged_float_constraint_v1` は greedy Float32 constraint だけの別 fence。FPCP list には入れていない（Float32 constraint を扱うなら追加が要る）。

## 統括への依頼

- FpTokenizer lane と fence 集合が同一（staged court＋LG14-A）。FPCP list をそのまま共有できる。
- LEGACY へ: LG14-A の record に constraint/prefix の事例（上の evidence 定数）を含めるか、staged court record 側に寄せるかの決定。
- pin は実行結果から統括が行う（本 lane は record を空のままにした）。

## 追補 2026-10-26（統括の指示 (1)〜(3)）

統括決定: family 固有の事例は staged court（`palw_staged_public_court_v1`）の record に入れる。LG14-A は共通の public filer の record。FPCP list は FPTOK と共有。

### (1) native E2E（kaspad の in-process service、実 virtual processor）

1 invocation: `cargo test -p kaspad --lib -- fpcp_ constraint prefix --test-threads=2` → exit 101、43 passed / 6 failed、861 s。

| 事例 | 試験 | 結果 |
|---|---|---|
| constraint 全員共謀（forbidden／premature stop／beaten、untiled・tiled） | `fresh_public_bond_service_*constraint*` 6 件 | PASS（producer と全 Valid seat の有罪・徴収） |
| constraint の honest twin | `…replays_an_admitted_large_constraint_job_from_public_multipart_history` | PASS（dispute なしで release） |
| constraint 取得不能 → DA default | `…defaults_on_a_withheld_large_constraint_job`、`…tiled_constraint_withheld_selected_tile_defaults` | PASS（計算誤りの有罪にならない） |
| prefix（登録 class、pruned import 後） | `…registered_prefix_initial/inherited_after_pruned_import` | PASS |
| **prefix native public（staged PublicPrefix court）全 custodian 有罪** | `…native_public_prefix_initial/inherited_convicts_all_custodians` | **FAIL（基点 d8aded6de のまま）**: prefix job が「carried but not admitted」、fold rehearsal `None`（`palw_service_public_e2e.rs:1267`） |
| restart/reorg（新規 4 件、`palw_service_recovery_e2e.rs` の `fpcp_*`） | constraint forbidden × Restart/Reorg、public prefix × Restart/Reorg | **FAIL**: constraint は recovery harness が tiled fixture の資金を再提出（`RejectDoubleSpendInMempool` / `RejectDisallowedOrphan`、`palw_service_tiled_e2e.rs:591`）、prefix は上の不受理。`#[ignore]` に理由を明記 |

注意: 既存の constraint 有罪事例は staged court を武装せず、LG14-A（legacy public filer）経路で裁かれる。staged Constraint court を通る node E2E はまだない。

### (2) TIR prefix-state relation

- relation・court・custodian responder・backend（`TirBackendV1::declare_tir_prefix_state_v1`）は実装済み。拒否が残るのは 2 点。
  1. 揃っていない k: 設計上の拒否（recurrence replay = kernel court が要る）。接続しない。
  2. `palw_tir_fp_derive_work_v1`（`palw_tir_fp_lane_v1.rs:120`）が IR FP claim の prefix state 消費（genesis 以外）を拒否する。接続には新しい fence（新規 position 分だけ credit）と、FpSelection の frame（TFP1）が prefix job 11/12 を覆うことが要る → **FPSEL2 依存**。本 lane では変更していない（FPSEL2 の基点にまだ commit がない）。

### (3) Float32 constraint（`palw_staged_float_constraint_v1`）を list に足す条件

- staged court が同じ高さかそれより下で有効（validation 2968 行）、ConsensusV2。
- V1〜V7 の record と実測値（`needs_measured_value`）。
- Float32 の row を保持する producer（matrix 881 行で欠落）と greedy 以外の job は対象外。
- 全員共謀の Float32 constraint node E2E と出荷 binary の drill。
