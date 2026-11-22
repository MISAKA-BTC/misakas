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

## 保存台帳での新規Studio 8K受付

DAA 51258・blue 102520のDBを読み取り専用で開き、保存ブロックとPALW rootが一致する先端で登録状態を列挙した。再現には引き継ぎ記録の `verify_recovery_snapshot_tip` を使う。nativeビルドとこの試験の終了コードは0、試験は1.08秒。[5クラスの全記録](round16-studio-inventory.json)

登録5件はすべてActive。TIRの最大コンテキスト8192の2件（DAA 1、7で登録）は、この状態で `palw_fp_lane_available_v1` がともにfalse。さらに、保存系統の元の設定で `palw_tir_public_conviction_v1_fence()` はNone。DAAを進めても発効予定はない。通常の `palw_accept.rs` のfunded original処理はこの場合carrierを拒否する。したがって保存台帳の規則では、どの登録モデルにも新規Studio TFO3を受け付ける経路がない。旧Studio claimを受理した隔離系統のgenesis（3b4233ae…）と保存台帳（20482be2…）も異なる。新規nonceによる推論・抽選・受賞は未実行。

`b43fb4ebc` はPFS3/PFS4の候補一覧から署名直前まで、適格性を評価した親とDAAの組を使う。consensusとkaspadのテストビルドはともに終了コード0。同じ親で期限をまたぐ候補・二重消費（1.02秒）、3 DBのfloor受賞（4.50秒）、funded originalの不完全受付拒否（0.83秒）、kaspadのT12 producer 2件（計0.10秒）はすべて終了コード0。

## Panel承認とcloseのfinality

`303157128` はprivateの算術PanelとTB1実行Panelで、署名集合、過去の承認、未検証期限終了を返す直前に、対象closeのfinalityとcertificateの選択鎖上の位置を再照合する。同じsinkでもcommitment照合中にfinalityが撤回された場合を回帰試験に加えた。remoteの対象13件は終了コード0（3.76秒）、nativeテストビルドは終了コード0。

以下はRound 3の非報酬3プロセスfixture。各試験で3つのPIDが同じ結果のブロックhashを返した。

| 試験 | 観測 | 終了コード |
|---|---|---:|
| 期限・再起動（4.81秒） | 選出2席が完全再実行。DAA 27 の承認 `9fe6d2ba64927ee57af41461f763f0342eb9a0cffc6f054e20ea8c2028b9f87294c386a5f94cd21b3851cf85d7fe4892e5fff5e697c90f31d954310ec7342d91` を期限 60 のDBから復元し、期限当日の新規承認を拒否。 | 0 |
| 偽readout（4.37秒） | 選出席は署名せず、Court Endは3ピアとも `ComputationRefuted`。裁定ブロック `bcf22f7182c500a7ec0b27daf2a777cfc30bbfe824ee6085bb5b1c4443edc4bdced50c4a158b36eae6283c39a52a8458239fd3ac4fc4fe88d899b5645d6e5585`。 | 0 |
| 資料遮断（261.39秒） | Qwen3-8Bで1席の登録済みscale sectionを遮断。1署名だけで期限に達し、3ピアが未検証終了 `e220caac5af4f711b4b3b0c2f7d9e898e83b9344513bc2e757d2524fb44a38c8751242ac4c7a85b88579095144b565035410259e72f5fded9c487d3a4e1f3815` をDB再開後も確認。 | 0 |

Round 16の実close、当選権利の消費、MOE2決済は上記の試験範囲外。

## EXEC予約から一回決済までの検証

| commit | 変更ファイル | 動作 |
|---|---|---|
| `4c176a3a6` | `consensus/core/src/palw_bond_budget_v1.rs` | 同じ予算行にclaim 1件と実際のdrawを記録する。drawがゼロでも一部でも、そのslotの2回目の消費を拒否する。 |
| `64b6c8d8c` | `consensus/core/src/palw_model_market_v1/moe2/settlement.rs`、`consensus/core/src/palw_state_v2/private_moe2_settlement.rs` | 元のRegKey・slot・close・見積もりにclaimを結び付ける。登録と決済を逆順に戻せる。新形式のcheckpointは割当を復元し、旧形式も読み取る。 |
| `1a5c727a1` | `misaka/palw/remote/src/tb1_exposure/exec_input/moe2.rs`、`consensus/src/pipeline/virtual_processor/tests/tb1_holder_finality.rs` | 所有者と資金提供者が署名したEXEC closeから得た2件のslotを、各ピアが再計算した予算行と照合する。 |

coreの市場28件（決済4件を含む）、予算91件、MAS1 4件、QF1 2件はいずれも終了コード0。決済fixtureは1000の予約から800を使い、購入240・永久LP160・残余400を記録した。別slot、異なるcloseでのundo、残余を使う二重消費を拒否し、再起動と巻き戻し後の再適用で経済状態が一致した。

nativeテストビルドの終了コードは0。3プロセスの `private_moe2_three_process_registration`（28.27秒）、`private_registered_tb1_pending_freeze`（26.69秒）、`private_tb1_stable_panel`（4.67秒）はすべて終了コード0。pending-freezeの3プロセスが読んだcloseは `96a083bd031d519f49dc15d2a3e079baafeb8e81704b295f81fd4bf01b0a0c0a7067f346dc232b5356db9a53ae57f0ee09a36921967ae5cb3fac3090e882f087`。2件の予約を書き加えたdeltaのrootは全ピアで `2801cb936f648718a888783fdd31ce2fb6d2ad521c3f45e44e1868f3b89fead0489b10a3a22e451c6f9326b7d7e20079efcaf55573b981dd3cb91f0870c02530`。このrootはfixture内で計算した値で、選択鎖の受賞ブロックに記録された決済rootは未取得。

新profileでは、採掘適格なOriginalのTicket、完全再実行したPanelの承認、Gate C、同一実ブロックでの仕事キー消費と決済の呼び出しが残る。保存Studio claimは旧規則で0/21。新しいTFO3用に元の受付設定とSmolLM2の登録資料を別ディレクトリへ用意した。新規claimの登録・当選、Round 16固定close、native39本試験の完走はこの試験の対象外。

## QF1の非報酬用途とEXEC予算の予約

上記の予約差分は、QF1の評価用LocalInferenceで署名したslotを入力にしていた。価格・担保の試算には使えるが、評価用途の署名では採掘資金を拘束できない。private予約APIは、差分への書き込み前に見積もり付き全slotの用途を検査する。LocalInferenceと採掘profileのないTIR入力を拒否し、QF1 NewMiningには既存の採掘適格性判定を適用する。

nativeビルドは終了コード0。独立3プロセスの private_registered_tb1_pending_freeze は25.81秒で終了コード0。各ピアで署名済み2件の試算が成立し、予約要求は拒否された。拒否前後のclose差分は全バイト一致し、正規のpost-state rootが保存され、どちらの予約行も存在しなかった。private_moe2_three_process_registration（27.43秒）、private_tb1_stable_panel（4.58秒）、小規模な再起動・prefix再生の replay_to_original_panel_parent（0.21秒）も終了コード0。実採掘の予算予約、受賞、決済およびRound 16の実closeは、この結果からは確認できない。
## 隔離 P200 での採掘入口

保存 Round 16 の DAA 51258 台帳と、正規 TFO3 を DAA 200 で有効にした P200 は別 genesis。P200 では登録済み SmolLM2 の protected original を使える。通常 FP の `fpCertified=false` 単独から protected TFO3 の不適格を判定していた説明を訂正する。`palw_state_v2/objects.rs` は受理済み funded pairing の種別を読み、`palw_accept.rs` は original fence を検査する。

同じ登録・bond・プロンプトを持つ二つのチェックポイントで比較した。DAA 3507 は割当なし、epoch 4 のモデルの持分は 47,660,870,950 / 2,521,321,958,376、累積 10,358,913 block units。Studio の worker は 85,938 leaves を実行したが、native の P_m 予算判定で queue は refused となった。独立 funding UTXO から署名した初回割当を同期した別 DB のノードで処理し、DAA 3508 で sequence 1、10,000,000,000 sompi が pending に入った。

