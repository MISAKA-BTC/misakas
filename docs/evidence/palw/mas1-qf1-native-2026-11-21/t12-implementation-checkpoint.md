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

## 追加検証

`0bbc98ec0` は同じsinkの受賞観測で各ピアのnative reward state rootと報酬残高を比較する。protected drillのPython試験78件が終了コード0。

`b82b279e3` はEXEC closeの予算見積もりにも、署名したMOE2条件の適用開始、モデル登録、元期限、稼働中の予算policyを適用する。締切後とprofile開始前の予約を拒否し、担保はexport上限で署名済み報酬全額を賄う最小整数を使う。変更は `consensus/core/src/palw_model_market_v1/moe2/settlement.rs`、`misaka/palw/remote/src/tb1_exposure/exec_input/moe2.rs`、`consensus/src/pipeline/virtual_processor/tests/g14_registration_e2e/moe2_reservation.rs`。nativeテストビルドは終了コード0。

| native試験 | 終了コード | 時間 |
|---|---:|---:|
| `private_moe2_three_process_registration` | 0 | 27.65秒 |
| `private_registered_tb1_pending_freeze` | 0 | 25.80秒 |
| `private_tb1_stable_panel` | 0 | 4.55秒 |

登録済みQF1の試験で、3プロセスがDAA 33で短期offerの期限経過を確認した。長期offerの見積もりは購入4,800,000,000 sompi、永久LP 3,200,000,000 sompi、余剰Position 1単位burn、miner 6,400,000,000 sompi、Panel 1,600,000,000 sompi。実際のreward debitは0。

`private_read_cold_panel_receipt` も、独立した3つのRound 3チェックポイントを各プロセスに展開して実行し、各終了コード0、各0.31秒。復元seedは `a68ed6b9029e13b731a6947250e992b416ad5b2a98881e170a9d6d15bb2b219c`。3つとも同じ2席を選出し、記録済みの両署名を検証した。

受賞からMOE2に接続する際は、署名済みEXEC closeの `consent_ref(reg_key, slot)` と、claim確定後の `Moe2SettlementTerms::reservation_key()` が別の識別子になる。前者で現在実行する予算予約はscratch budget内で終了し、後者のキーで実受賞時に消費できる行は生成されない。元の署名・slotと、同じ登録から発生し認証されたclaimの対応を選択鎖の状態遷移で保持する必要がある。旧claimの予約に新しい条件を後付けしてはならない。

## EXEC closeの予算予約

`fc62f4e66` で、署名済みslotについて、通常処理が生成したclose差分の末尾に予約を追記するprivate APIを実装した。対象は `misaka/palw/remote/src/tb1_exposure/exec_input/moe2.rs` と `consensus/src/pipeline/virtual_processor/tests/tb1_holder_finality.rs`。元のcloseと親状態を照合し、同一ブロック内の先行書き込みを適用した状態から予算を算定する。全slotが成功し、選択tipが一致した時点で書き込みを追記する。

3プロセスが同じ実ブロックから読み出した差分に各自で予約を適用した。closeは `f1521c911aa04360fe4f3927f2d22d7b8d32a2b8cab69111acabafb3134cf4c4cf11cdf4ffc52245433faa27fdc785a7ab82a59a1258310cdb94511fa2e29cde`、予約前の親rootは `1c5257e0624ac46f0d7ff953dec7573367808842cd9c65308a73f144e5c30dc4113f9074d48655d65374097e698c435d7363e3e961dae3e0a82093280b7f82a6`、2件の予約を含む試験上のpost-state rootは `1640360f280f63d79341adcf64680d086d34d436fffbe5ce192036be849bef263049ad0696458d106774ebc9f5b78516fb7851376c29a3e43659dbebf9c71e56` で一致した。重複予約の拒否、差分取り消しによる親rootの復元、再適用によるpost-state rootの再現を各プロセスで検査した。

今回のnativeビルドは終了コード0。 `private_registered_tb1_pending_freeze` は25.95秒、`private_moe2_three_process_registration` は22.19秒、`private_tb1_stable_panel` は4.58秒で、いずれも終了コード0。上記rootは試験で予約を加えた分岐状態の値であり、ネットワークの選択済みブロックヘッダーへのcommitは未実証。通常のblock ownerからの呼出しと、当選claimに結び付けた一度限りの予算消費・決済が残る。
