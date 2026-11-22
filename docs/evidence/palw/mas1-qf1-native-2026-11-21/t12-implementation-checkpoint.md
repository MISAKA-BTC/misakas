# T12 実装・短時間試験記録

対象は private checkout の `db9bcf3bc`。ビルド終了コード 0。保存台帳はコピーを使用した。

## 確認した動作

- 受賞fixtureは生成ノード1台と、同じDAGを別DBで受理して再起動した2台。fee出力500,000 sompiを含むvirtual UTXO集合、sink、PALW state root、同一claimの消費数1が一致した。これは同一プロセス内のfloor roundレーンで、TFO3のreceiptブロックとは別である。
- 合意へのreceipt入力は従来どおり候補DAAで検証する。producerの候補一覧も同じDAAを使い、親が切り替わった読み取りは破棄する。二重消費用fixtureで、期限当日の勝者を取得でき、翌DAAでは候補から消えることを検査した。
- Round 16再開処理は、保存DBを新しい一時ディレクトリにコピーして開き、記録済みのtip・root・分岐親・資金outpointと照合する。アーカイブ内でmaterialized tipを特定し、その次のブロックから通常の受理処理に戻る。
- private MOE2登録試験の3プロセスで、登録市場の生成・再編、買付・LP・burn・残余の計算とprojected deltaを確認する。実際の受賞で解放された原資をselected chainで決済する試験は残っている。

## 未完了の接続

- 保存されたStudio TFO3は旧規則で21量子すべて落選し、残存期間のtarget上限でも当選者はない。受付済みの権利や抽選条件を書き換えて成功させることはできない。受賞の実証には元の規則で当選した別claimが必要。
- Round 16の算術提出は非報酬。保存台帳を使うDAA 53098の成熟work、blue 181382のclose、share公開・seed復元・選出Panelの全再実行と承認は長時間検証に残る。
- QF1の新規採掘はprivate readerのQf1Use::NewMiningでREVIEW_HOLD。FOW1の実ブロック受理には、適格な規則と真正なclaim単位のGate C・Court・権利消費を結ぶ経路が必要。
- MOE2は元の受付で固定した条件を必要とする。旧claimに事後のMOE2条件を設定できない。現在settle_private_moe2を呼ぶmoe2_checkpoint.rsのprojectは、分岐から取得したdeltaに予約と報酬を投影する。実ブロックの所有するfoldへ、認証済み消費と元の予算予約を同じ遷移で接続する作業が残る。

## 実装修正

| commit | 変更ファイル |
|---|---|
| `3386a7b2f` | `consensus/src/pipeline/virtual_processor/tests/t12_round_lane_e2e.rs` |
| `8f45497e0` | `consensus/src/pipeline/virtual_processor/processor/palw.rs`, `consensus/src/pipeline/virtual_processor/tests.rs` |
| `c050cea23` | `consensus/src/pipeline/virtual_processor/tests/tb1_fresh_audit.rs`, `consensus/src/pipeline/virtual_processor/tests/tb1_stable_panel.rs` |
| `0a3a0853f` | `consensus/core/src/palw_model_market_v1/moe2/settlement.rs`, `consensus/src/pipeline/virtual_processor/tests/g14_registration_e2e/moe2_reservation.rs` |
| `db9bcf3bc` | `consensus/src/pipeline/virtual_processor/tests/tb1_fresh_audit.rs` |

## 個別実行結果

| 試験 | 終了コード | 実行時間 |
|---|---:|---:|
| `g14_adversarial_c_gate_c_with_only_colluders_holding_the_model_is_not_certified` | 0 | 8.39s |
| `legacy_v2_attempt_held_da_withholding_ends_in_da_default` | 0 | 13.06s |
| `native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected` | 0 | 1.01s |
| `palw_v2_a_quantum_spent_twice_in_one_mergeset_is_paid_once` | 0 | 1.08s |
| `private_moe2_three_process_registration` | 0 | 0.52s |
| `qf1_original_requires_actual_stop_and_complete_trace_budget` | 0 | 0.05s |
| `reject_panel_parent_at_share_release` | 0 | 0.24s |
| `replay_to_original_panel_parent` | 0 | 0.22s |
| `rfc0012_native39_fixture_clock_survives_same_db_restart` | 0 | 0.16s |
| `rfc9_c5_v3_is_unchanged_and_a_v3_and_a_v4_spend_of_one_quantum_are_one_double_spend` | 0 | 1.16s |
| `t12_a_final_that_mints_no_ticket_holds_no_round_permit` | 0 | 1.86s |
| `t12_a_floor_final_becomes_a_permitted_round_block_whose_fee_pays_its_bond` | 0 | 4.32s |
| `t46c_forged_output_full_mask_only` | 0 | 0.58s |
| `resume_round16_across_saved_input_cursor` | 0 | 91.90s |

blue 102522 の tip は `07d42def9040cf1f0b57e52682eb11e93810cde8f9b9fda50189d7bf9d7d4e944d839506eb5af7b9fe32244185d6d33e5defa5568a986b3b0ad460f7bdaa1e8b`、state root は `33318f4cab59ba2e70503d1b776de15e10c767dbc817646b0471c22f76d8384050c5c6713a8847352fff636e8aa44ace51221e4963ca36ac6bdd845ee017e016`。

引き継ぎ時の native39 本試験は SIGTERM で終了。上表の clock 試験は本試験の完走件数に含めない。最終の隔離3ピアの一致・決済は未検証。