同じ系統で旧割当が成熟済みの DAA 7745 をコピーから復元した。新規に投入した実プロンプトは17入力・1出力。8K クラスの実行であり、入力長は17トークン。新しい job と claim を生成し、通常の quote が21 quanta、protected funding 193,119,700 sompi を返した。署名済み carrier を独立したテスト用 UTXO から送信し、再起動後も同じ署名済み bytes と同じ取引 ID を使用した。DAA 3508 の新割当をここで効力発生済みと扱っていない。

古い claim の Final にあった spentCount=0,total=21 は消費状態。Gate C 後の beacon で全21 Ticket のハッシュを再計算すると winningIndices は空で、最小値も期限内最大 target を超えた。1 claim に割り当てた21量子であり、21件の claim 受付を表す数ではない。元の target と独立一様の仮定で21枚とも落選する確率は99.9840%。

署名用 example のビルド、別 genesis と予約済み Panel float の拒否、2ピアでの割当 readback、成熟済み checkpoint の新規 Studio queue、publisher、full-node rail は終了コード0。単独ノードでの包含試験と、2ピアで子ブロックの確定前に打ち切った試験は終了コード1。24 GiB への拡張前の成熟済み3ピアでは、子の native 準備に既予約7,706,971,063 bytes と新規6,983,593,776 bytes を要し、上限13,172,799,892 bytes を超えた。採掘済みの子を再検証するため発行を止めた。成熟済み復元ノードは過去の Linux バイナリ、Studio、worker、gateway、rail の今回の入力・出力は [P200 記録](p200-studio-mining-path.json) に対応する。

新規 claim の当落には Panel の完全再実行、Final、Gate C、成熟後の beacon が要る。旧規則の claim に MOE2 は適用されない。新 profile は `private_moe2_settlement.rs` の予約・決済を受賞ブロック本体の fold に結合する必要がある。

24 GiB の VM で保持済みの子を通常の submitBlock に再提出し、独立3 DB が DAA 7746 で同一の新規 claim、carrier 取引、original、execution root を返した。全ピアで provisional、認証 pending、Panel 承認未記録。受付ブロックは 2 対 1 に分かれた。共通の子を生成・再提出しても、26 tip を保持した peer 1 では 723,108,006 bytes 予約中に 30,076,294,708 bytes を要求し、上限 17,603,214,538 bytes を超えて virtual 更新を保留した。他2ピアは子を選択した。3ピア一致、Panel、Final、Gate C、当選、受賞および決済は未実証。分岐前の各 DB に適用する実ブロック3個を取り出した。作業ボリュームが ENOSPC になり、隔離 VM を停止した。

## 新規Studio claimの現在地と新profileの前提

隔離P200の3ピアは新規Studio claimのPanel承認と異議予約2件の返上を受理した。最新の共通ブロックはDAA 7751の `1bf869a1…`、PALW state rootは `a7a4dddc…`。claimは `receipt_licensed`、`accusationPending=false`、通常のphase期限はDAA 10866、Gate C認証は `pending`。[選択鎖の返上取引と承認](p200-studio-mining-path.json)

旧規則で受け付けたこのclaimは、受付時のTFO3規則でFinal、Gate C、成熟後のTicketへ進む。新profileのMOE2へ移す経路は規則にない。新profileの実ブロック検証には、署名済みEXEC closeで採掘適格な入力、対応する公開済みseed、正規のFOW1 Ticket、選出席の完全再実行承認とGate Cが必要。現在のprivate入口は `qf1.rs` の `Qf1Use::NewMining` で `REVIEW_HOLD` を返す。`exec_input/moe2.rs` の `reserve_closed_moe2_budget` は非報酬QF1と旧TIRの入力を採掘予約に使用できない。このため、決済kernelを呼べる適格な新profileの受賞ブロックが現行入力から得られない。

`fe910813d` で、private MOE2市場の登録・価格更新にブロック単位の巻き戻し記録を追加した。3プロセスの登録試験では、native登録差分から記録を作成してエンコード・復元し、異なるブロック、未切り離しの状態、同じ記録の二重適用を拒否した。登録元へ巻き戻してから同じ差分を適用すると、元の市場状態に戻った。実ブロックの決済呼び出しは未接続。

| 現行試験 | 終了コード | 実行時間 |
|---|---:|---:|
| Rustテストバイナリのビルド | 0 | 13分06秒 |
| T12当選fixture。3個のDBで受賞ブロック・root・消費数・経済状態が一致 | 0 | 3.82秒 |
| T12落選fixture | 0 | 1.68秒 |
| 同じ量子のV3/V4二重消費 | 0 | 1.04秒 |
| MOE2の独立3プロセス登録・巻き戻し・再適用 | 0 | 1.11秒 |
| QF1の隔離3プロセスで署名済みEXEC closeを読み、非報酬入力の予算予約を拒否 | 0 | 40.53秒 |

当選fixtureは決定論的なseedを選んで正規の後続状態遷移を試す。新規Studio claimの当選結果は未取得。初回ビルドは実装編集中に停止し、終了コード130。QF1試験の初回はartifact指定が欠けて終了コード101。引き継ぎ済みartifact・tokenizerを指定し、上表の結果を取得した。

## 受賞ブロック本体の受理後に選択鎖の更新が保留された場合

`18b441eb8` は `kaspad/src/palw_producer.rs` のPFS3とPFS4で、受理済みまたは検証待ちの署名済みブロックをプロセス内に保持する。親が選択tipの間は、最短5秒間隔で同じブロックを通常の経路に再提出する。親の変更や無効状態を検出すると保持を終了する。合意判定は従来どおりで、プロセス再起動をまたぐ候補の保持は未実装。

`palw_producer::tests` は6件とも成功、終了コード0、試験時間0.12秒。回帰試験は親、候補の状態、再提出間隔、および再提出時のブロックhashを検査した。

同じコードでprivate Linux ARM64ノードを `private-integration-clock` 付きでビルドした。終了コード0、7分47秒。バイナリは `target/t12-current-arm64/aarch64-unknown-linux-gnu/debug/misaka-rfc12-private-node`、SHA-256は `4d1c8bda07dd08855ff354232e23d48bbe0142031e8d29bcc3e47c0f2dda698c`。このバイナリで新規claimの隔離3ピア試験は未実行。

過去のT-N1受賞ブロックからの分岐検証は、1ピアで分岐側の候補2個を受理したところから短時間試験の範囲を超え、SIGTERMで停止した。runnerの終了コードは1。選択鎖の切り替え、消費状態と報酬の巻き戻し、3ピアの一致は未確認。この試験は現在の隔離P200 claimのGate Cや当選結果を検証していない。

| 採掘経路 | 判定 | 隔離P200で確認した到達点と残る条件 |
|---|---|---|
| A 登録 | PASS | SmolLM2の8Kクラスを登録し、モデル予算の割当が成熟した状態を使用した。 |
| B 受付 | PASS | 隔離genesisのDAA 200受付fenceで、実推論から作った新規claimを3ピアが受理した。 |
| C 当選 | BLOCKED | 選出席の完全再実行とPanel承認、二件の異議予約返上を確認。DAA 7751時点で通常のphase期限10866は未到達で、Final、Gate C認証、確定した乱数でのTicket判定は未確認。 |
| D 受賞 | BLOCKED | 新規claimの当選権利が未確認。決定論的fixtureの消費・受賞と独立に、隔離ネットワークでの生成・選択鎖受理・支払いの一致が必要。 |
| 新profileのMOE2 | BLOCKED | 採掘適格な新profile入力と、受賞時の通常のブロック処理から決済を呼ぶ接続が未成立。 |

### 再起動後の受賞候補

受賞ブロック生成サービスは、再起動後および選択親・bondの変更時に、native DAGから親を選択親に持つ自分のPFS3/PFS4を探す。登録鍵とbond、header位置、署名を照合し、本体を持つ検証済みまたは検証待ちの既存候補を再提出に使う。候補がある間は同じ親で新規に署名しない。DBに残る本体を読み出すため、別の永続ファイルは追加していない。

