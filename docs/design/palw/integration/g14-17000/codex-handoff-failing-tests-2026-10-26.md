# Codex への引き継ぎ — Codex 側の基点で落ちる試験（2026-10-26）

作成: Claude（G14 統合の統括、branch `claude/g14-panel1-17000`）。統合検証（`d8aded6de`、consensus 299 件中 22 件失敗）のうち、Claude の lane で直したもの以外を、Codex 編集範囲として渡す。Claude 側はこれらのファイルを変更していない。

## 1. Codex の基点 `109f409cc` で同じく落ちる 8 件（統合が原因ではない）

lane INTEG（branch `claude/g14-integ`）が基点で再実行し、同じ panic・同じ値で落ちることを確認した。

| 試験 | 失敗箇所 | 所見 |
|---|---|---|
| `conformance::exec_slices::native_final::x8_g14_native_happy_final_pays_all_positive_funds_once…` | 担保・slash の金額 | 5 MSK（500,000,000 sompi）ちょうどずれる |
| `…native_final::x8_g14_native_paid_material_rejects_one_daa_beyond_signed_exec…` | 同上 | 同上 |
| `…opv_settlement::x8_g14_opv_settlement_never_substitutes_for_selected_exec_approval` | 同上 | 同上 |
| `…exec_slices::x8_g14_a_withheld_slice_is_demanded_by_an_outsider…` | — | 基点と同一の panic |
| `…exec_slices::x8_g14_honest_slices_verify_through_kernel_finals…` | — | 基点と同一の panic |
| `…owning39::master::g14n2_node_a_forged_mining_claim_is_convicted…` | `master.rs:589` | `certified by the colluding draws: None` |
| `…master::node_e2e::rfc02_native39_node_e2e_job_credit_delivery_witness…` | `node_e2e.rs:423` | `native39 delivery witness is not the bracketing state of the same immutable credit` |
| `…master::node_e2e::rfc02_native39_node_e2e_register_fund_final_gate_c_pnr9…` | `node_e2e.rs:501` | `native39 receipt's budget reservation cannot pay this block (ADR-0176, 0.51 export)` |

exec_slices の 5 MSK のずれは paid/kernel escrow の裁定が要る。native39 の 2 件の文言は `palw_native_receipt_v39.rs` / `palw_native_redemption_v33.rs` から出ている。

## 2. Codex の Panel=1 試験 `g14_kernel_route_panel_one_assignment_and_outsider_conviction`

lane CLOCK2 の調査:

- `g14_kernel_route_e2e.rs` の `kernel_config_panel_one`（72〜84 行）が試験用の外部利得上限を設定していないため、class が登録されない（F03k2 の fixture は設定している）。
- 992 行は commit 直後に seat があることを期待している。lane SEED（`648544c46`）で Panel=1 の seat と sample seed は commit より後の seed anchor から引く規則に変わったので、post-commit anchor まで attempt block を進めてから seat を読む必要がある（CLOCK2 の fixture 修正 `ee96b7b9f` と同じ方法）。

## 3. wire の互換（既報）

Codex の branch で `PaidClaimObjectV2` の wire 判別子がずれていた件は、lane kernelfix2（`ebd05eb62`、統合では `102349a3f`）で main の 15..19（V5..V7）を保ち、休眠の TrapCommitment を 20 に追加する形で直した。Codex 側の branch にも同じ修正が要る。
