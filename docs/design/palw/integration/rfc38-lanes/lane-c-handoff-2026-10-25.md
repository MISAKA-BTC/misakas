# Lane C 引き継ぎ（途中停止、2026-10-25）

開始 SHA `14fc6e997`。コード編集・commit はしていない。build も試験も実行していない。統括の指示で停止した。

## 対応表（ここまでに確認した分）

| 項目 | 仕様出典・採択状態 | 入力提供者・現 caller | 常時 Err/None の箇所 |
|---|---|---|---|
| funded original の fee/capital/retention/settlement schedule（RFC35） | RFC-0035 / ADR-0202 は Proposed。承認されたのは A（実 UTXO で裏付けた独立 escrow）の詳細仕様化と隔離実装だけ。金額・新料金・保持期間・本番採択は未承認 | `palw_funded_original_schedule_v1`（`consensus/core/src/palw_funded_original_coordinator_v1.rs:169`）。coordinator は `palw_state_v2` の funded coordinator fold から呼ばれる | production では常に `Err(Schedule)`。値が採択されていないので正しい挙動で、穴埋めしてはいけない |
| payee settlement（RFC35 §7.1/§9） | 同上 | `palw_funded_escrow_v1.rs:356`（payee は burn のときだけ None） | 監査は「payee settlement None」と指摘。精算 policy が未採択 |
| RFC08 一回決済 | RFC-0008 は Revised Draft | `palw_settlement_liability_fold_v1.rs`、`palw_state_v2.rs:1974` | `palw_settlement_once_v1` は全 preset で None（休眠） |
| RFC33 fee 1.3M / slash 300M / draw cap 64 | RFC-0033 は Draft。採択済みは ADR-0188 B の codec だけ | `palw_court_model_scope_routes_v1/readiness_disclosure.rs:67` | 定数はあるが movement 未接続。slash を適用する fold がない |
| RFC34 archive discharge | RFC-0034 / ADR-0201 S1 は判断待ち（Proposed） | `palw_court_archive_verify_v1.rs:110` `apply_native_archived_chunk_v1` | 非 test の実 caller がない |
| RFC32 material storage schedule | RFC-0032 は Draft。A の隔離実装だけ承認済み | `palw_court_shared_material_intake_v1.rs:85` | production storage schedule を拒否 |
| RFC26 全 branch 予約 readiness | RFC-0026 は Draft | `palw_court_shared_original_aggregate_v1.rs:120`。B の `AllBranchTermsSourceV1` が受け口 | Ready を返さない |
| RFC27 AccountID/lot/Panel | RFC-0027 は Proposed（未採択） | なし | runtime 不在（ABSENT） |
| RFC16 容量報告 | RFC-0016 は V+ 採択 | `palw_rfc0016_fold_v1.rs:968` | `capacity_per_span` と `capacity_shortfall_per_span` が固定 None |

## 次に進める候補（採択済み仕様だけで進められるもの）

1. RFC16 §11 列7。V+ は採択済みなので、`capacity_per_span` と `capacity_shortfall_per_span` を実容量の fold 値から埋めて RPC に出す。値の創作はいらない。ただし報告だけの項目で、資金の保存則には触れない。
2. RFC35 A の隔離実装。採択済みは方式だけなので、`test_only` schedule を使って実台帳で次を試験する。
   - escrow lock → 支出 → 返還の保存則
   - exactly-once と undo/replay
   - 資金不足のときに何も変わらないこと
   `palw_funded_original_coordinator_fold_v1.rs` に既存の試験があるか、先に確認する。
3. B の `AllBranchTermsSourceV1` 境界への接続案。schedule の `admission_v1()`（`FundedAdmissionScheduleV1`）を供給元にする形で、文書だけで出す。

## 決定票（草案）

- **D-C1 RFC35 schedule の値**：fee/capital の額、retention 期間、settlement policy ID、requirement budgets、role の契約 4 bit。これらが採択されるまで、production は `Err(Schedule)` のままにする。
- **D-C2 payee settlement**：payee の決め方、void/refund のとき誰に返すか、reward を一回だけ精算する経路（RFC08 の `palw_settlement_once_v1` と統合するかどうか）。
- **D-C3 RFC33**：1.3M fee の徴収 movement と返還条件、300M slash の適用条件と責任者。RFC33 本体が未採択。
- **D-C4 RFC34 S1**：詳細仕様化と隔離実装の承認が要る。
- **D-C5 RFC27**：AccountID と lot の採択。全体が未採択。
- **D-C6 native39→RFC12 写像**：W/PWU 変換、operator、anchor、retention、成熟、void。仕様が未策定。