`palw_producer::tests` は7件成功、終了コード0、試験時間0.21秒。回帰試験は実署名付きPFS3/PFS4を用いて、同一hashの復元と、異なる選択親・bond・署名者、headerのみ、署名した位置の改変の拒否を確認した。DBの読み出しはテスト用のConsensusApiを使用した。実ノード再起動・選択鎖切り替えによる再提出は未実証。

private Linux ARM64ノードの再ビルドは終了コード0、24.86秒。SHA-256は `ab934ea6213fce8947dae8ef4d4677307197d7a9b474d7968bc3d068666cacb3`。

過去のT-N1のnative RocksDBをコピーし、`recover_tn1_receipt_from_cold_native_db` を独立した2プロセスで順に実行した。各プロセスがgenesisを再処理せずにDBを開き、DAA 2725の受理済みPFS3 `7cb8b072…` を選択親 `3c1cd4ff…` から通常の再提出用lookupで復元した。両プロセスで通常のRPCブロック形式の全バイトがDB内の受理済み本体と一致し、その受賞の選択親ではないDB先端 `53b8c9fa…` からのlookupは空だった。終了コードはそれぞれ0、試験時間は0.11秒と0.09秒。初回はテスト内の参照型が一致せずビルド終了コード101で、参照を修正後に上記を取得した。

再現にはコピーのパスを `MISAKA_TN1_COLD_RECEIPT_DB_COPY`、対応するreceiptとtipを記録したJSONのパスを `MISAKA_TN1_COLD_RECEIPT_CONTEXT` に指定し、kaspadのテストビルドで `recover_tn1_receipt_from_cold_native_db -- --ignored` を実行する。検証対象は過去の受理済みブロックの本体復元。保留中の候補を実ノードが再提出する経路、3ピアの選択鎖切り替えと消費・報酬の巻き戻しは未検証。

### EXEC slotによるMOE2決済差分の取り消し

`revert_private_moe2_exec_slot` は、取り消し記録に結合したブロック位置と、native delta末尾の予約消費・市場更新を照合する。元の予算行、worker配分・永久LPを持つ決済台帳、市場状態を戻し、その決済差分だけを取り除く。選択親への通常の巻き戻しは、取り除いた後のnative deltaで行う。この接続点に渡す採掘権利は、FOW1 claimの正規受付、Panel承認、Gate C、当選、同一ブロックでの権利消費を確認した処理から取得する必要がある。その通常ブロック処理からの呼び出しは未接続。

`private_moe2_three_process_registration` は終了コード0（実行1.14秒）。3プロセスが共通のnativeブロック `3bc753c0…` の差分と試験用EXEC slotで決済を計算し、決済後root `74a710ae…`、決済台帳root `dcabe505…`、購入4,800,000,000 sompi、永久LP 3,200,000,000 sompi、余剰Position 1単位burn、Miner 6,400,000,000 sompi、Panel 1,600,000,000 sompiで一致した。エンコードした取り消し記録と再読込した台帳を使い、別ブロック・別close・欠けた差分・二重取り消しを拒否した。正しい記録では決済前root `91f1d8dd…` を復元し、再適用で決済後rootを再現した。これらは各プロセスの試験用差分に対する結果であり、選択済みブロックの通常の決済ではない。

テストビルドは終了コード0（2分51秒）。初回は指定featureが対象crateになく終了コード101。続くビルドはテスト内の一時参照が短く終了コード101。対象crateの `evm` featureでビルドし、テスト内の参照を修正して上記を取得した。

同じビルドで既存のT12当選・落選とPFS3/PFS4同一量子の二重消費拒否を再実行し、いずれも終了コード0を取得した。

### EXEC決済のnative receipt

隔離MOE2用のEXEC決済は、予約行の消費と市場更新と同じ差分に、claim、署名済みslotのdigest、モデル、Miner・Panelの配分、永久LP記録を持つ行を追加する。行はnative state rootに含まれ、決済位置と消費済み予約が一致する場合に差分の再生で復元できる。取り消しでは同じ差分から行を取り除く。空の履歴にはstate rootとsnapshotの追加データがない。追加した識別子は隔離profileの内部差分253とsnapshot tail `0xFC`。

ここで記録するworker配分は決済の会計結果。新profileの採掘受付と、当選・Gate Cを経た実ブロックの通常処理からの呼び出し、workerへの支払出力は未接続。

`private_moe2_three_process_registration` は終了コード0（試験1.14秒、最終ビルド7分12秒）。3プロセスが試験用EXEC決済を同じnativeブロック `47013bb1…` の差分に加え、native決済後root `5872ffeb…` と決済台帳root `873f88be…` で一致した。snapshotのnative preflight・復元を通し、受賞行を削ったsnapshotを元のrootで拒否した。通常の差分取り消しで行が消え、元の決済前rootと再適用後rootも一致した。

初回ビルドはnative資源見積もりの型一覧が未更新で終了コード101。最初のcodec生成は手書きsnapshot形式のreview pin更新が必要で終了コード1。追加tailを確認してpinを更新し、資源見積もりとwire scannerを再生成してから最終結果を取得した。

同じ最終ビルドで既存のT12当選、落選、PFS3/PFS4同一量子の二重消費拒否はすべて終了コード0。

### EXEC決済とcoinbaseのworker出力

隔離MOE2では、ブロック処理が認証したMiner・Panelのbondを、登録済み支払先から通常のML-DSA-87出力に変換できる。決済の入口で、同じブロックのcoinbaseから渡されたworker向けの区間を、金額とスクリプトを含めて照合する。重複Panel、合計の不一致、未登録bond、出力の欠落・入れ替えは予約消費の前に拒否する。原資消費、LP、burn、配分の既存の決済判定はその後に実行する。

通常のブロック処理には、FOW1当選とGate Cから認証済みの受取人を求め、coinbase全体の構築と照合でworker区間を確定させる接続が残る。

`private_moe2_three_process_registration` は終了コード0（ビルド7分22秒、試験1.13秒）。3プロセスは受理済みブロック `326c3df5…` のcoinbaseのコピーへ、試験用Miner 6,400,000,000 sompiとPanel 1,600,000,000 sompiの出力を加えた。トランザクションをシリアライズして読み戻し、実際に登録された支払先のスクリプトと金額を照合した。出力がない場合と二つを入れ替えた場合に、state root・決済台帳・差分が変わらないことを検査した。試験用coinbaseを照合した決済後root `5ef64caf…` は全プロセスで一致した。加工したcoinbaseのネットワーク送信や選択鎖への受理は実施していない。

同じビルドの既存T12当選・落選・PFS3/PFS4同一量子の二重消費拒否も、それぞれ終了コード0。

### Round 16の固定closeと公開ゲート

TB1のclose解決に、読み取り開始時の選択tipへの祖先関係と、commitment・選択親の照合後も同じtipにいることを追加した。公開ゲートの観測では、固定closeのfinalityをヘッダー読取り後にも再確認する。未選択のcloseを拒否し、読取り中にtipやfinalityが変わった場合は再試行する。

Round 16の固定値（close blue 181382、公開開始DAA 90741、期限DAA 256802）を使った短いfixtureで、正しいclose、誤ったblue・親commitment、公開開始と同じDAA、選択鎖から外れたclose、読取り中のtip移動と同一tipでのfinality撤回を検査した。remoteのTB1試験14件は終了コード0（4.59秒）。nativeテストビルドは終了コード0（2分59秒）。

短いRound 3の `private_tb1_stable_panel` は終了コード0（4.62秒）。3プロセスが同じclose `42f421c5…`、公開batch `2a39a1f2…`、certificate `dbc73387…` から同じseed `6e2a68b2…` を復元し、選出2席が再実行した。native finality撤回時のholder停止（2.32秒）とclose直前のprefix再開（0.24秒）も終了コード0。保存台帳のRound 16実close・実share公開・Panel最終承認は未実証。

### native39の完走

native39の隔離fixtureで、生成とヘッダー検証を同じノード内模擬時計に揃えた。分岐側と独立再生側にも受理するブロックの時刻を適用し、同じDBへの再起動では既存の時計引き継ぎを使用した。

`rfc02_native39_node_e2e_register_fund_final_gate_c_pnr9_coinbase_spend_reorg_restart` は終了コード0（試験555.21秒、ビルド2分50秒）。DAA 7626でjob・MasterのFinal、DAA 12669で7検証者のGate C認証と、4量子を持つreceiptの発行を確認。PNR9 `4082af34…` が受理され、coinbaseに812,500,001 sompiを計上した。Bの4ブロックの分岐へのreorgで、Aの報酬出力が消え、B側の報酬出力と元のjob creditのUTXO消費をブロック `1add9799…` が受理した。同じDBからの再起動でtip・state root・台帳行・UTXOを復元し、独立ノードの再生で選択tip・state root・台帳行と選択鎖の各delta rootが一致した。

`rfc0012_native39_fixture_clock_survives_same_db_restart` も終了コード0（0.16秒）。前回のSIGTERMによる中断は完走結果に含めない。今回の完走はnative39 fixtureの結果で、隔離P200の新規Studio受賞、Round 16の実close、新profileのMOE2同一ブロック決済とは別の試験。

### Gate C検証義務と同一DAAのreorg

Gate Cの検証義務を、選択sinkとvirtual DAAの組で管理する。コンセンサスの読み取り口は、指定されたsinkと同じ選択鎖のPALWスナップショットからだけ義務を返す。読み取り後、非同期再実行後、署名のキュー登録前に選択履歴を再確認する。同一DAAでも選択sinkが変われば読み直し、引き継がれなかった検証義務の判定・再実行結果・送信待ち署名を破棄する。受付済みTFO3の再実行入力は、既存のcarrier・原受付の照合を維持する。

`palw_cert_duty` は7件すべて終了コード0（試験0.14秒、ビルド5分13秒）。同一DAAで検証義務が置き換わった場合の旧判定・送信予定の破棄と、実バックエンドによる正しい根の一致・偽根の反証を検査した。初回ビルドは指定sinkをコンセンサスAPIで照合する修正を加えるため中断し、終了コード130。

隔離P200の実Studio claimはDAA 7746で受理済み。DAA 7751の3ピア共通状態ではPanel承認済みで、Finalの期限はDAA 10866。保存した旧台帳の再生およびP200の追加発行は実行していない。P200のFinal、独立したGate Cの75枠中41枠の認証、その認証後のbeaconとTicket、受賞ブロックの実消費・受理は未確認。MOE2は別profileの適格なFOW1入力と実ブロックへの決済接続を要する。

### 隔離P200の通常Attemptと、二つの乱数源

`palw_security_certification_fold_v1.rs` の `commit_at_final_v1` はFinal時点のseed spanを固定する。同ファイルの `sweep_security_certification_v1` は後続spanのseed anchorを要求し、`palw_state_v2.rs` の通常foldは受理されたAttemptの実行キーからこれを記録する。Gate C後のTicketは `palw_beacon_fact_of_candidate` が成熟slot以降の選択鎖のAttemptを読んで確定する。heartbeatの追加だけでは、いずれの条件も満たせない。

コミット `8ab8db36b` のLinux ARM64ノードを `private-integration-clock` 付きでビルドした。終了コード0、5分04秒。SHA-256は `a5c384b3a9e554587b69502dd90c1314a54818c5cc45d08de9c0149649f2e4ea`。DAA 7751の作業コピーから3ピアを起動し、同一の選択tipとPanel承認済みclaimを確認した。直近600ブロックのlane内訳はAttempt 12、heartbeat 588。独立した検証者選出や成熟後のbeaconは、この履歴からまだ確定していない。

短い試験では通常の `--palw-produce` で既存のBASE-0を使用した。bondは登録済みで通常の受付前判定に拒否理由なし、予算の使用は20/1,000。接続後に64回の通常drawを実行し、新規Attemptは0。85秒の観測期限で終了コード4。ログの1回のclass ticket確率は0.003582で、64回すべてが外れる確率は独立一様を仮定すると約79.5%。この抽選はStudio claimの受賞Ticketと別の規則であり、今回のStudio claimのTicket判定は未実施。試験用heartbeatの発行許可を与えず、DAAは7751のまま終了した。今回の新規Attempt受理、Gate Cの抽選、PALW受賞ブロックは未確認。

隔離P200の選択鎖で、通常Attemptが実際に受理されていたことを3ピアの `getBlock` と `getPalwClaims` で照合した。終了コード0。ブロック `9252197f4243fb34…` はalgo 6、受理DAA 7747、PALW root `5559564216b6ba01…`。各ピアの受理claim `a3cdfe8c80c9e39a6…`、受理ブロック、識別したBASE-0クラスが一致した。受理DAAはStudio claimのFinal期限より前のため、このAttemptは同claimのGate C抽選や成熟Ticketのbeaconを確定しない。

同じ作業コピーで登録済みのBASE-0 producerを2つ稼働させた短い試験は、合計233回の通常drawで新規Attempt 0、ネットワーク難易度段階の落選も0。300秒の観測期限で終了コード4。各drawのclass targetがログの0.003582で独立一様と仮定すると、233回すべてが落選する確率は43.3%。選択tipとDAA 7751は3ピアで一致した。試験中にheartbeatの発行許可は与えていない。

### MOE2決済記録と同一ブロックのEXEC消費

隔離MOE2のnative差分を適用・取り消しするとき、決済行が指定するブロックの中に、同じEXEC予約の未消費から消費済みへの遷移があることを検証する。消費処理と差分読戻しは同じ予約行の遷移規則を使用する。市場の変更を含む場合は決済行の市場と状態も一致させる。これにより、過去のブロックの消費済み予約を再利用して決済行だけを追加する差分を拒否する。

`private_moe2_three_process_registration` は終了コード0（ビルド17分32秒、試験1.18秒）。スナップショット復元後、同じ予約を参照する別ブロックの決済行を拒否。正しい差分では3プロセスの決済後root `e55094cc0516922f…` と決済台帳root `5491edb96b0183f6…` が一致し、取り消しと再適用で同じ状態を復元した。これは隔離fixtureの経済差分の検証。適格FOW1 claimの当選・仕事キー消費から通常ブロックの決済へ渡す経路は未接続。

同じテストバイナリのT12当選・3ピア受理（3.79秒）、落選（1.72秒）、V3/V4の同一量子の二重消費拒否（1.04秒）はそれぞれ終了コード0。

### 期限後のTB1 PANEL入力集合の再構築

隔離profileのPANEL入力集合を、署名済みEXEC登録、元の公開済みseed、収録済みのOriginalと所有者署名から再構築できるようにした。各Originalについて登録slotから実入力を再導出し、元の受付期間、EXEC certificate以後の選択鎖への収録、PANEL closeのblue cutoff、Ticketと署名を照合する。読戻し中のEXEC finalityも再確認する。通常の新規受付・再実行・署名には元の現在期限が適用される。

短い非報酬fixture `private_tb1_stable_panel` の最終実行は終了コード0（ビルド2分49秒、試験4.96秒）。3プロセスはEXEC公開前のDBとDAA 60の期限時点のDBをそれぞれ開き直し、Original `0b4e2217…` からPANEL close `c991e707…` と一致する集合を再構築した。承認はDAA 27に収録されていた。期限後の新規受付、slotの変更、別の収録ブロック、改変Ticketを拒否した。初回の期限後読戻しは終了コード0。EXEC観測の復元まで広げた試験の最初のビルドは、チェックポイントAPIを間違った型から呼び出してコンパイルに失敗した。runnerも終了コード取得用のシェル変数で停止し、終了コード1。双方を修正して上記の最終結果を取得した。

同じビルドの `native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected` は終了コード0（2.26秒）。今回の検証は非報酬の短いroundで、Round 16の実close、P200の受賞、およびFOW1のGate C・MOE2決済は未実証。

### MOE2予約とEXEC closeの結合

資金提供者の署名参照は登録キーとslotから従来どおり計算する。native台帳の予約IDは、同じ登録・slotに加えてEXEC closeのブロックhash・DAA・元のquoteを含む完全なslotから別domainで導出する。選択鎖のclose読取り、予算予約、決済、取り消しは同じ予約IDを使う。割当台帳は引き続き署名参照で一回性を管理する。元の署名参照を共有していても、同一DAAの別closeまたは改変quoteは元の予約に一致しない。

`private_moe2_three_process_registration` は終了コード0（試験1.12秒）。closeのhashだけが異なるslotとquoteだけが異なるslotは、同じ署名参照を持っていても元の予算行を使った入場を拒否し、決済台帳を変更しなかった。正常なslotでは予約を一度消費し、巻き戻し・再適用後のroot `9b7243789c22e137…` と決済台帳root `2f75e5d8a9d3d8aa…` が3プロセスで一致した。

登録済みQF1の `private_registered_tb1_pending_freeze` は終了コード0（40.35秒）。3プロセスが同じclose `773a9f9815b67150…`、署名参照 `e0b0fd82bc813933…`、native予約ID `e8011878c6e43d97…` を得た。非報酬QF1での採掘予算予約は拒否され、通常ブロックの差分とrootが変わらないことを確認した。最終ビルドは終了コード0（2分49秒）。同じバイナリのT12当選・3ピア受理、落選、V3/V4同一量子の二重消費拒否も終了コード0。FOW1の適格な実当選と受賞ブロック決済は引き続き未確認。

### MOE2決済と候補ブロック本体の照合

EXEC決済入口は、候補ヘッダーから再計算したブロックhashとDAA・blueをnative差分の位置と照合する。ヘッダーがコミットしたトランザクションのMerkle rootと先頭のcoinbaseを検証し、認証済みの受取人計画に基づくworker出力区間が一致した場合に限り予約を消費する。出力の欠落・順序違い、区間指定のオーバーフロー、同じヘッダーへの別のブロック本体の付け替え、別ブロックの使用は状態の変更前に拒否する。

`private_moe2_three_process_registration` のビルドは終了コード0（7分02秒）、試験も終了コード0（1.15秒）。3プロセスは通常のheartbeatを元に同じ試験用ブロック `6e3da423652ea3a8…` を組み立て、予約済み親からその候補用のnative差分を導出した。決済後root `36c5b94c0047423bf…`、決済台帳root `dcdfb8619e35d36d…`、EXEC予約消費、市場購入4,800,000,000 sompi、永久LP 3,200,000,000 sompi、余剰Position 1単位burn、Miner 6,400,000,000 sompi、Panel 1,600,000,000 sompiが一致した。候補は試験用で、FOW1受賞として選択鎖には提出していない。通常ブロック処理には、認証済みのFOW1当選権利を消費し、coinbaseの区間と受取人を確定して決済を呼ぶ接続が残る。

同じテストバイナリの既存T12当選と3ピア受理（4.05秒）、落選（1.96秒）、V3/V4同一量子の二重消費拒否（1.07秒）は終了コード0。いずれも隔離P200の新規Studio claimの通常Ticket結果には含めない。

### 隔離P200のGate C検証者と手数料の起動準備

DAA 7751の作業コピーを、ブロック生成を有効にせず3ピアで起動した。共通tip `1bf869a123fd4623…` に対する `getPalwStateProof` のbond集合とstate preimageは一致した。8件のbondが有効で、登録DAAはすべて0、operator IDはすべて異なった。claimの実行者である席0を除いた7席が、現時点の成熟・担保・operator条件を満たした。実抽選の集合はFinal時点の条件で決まる。3ピアのbond集合と申告状態の読取りは終了コード0。最初の起動はVM再起動で一時ディレクトリからバイナリ照合用hashファイルが消えており終了コード1。既存hashを復元してから読み取った。

検証者7席が1ピアに集中していたため、隔離環境専用の `/srv/misaka-private/t12-new-claim/run-gate-c.json` で席0の実行者をpeer 1に維持し、peer 0に席2・3・4、peer 1に追加席5・6、peer 2に席1・7を割り当てた。組み替え後の起動試験は終了コード0。各ピアの追加サービスの起動と、3ピアのStudio class artifactの読込みを確認した。Final後の抽選結果に対応する各席のサービスを起動した。Gate Cの実再実行とattestationの提出は未確認。

従来の設定に入っていた8件の初期手数料outpointは、3ピアとも未消費集合に存在しなかった。既存の各署名者のアドレスを3ピアで照合し、有効な通常出力から8件の相異なるoutpointを選んだ。確認は終了コード0。検証者用7件の最小額は9,672,076,366 sompi。新起動設定はこれらを直接指定する。設定SHA-256は `03a622fdce01295e67ad3a23c159dd42057db283c60b85dd5dbd7bf7d9261103`。保存台帳・合意ルール・既存の隔離genesis設定には手を入れていない。

新しい手数料outpointで再起動した `gate-c-preflight.py` は終了コード0。3ピアから同じ通常UTXOの集合を読み、設定した8件すべてが未消費であることと、選択tipが動いていないことを確認した。起動スクリプトとバイナリ検証用hashは設定と同じゲスト内の永続ディレクトリに保存した。DAAは7751、claimは `receipt_licensed` のまま。Finalの期限10866と後続Attemptによる実抽選が残る。

### 選択鎖が変わった場合のP200受賞決済の読戻し

隔離ドリルは、監査済みPFS3ブロックが選択鎖から外れた場合、古い報酬を再利用せず、選択鎖への復帰を待つ。同じPFS3ブロックの報酬が別の後続ブロックで受理された場合は、入力の報酬outpointと新しい受理ブロックを全ピアで照合する。新しい報酬チェックポイントを保存する前に、同じ選択tipで同じclaimのFinal・Gate C・量子の一回消費を再確認する。監査済みPFS3の入力記録は保持し、報酬チェックポイントだけを原子的に更新する。

`test_protected*.py` は83件、終了コード0（1.83秒）。RPCを制御した短い試験で、PFS3離脱中の待機、同じPFS3の報酬受理ブロック変更、更新後の読戻し、消費未確認時の上書き拒否を検査した。既存native fixtureのT12当選・3ピア受理（3.79秒）とV3/V4同一量子の二重消費拒否（1.10秒）も終了コード0。別のPFS3ブロックが同じ量子を消費するreorgへの追従、P200実claimの当選、実ピアでのreorgと受賞報酬の復元、およびMOE2同一実ブロック決済は未検証。

### 別のPFS3が同じ量子を消費した場合の監査再開

通常稼働時と `--resume-settlement` の両方で、旧PFS3が選択鎖から外れたら、所有producerのログから同じclaim・量子を消費した選択済み候補を探す。候補には初回と同じnativeヘッダー・署名・量子の監査、承認済みPanel・Gate C・受理の全ピア照合を行う。新しい監査を保存するまで決済チェックポイントは旧記録を指し続ける。切り替え後は以前の報酬を引き継がず、新しい受理ブロックと報酬を読み直す。復帰前後の監査記録は別ファイルに保存する。この経路は所有producerのログに候補がある場合を対象とする。

`test_protected*.py` は84件、終了コード0（1.63秒）。制御したRPC応答で、再起動後に離脱した旧PFS3と別量子の候補を除外し、同じ量子の新PFS3を監査した。監査拒否では旧チェックポイントを保持し、成功時だけ新しい報酬へ進んだ。最初の試験はテストのRPC stub不足で終了コード1。stubを修正して最終結果を取得した。

別profileのnative `rfc9_v4_chain_e2e_the_offline_miner_is_paid_by_another_builders_block` は終了コード0（20.38秒）。経済的に劣る長い分岐を拒否し、適格な分岐で消費を巻き戻し、量子5を2候補が争った場合に片方だけに16,004,232,504 sompiを計上した。このnative試験はPFS4のコンセンサスを検証する。P200のPFS3実claimによる当選、実ピアのreorg、MOE2実ブロック決済は引き続き未確認。

### MOE2の同一ブロック市場遷移を再計算

EXEC予約の消費を伴うnative差分では、市場変更がある場合、決済行の直前に同じモデルの市場更新とEXEC予約消費が並ぶことを要求する。市場の更新前の値から既存のMOE2式で買付・永久LP・余剰Position burn・残余報酬・更新後残高を再計算し、記録値と照合する。永久LP履歴の更新前残高も同じ市場更新前の値と一致させる。差分適用と巻き戻しは同じ検査を通る。スナップショット復元でもworker配分、即時支払上限、未払い時のvesting、永久LP記録の整合性を確認する。署名済みquoteの上限・期限・価格条件の照合は書き込み時の既存処理で行う。

この変更は既存の隔離決済差分とsnapshotの書式を維持する。通常ブロック処理で認証済みFOW1仕事キーを消費して決済を呼ぶ接続と、採掘資格を満たすFOW1入力による実ブロック試験は残る。

`private_moe2_three_process_registration` は終了コード0（ビルド7分01秒、試験1.15秒）。市場の更新後残高を維持したまま買付Position数とLP履歴の同じ数を改変した差分を、適用・巻き戻しの両方向で拒否した。Miner即時支払を配分より1 sompi増やしたsnapshotは、期待rootの比較を指定しない状態整合性検査で拒否した。正しい候補 `a2bced207d4d3b20…` では、3プロセスの決済後root `5b253baaf6a3aa2b…`、決済台帳root `9fe82ed97dc689e88…`、購入4,800,000,000 sompi、永久LP 3,200,000,000 sompi、余剰Position 1単位burnが一致した。同じビルドのT12当選と3ピア受理（3.96秒）、落選（1.90秒）、V3/V4同一量子の二重消費拒否（1.14秒）はすべて終了コード0。

### 隔離P200の短い進行試験

DAA 7751の隔離作業DB3個にZFS snapshot `p200-before-bounded-forward` を残してから、検証者・手数料設定を読み込んだ3ピアで通常のalgo 8 heartbeatを正規PoW検証に通した。最初の子 `383f5622a2b160c7…` を全ピアが選択した時点ではvirtual DAAが7751のままであり、全heartbeatが即座にDAAを1進めるとした観測条件が終了コード1となった。その条件を修正して同じ作業コピーから進めた。

後続試験は選択tipを `20834c6dc69efc9d…`、virtual DAAを7753まで進めた直後に履歴proofの照合で終了コード1。採取時のノードログでは、選択親の状態再構築に約17.0 GB必要なところ、約18.2 GBが予約済みで約20.6 GBの論理上限に達していた。発行を止めて再起動した3ピアの読戻しは終了コード0。同じ選択tip、ヘッダーの親状態root、親状態proofのpreimage、選択tipのpost-state root `ff74dfb10930acae…` が一致した。

最後の短い試験の発行中は、全ピアの選択tipを揃えて現在のnative post-state rootを照合した。終了コード0。3ピアがalgo 8ブロック `c07eaca5b8b7a751…`、virtual DAA 7754、post-state root `125cdac1ab342017…` を返した。Panel承認済みの同じStudio claimは `receipt_licensed`、異議保留なし、Final期限10866、Gate Cはpending。新規Attempt、Final、当選Ticket、受賞権利消費は今回の試験では確認していない。作業コピーをこの位置で停止した。完全な識別子と終了コードは [P200実行記録](p200-studio-mining-path.json) に収録した。

### 履歴proofで保持済み親状態を再利用

`getPalwStateProof` が対象ブロックの選択親を知った後、テンプレート処理がすでに保持している状態の親ブロック・root・pruning point・import世代を照合する。一致した場合は状態を借り、native状態の複製と履歴差分の巻き戻しを省く。キャッシュが空、別の親、別世代、または取得中の場合は従来の履歴読取りを使う。proofを生成する際はどちらもヘッダーのcommitmentと照合する。履歴proofの問い合わせはテンプレート用キャッシュの中身を入れ替えない。

`template_parent_snapshot` は12件、終了コード0（試験0.95秒、ビルド2分49秒）。追加試験では通常の履歴歩行を利用できなくした後、照合済みのキャッシュから同じ親のproofが生成され、親の保存済みroot参照を取り除くと拒否されることを確認した。初回は試験コードで設定値の所有権を移動してビルド終了コード101。クローンを渡すように修正して再実行した。同じバイナリのround observer試験5件（0.57秒）とT12当選・3ピア受理fixture（3.77秒）も終了コード0。

DAA 7754の隔離P200作業コピーには今回のバイナリを適用していない。キャッシュにない履歴読取りの容量制限、実claimのFinal・Gate C・当選、MOE2同一実ブロック決済の検証は残る。

### 原期限後の隔離QF1 Court裁定の読戻し

署名済みEXEC登録と公開済みseedから、期限後でも元の入力を再構成し、収録済みOriginalの署名と選択鎖への包含を照合する。StopおよびReadout/Endの局所反証は元の証拠から再裁定し、caseとverdictそれぞれが原期限内に別々のブロックへ収録され、同一選択鎖上で正しい順に並ぶことを要求する。新規caseやverdictの発行には現行の受付期限を適用する。この読戻しは隔離の非報酬QF1経路で、採掘資格・公開fence・Gate Cの条件は従来どおり。

ビルドは終了コード0（2分59秒）。`private_tb1_stable_panel` の正常trace（4.74秒）と不正Readout（4.66秒）はともに終了コード0。3プロセスが原期限のDBからEXEC入力を再構成し、それぞれ同じ `ChallengeNotEstablished`（verdict `0805d9e222dce73b…`）または `ComputationRefuted`（verdict `3609211ac4b020fc…`）を読み戻した。期限到達後の新規case作成、verdict発行、期限境界のブロックにコピーしたverdictを拒否した。既存のnative finality撤回試験も終了コード0（2.30秒）。

実Qwen3-8Bの資料不足試験は終了コード0（262.14秒）。選出2席のうち1席だけが資料を取得でき、3プロセスは1署名の状態から元期限に未検証で終了し、同じ終端記録をDBの再起動後に復元した。初回は親artifactと登録済みscale sectionの指定漏れで終了コード101。両方を引継ぎ済みファイルに固定して再実行した。この試験の終端は非報酬の隔離Panelに限る。通常ブロック処理のFOW1成功Final・Gate C・MOE2接続、およびP200の実claimの当選は引き続き未確認。

### Native決済履歴と署名済みEXEC slotの一回性

決済差分を適用・巻き戻しする際、決済行が持つ署名済みslotのdigestから元の予約IDを再導出し、同じブロックで実際に消費した予約と比較する。スナップショット復元でも同じ照合を行い、同一予約を別claim IDで重複記録した履歴を拒否する。既存の予約IDと保存形式はそのまま使用する。

この検査は元の署名済みslotの予約消費を対象とする。FOW1の認証済み実入力から得るcanonical `ComputationalWorkKey`、旧・新profileのaliasを含む一回消費、これを通常ブロック処理からMOE2決済に渡す接続は未実装。

`private_moe2_three_process_registration` は終了コード0（ビルド7分09秒、試験1.11秒）。予約消費を保持したままslot digestだけを変更した差分を、適用・巻き戻しの双方で拒否した。同じ予約を別claimと対応するLP行で複製したsnapshotは、期待rootを与えない状態整合性検査でも拒否した。正しい隔離候補の適用・復元では3プロセスの候補hash `4b10b64659a18de0…`、決済後root `b408f4a9fb3514ac…`、決済台帳root `803080942440cfc5…` が一致した。既存T12の当選・3ピア受理（3.98秒）、落選（1.89秒）、V3/V4同一量子の二重消費拒否（1.07秒）も終了コード0。

### 認証済みQF1入力の数値実行キー

隔離QF1の数値実行キーはcanonical TIR program、実weightの通常inventory root、順序付きReadout参照、実際のtoken ID列、署名済み生成上限から導出する。分割保存されたweightでも、通常inventoryを導出してキーに結合する。同じ数値実行を指す限り、登録manifest、資料の包装、claim ID、役割、署名値、資金参照は直接キーに入らない。TB1入口では、先に選択鎖上のEXEC証明と登録済みinstance descriptor・class・tokenizerを照合し、元の受付期間を確認する。

現行の採掘適格性判定と `REVIEW_HOLD` を適用したまま、非報酬QF1でキーを検証する。通常ブロック処理における同じキーの原子的消費、旧profileとのalias対応、認証済み当選からMOE2への接続は残る。

`private_tb1_stable_panel` は終了コード0（ビルド3分25秒、試験4.67秒）。3つの独立したプロセスが同じ実入力から `5e10763204ee06bb…` を導出した。fixtureのReadout scale参照メタデータを変えると登録照合では拒否し、同じ実weight・実入力の数値キーは一致した。token IDまたは生成上限を変えた場合は異なるキーになった。実登録済みQF1の `private_registered_tb1_pending_freeze` は終了コード0（28.08秒）で、非報酬の見積もりと採掘予約の拒否を確認した。隔離MOE2の3プロセス決済fixtureも終了コード0（1.19秒）。full fileと分割weightを使ったキーの直接比較、P200の実当選と受賞決済は今回の試験範囲に含めていない。

### 隔離MOE2決済での数値実行キー消費

隔離MOE2の決済処理に、受賞側が導出したcanonical work keyを受け取る入口を追加した。決済書き込みはキーが未消費のときだけ進み、署名済みEXEC予約の消費、市場更新、決済行、work keyとclaimの対応を一つのnative差分に記録する。差分適用は別claimによる同じキーの消費を拒否し、巻き戻しは決済とキーをまとめて戻す。スナップショットの状態rootも対応表を拘束する。キーのない既存決済は既存の差分・スナップショット形式で読み戻せる。

試験用の決済候補にはfixtureがcanonical keyを渡す。通常ブロックでのFOW1 SuccessFinal・Gate C・署名済み当選・受賞権利の消費をこの決済へ渡す接続と、意味の一致を認証した旧採掘profileの移行記録は引き続き未実装。P200のStudio/TFO3実当選と受賞ブロック受理は未確認。今回の試験中に長時間のDAA再生は行っていない。

`private_moe2_three_process_registration` は終了コード0（ビルド4分17秒、親試験1.82秒）。3プロセスで候補 `b052649ddb1e3db8…`、決済後root `3655b21837e4f0df…`、決済台帳root `f1469b3098b40fc2…` が一致した。別claim・別資金枠から同じキーを提示すると、決済台帳・差分・状態の変更なしに拒否した。巻き戻し後の兄弟候補では同じキーを消費できた。旧形式の読戻し、新形式のjournalとsnapshotの読戻し、期待root付きsnapshotでのキー欠落、journalからのキー除去、決済に対応しないwork記録と同一決済への二重対応も検査した。

実Qwen3-8Bの `private_qf1_work_key_matches_composite_and_portable_artifact` は終了コード0（56.47秒）。分割版と単一ファイル版のclass IDは異なり、同じ数値入力からキー `3c2875081bb9135a…` を得た。生成上限を変えるとキーも変わった。同じビルドの `private_tb1_stable_panel` は終了コード0（7.56秒）で、3プロセスが署名済みEXEC入力から同じキーを導出した。

`private_registered_tb1_pending_freeze` は終了コード0（56.87秒）で、実登録済みQF1を使って採掘予約の拒否を再確認した。既存T12の当選・再起動後の3ピア受理（17.88秒）、落選（7.52秒）、V3/V4の同一量子二重消費拒否（1.17秒）と、保護プロファイルのPython試験84件（1.59秒）も終了コード0。

同じビルドのPFS4受賞・reorg fixture `rfc9_v4_chain_e2e_the_offline_miner_is_paid_by_another_builders_block` は終了コード0（85.14秒）。実装差分の文書参照を検査し、既存の参照欠落による終了コード1と出力が変更前後で一致した。

### 候補ブロックの親状態で認証するEXEC予約

候補のブロック処理用に、署名済みEXEC登録を候補の選択親の履歴に対して検証する入口を追加した。準備ブロックがコミットした親状態を直接照合し、その状態から資金bond・登録classを引いてquoteへの同意を検証する。現在の選択tipとRPC用の履歴proof生成に依存しない。closeでは候補のコミットメント・元の受付時刻・選択親の状態rootと差分の位置を照合してから採掘資格を検査する。歴史照会とholderの公開許可は従来の選択鎖検査を通る。

通常ブロックで署名済み集合を読み出して適用する処理は残る。QF1は採掘保留で、P200の実受賞とRound 16の実closeは未確認。

ビルドは終了コード0（1分00秒）。`private_registered_tb1_pending_freeze` の3プロセス試験（57.03秒）は、選択tipへの問い合わせを失敗させ、RPC proofを提供しない履歴から署名済み集合を再構成した。従来の照会経路の集合rootと一致した。誤った準備状態、準備を含まない候補親、誤ったclose親状態・差分位置を拒否した。正しい候補ではローカル推論の採掘予約を拒否し、差分が元のバイト列と一致した。同じビルドの隔離Panel（7.57秒）、finality撤回（6.86秒）、MOE2決済と巻き戻し（1.64秒）、既存T12の当選・再起動後の3ピア受理（19.70秒）も終了コード0。

### 受賞候補の親予約からのMOE2決済

隔離MOE2の受賞用処理に、受賞ヘッダーがコミットした選択親と通常差分の親位置を照合し、親に残る署名済みEXEC slotの予算行から受賞時に台帳登録と決済を行う入口を追加した。市場の参照価格はブロック所有側が認証した元のquote anchor状態から読む。受賞ブロック内での予算窓の解放を許容し、それ以外の予約行の変更は拒否する。決済拒否時は仮登録を戻し、受賞の切り離し時は決済・仕事キー・受賞時登録を巻き戻す。

当選claim・仕事キー・受取人の認証結果はブロック所有側から受け取る。通常ブロック上のFOW1受賞権利をこの入口に渡す処理と、Round 16・P200の実運転検証は残る。

`private_moe2_three_process_registration` は終了コード0（ビルド4分10秒、親試験1.71秒）。候補 `a17dea0da0e4a61c…`、決済後root `963894e792fa65b5…`、台帳root `b9b5e2e6d2111283…` が3プロセスで一致した。親にない予約を受賞ブロック内で作った候補、既存予約の再利用時刻を変えた候補、異なる親・価格参照状態、署名済み報酬上限の超過を全て拒否した。決済済み台帳を復元した後も、切り離しで台帳登録まで消え、同じキーを兄弟候補で決済できた。

同じビルドの実登録済みQF1の採掘保留（57.04秒）、T12の当選・再起動後の3ピア受理（18.14秒）、V3/V4の同一量子二重消費拒否（1.17秒）の終了コードは全て0。保護プロファイルの運用スクリプトの84件も終了コード0（1.72秒）。

### Round 16の回復履歴を区間ごとに保存・再開

`resume_round16_recovery_work` に、隔離DBの通常受理を指定blueで区切り、DBと再開位置を保存する経路を追加した。再開時に保存先端のhash・blue・DAA・状態root、元の保存先端からの祖先関係と、アーカイブ上の一致する位置を照合する。公開開始DAAの境界を区間再生にも適用し、pruning pointが変わると通常の捕捉完了を待つ。再開位置ファイルはDBの保存後に公開する。

元台帳blue 102520から、2回の独立起動で1ブロックずつ再生した。blue 102521の保存（96.82秒）、そのDBからblue 102522への再開・保存（95.18秒）、最終DBの読み取り専用照合（1.21秒）は全て終了コード0。最終tip `07d42def9040cf1f…`、DAA 51259、状態root `33318f4cab59ba2e…` は既存の連続再生と一致した。両チェックポイントの状態rootはアーカイブの各後続ヘッダーの親rootとも一致し、元台帳118ファイルのチェックサムも照合した。

再開可能な最終コピーは `~/.local/share/misaka-t12-handoff/2026-11-22/round16-bounded.aW10gy/through-102522`。同じディレクトリ階層に再生計画、実行ログとDBのSHA-256一覧を保存した。区間再生は `MISAKA_PRIVATE_ARITHMETIC_RECOVERY_CHECKPOINT_READ` に前の保存先、`MISAKA_PRIVATE_ARITHMETIC_RECOVERY_STOP_BLUE` に停止blue、`MISAKA_PRIVATE_ARITHMETIC_RECOVERY_CHECKPOINT_WRITE` に新しい保存先を指定する。

固定日程前の短い再開、公開開始境界の拒否、Gate Cの共謀拒否、既存T12受賞の再起動後照合も終了コード0。成熟work用の分岐親blue 106200、実close blue 181382、実Panel承認、P200受賞は残る。

### MOE2の数値実行キー付き決済を再生する際の親予約照合

決済の書き込み、差分の適用・巻き戻し、差分の途中までの適用で、ルールが要求する選択親のEXEC予約を照合する。差分の同じ予約行への最初の書き込みから親の値を取り、決済直前の値と比較する。受賞ブロックでの予算窓の解放を許容し、同じブロック内での予約作成や資金条件の変更は拒否する。キーのない旧差分は従来の形式と検査で読み戻す。

`private_moe2_three_process_registration` は終了コード0（1.84秒）。親にない予約と、予算窓の再利用時刻を改変した予約を、決済書き込みと差分の適用・巻き戻しで拒否した。両ケースのキーなし差分は適用でき、キー付きに置き換えた差分は親予約照合で拒否した。正しい候補 `52a3ae48d21eb732…` の適用後root `82c3b58d589ad71e…` は3プロセスで一致した。初回の試験は改変後の解放キューが不整合で、差分を検査する前にスナップショットの復元処理で終了コード101となった。解放キューも整合させたfixtureで再実行した。

実登録済みQF1の採掘予約拒否（58.26秒）、既存T12の受賞と再起動後の3ピア受理（18.06秒）、V3/V4同一量子の二重消費拒否（1.20秒）、保護プロファイルのPython試験84件（1.62秒）は全て終了コード0。通常ブロックでのFOW1 Gate C・受賞権利消費・MOE2決済の接続、旧profileとの仕事キー対応、Round 16の実closeとPanel承認、P200の実当選と受賞ブロック受理は残る。長時間のDAA再生は行っていない。

### モデル本体を持たない検証側のQF1数値実行キー導出

QF1のReadout参照、登録descriptor、数値実行キーの共通処理を `consensus-core` に置いた。検証側は認証済みのTIRプログラム、layout、通常inventory root、Readout末尾参照を使い、実入力token列と生成上限を同じ形式でハッシュできる。ローカル実行器は保有する実weightから通常inventory rootを得て、同じ処理を呼ぶ。分割weightのsection rootと通常inventory rootの対応を通常ブロックに記録する処理は今後の接続に含まれる。

実登録済みQwen3-8Bの3プロセス試験は終了コード0（57.27秒）。固定した入力について、選択鎖の登録行からTIRプログラムと通常inventory rootを読み、layoutを登録済みdigestに照合してキー `3c2875081bb9135a…` を導出した。実weightを使う実行器と3プロセスすべてで一致した。無効な末尾Readout参照は拒否し、別のinventory rootではキーが変わった。分割weightと単一ファイルの比較も終了コード0（64.84秒）で、変更前と同じキーを確認した。初回ビルドは実行器の `VOCAB` import漏れで終了コード101。修正後のビルドは終了コード0（2分22秒）。

隔離MOE2決済（1.77秒）、既存T12当選と再起動後の3ピア受理（19.95秒）、旧V3/V4の同一量子二重消費拒否（1.36秒）、Round 16の短期fixtureの再起動後の予定親照合（0.40秒）と公開開始境界拒否（0.36秒）は全て終了コード0。Round 16の保存DBを読み取り専用で開き、blue 102522、DAA 51259、root `33318f4cab59ba2e…` を再確認した（1.30秒）。長時間のDAA再生は行っていない。

署名済みTB1 EXEC入力を使う `private_tb1_stable_panel` は終了コード0（7.53秒）。3プロセスでキー `bd7bdfeb1b466004…` が一致し、登録descriptorの差し替え拒否と、token列・生成上限の変更によるキーの変化を確認した。文書チェッカーは既存の参照欠落と同じ出力で終了コード1。

### MOE2 EXEC予約の期限と予算窓

署名済みEXEC closeの新規予算行に、署名済みquoteの最終DAAの翌DAAを記録する。通常のブロック予算処理が期限を処理し、決済の有無にかかわらずopen slotと、署名済み期限までの資金拘束を解く。予算窓とモデル側の配分消費は従来の予算規則で扱う。決済済みclaimと仕事キーの記録は保持する。決済時は予算行に固定した期限と署名済み決済期限を照合し、期限後の決済差分を拒否する。旧形式の `Attempt` 行は従来の読み戻し経路を使う。

`private_moe2_three_process_registration` は終了コード0（1.84秒）。予算窓の終了と同時、予算窓の終了後、予算窓の終了前の3通りを確認した。元の最終DAAでは決済済み・未決済の両予約が残り、翌DAAの通常foldでcloseした。決済済みclaimとキーを保持し、期限処理後の状態をスナップショットから戻して、差分の巻き戻しと再適用でそれぞれ同じrootを得た。受賞側が署名済み期限を延ばした場合は状態を変更せず拒否した。3プロセスの受賞候補 `e5d5179a2ff8cd26…` と決済後root `080a7ee14723352f…` が一致した。初回のテストビルドでは期限境界fixtureの `subsidy` が未指定で終了コード101。直前の値を継承して再ビルドした。

native wireの全量再生成は、既存 `PalwStateCarriageV2` のレビューpin不一致で停止した。隔離コピーで既存のpinだけを合わせて再生成し、今回の予算行のメモリ計測とwire scannerの変更が、生成結果と一致することを確認した。既存のRound 16保存DBは再生なしで読み戻し、blue 102522、DAA 51259、state root `33318f4cab59ba2e…` を確認した（1.33秒）。

実登録済みQF1の採掘予約拒否（59.83秒）、Gate Cの共謀拒否（43.56秒）、既存T12の受賞と再起動後の3ピア受理（21.08秒）、V3/V4の同一量子二重消費拒否（1.31秒）、保護プロファイルのPython試験84件（1.61秒）は全て終了コード0。通常ブロックで署名済みEXECを取り込み、認証済み当選権利とキーを消費して同じブロックでMOE2決済する接続、旧profileとのキー対応、Round 16の実close・Panel承認、P200の実当選と受賞ブロック受理は残る。

### 検証済みEXEC closeを通常foldで予約

隔離TB1のclose検証から、検証済みslot列、候補ブロックの位置、選択親と親状態rootをnative foldへ渡せるようにした。foldは隔離fenceと親・候補の一致、slotの順序を検証し、候補ブロック内の先行処理を反映した予算から順に予約を取る。途中で拒否された場合は状態も差分も返さない。受理された予約には前節の署名期限が記録され、同じ予算差分から再適用・巻き戻しできる。ブロック所有側で署名済み集合を取得してこの入力をセットする経路は残る。

`private_moe2_three_process_registration` は終了コード0（1.85秒）。fixtureで解決済みslot列を与え、従来の手動差分追加を外して通常foldで予約した。fenceなし、親rootの改変、逆順、後続slotの予算不足を拒否した。予約を巻き戻して親rootに戻し、差分の再適用で決済の親状態を再現した。受賞候補 `829cf950c4a925c8…` と決済後root `be41df4c2a8d1da2…` は3プロセスで一致した。最初の2回のビルドではモジュール参照とエラー文字列の引数型を修正し、最終ビルドは終了コード0（1分13秒）。

前節で残った `PalwStateCarriageV2` のレビューpinと既存の仕事キー行のwire scannerを修正した。native inventoryとwireの再生成は、隔離コピーで2回連続して同じファイルを生成し、変更内容と一致した。保護プロファイルのPython試験84件と、Round 16保存DBの読み戻しは終了コード0。長時間のDAA再生は行っていない。

実登録済みQF1の候補親・close位置の照合と採掘予約拒否（57.75秒）、既存T12の受賞と再起動後の3ピア受理（19.17秒）、V3/V4の同一量子二重消費拒否（1.30秒）も終了コード0。`check_rfc06_native_wire_idempotent.py` は作業ツリーでも終了コード0。文書チェッカーは既存の参照欠落と同じ出力で終了コード1。
