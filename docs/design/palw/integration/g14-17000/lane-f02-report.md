# Lane F02 (P0-1 canonical opportunity) — report, 2026-10-26

Branch `claude/g14-f02` from `55510b56a`. 計算の正しさの暗号学的証明は使わない。取得不能を計算誤りの有罪にしない。モデル二階層・claim 単位の抽選承認は不変。wire tag・既存 ID・DB schema の意味は変えていない（新しい field は transition extras だけで、永続化されない）。

## 要件と現状（台帳 P0-1 残件との対応）

| 残件 | 実装の所在 | 実 caller | 状態 |
|---|---|---|---|
| original から全 duty/packet を導出 | `court_service_packets_v1`（Panel=1 通常 original: signer×turn）、`derive_funded_admission_v1`（funded: 全 branch + retention packet） | `install_court_service_binding_v1`（`palw_tir_public_fold_v1` original 受付）、`palw_funded_original_coordinator_fold_v1::install_original_admission_v1` | 既存。通常 original は profile 未 pin で休眠 |
| 原子的な paid 受付 | `ServiceReservations::reserve`（all-or-nothing）+ 受付 block の TransitionBuilder（失敗で block 全体を拒否） | 同上 | 既存（funded）。通常経路は profile 待ち |
| 受付 block での実供給 | `observe_court_service_block_v1` → `observe_block`、post-Final は `observe_native_post_final_duties_v1` | `palw_state_v2.rs` の block 末尾 | 既存 |
| **配送/取り込み前提の明示** | **本 lane で追加:** `CourtServiceInclusionPremiseV1 { version, inclusion_delay_daa }`、`PALW_COURT_SERVICE_INCLUSION_PREMISE_V1 = None`（typed-missing）、`PalwTransitionExtrasV1::court_service_inclusion_premise_v1` | processor `palw_accept.rs` が定数を写す。消費者 3 つ: `attribute_service_default_v1`（TIR default guard と post-Final default）、`decide_native_post_final_duty_v1` | **閉じた**（以前は 2 caller が `true` を直書き、post-Final duty 判定は前提を読んでいなかった） |
| duty に基づく default / 供給なし no-slash | `attribute_service_default_v1`（B-03 修正済）、`decide_native_post_final_duty_v1` | TIR default guard、post-Final sweep / tag 178 | 既存 + 前提の判定を追加（`unit + delay <= response_deadline` でなければ NoSlashHold、前提なしは undecided） |
| native adversarial campaign | — | — | 残（下記） |

## 不変条件
- 前提が採用されていない（`None`）間、どの F02 default も `Err`（undecided、移動なし、slash なし）。本番は `None`。
- 前提がある時、supplied unit `u` が actor の opportunity になるのは `first_eligible <= u < response_deadline`、`u < hard`、かつ `u + inclusion_delay_daa <= response_deadline` の時だけ。満たさなければ NoSlashHold。
- 未知の premise version は default にならない。

## 所有権
変更ファイル: `palw_court_service_binding_v1.rs`、`palw_court_service_state_v2.rs`、`palw_tir_public_fold_v1.rs`、`palw_native_post_final_default_fold_v1.rs`、`palw_native_post_final_duty_v1.rs`、tests `r5_f02_wire_v1.rs`・`r6_d4_post_final_duty_v1.rs`。central commit: `palw_state_v2.rs`（extras field）、`consensus/src/.../palw_accept.rs`（定数を写す 1 行）。Codex 担当ファイルには触れていない。

## 試験
command: `buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p kaspa-consensus-core --lib -- court_f02 f02_t r6_d4 r5_f02`、続けて `cargo check -p kaspa-consensus`。結果は commit message と下の「結果」に記す。

- T07（component）`f02_t07_inclusion_premise_boundaries_on_tir_attribution`: unit+delay = deadline-1 / = → PartyDefault、+1 → NoSlashHold、前提なし → Err。
- T02/T07（native、post-Final 経路、TransitionBuilder）`f02_t02_t07_silent_supplied_actor_deadline_and_inclusion_premise_boundaries`: 恒久沈黙の miner、実供給 unit 記録を確認。応答 deadline D-1 / D は undecided、D+1 で PartyDefault。前提 delay の -1/=/+1。陰性対照: 前提なし、version 2。
- T01: 既存 `r6_d4_an_answering_actor_is_not_defaulted`・`verify2_b02_*`（応答した正直 actor は default されない、誤った accuser は勝てない）を同じ run で再実行。
- 使った premise（delay 0、gap±1）と profile は **試験専用の値**で本番値ではない。

### 結果
- `cargo check -p kaspa-consensus`: error 0（processor の field 追加を含む）。
- `cargo test -p kaspa-consensus-core --lib -- court_f02 f02_t r6_d4 r5_f02`: exit 101、**31 passed / 1 failed / 1 ignored**。新規 2 件と T01 該当の既存試験は pass。
- 失敗: `r5_f02_wire_missing_supply_is_a_no_slash_hold_and_the_sweep_resumes_once_supplied`（l.289 で `NoSlashHold("supply is missing")` が返り、期待は PartyDefault）。この判定は premise の判定より前の supply 判定で決まるので、本 lane の変更が原因ではない（base `55510b56a` にある既存の失敗と判断。base での再実行はしていない）。repair 後に別の未供給 packet（turn 1）が残っているのが原因とみる。統括が確認すること。
- native inventory/wire は `scripts/drills/gen_rfc06_native_{inventory,wire}.py` で再生成した（差分は追加 field の 1 行ずつ）。

## SPINE slice との接続（統括の 10-26 方針変更）
SPINE が呼ぶもの: kernel route・Panel=1 の post-Final 経路 `observe_native_post_final_duties_v1` → `decide_native_post_final_default_v1` → `recover_native_post_final_defaults_v1` / tag 178 `NativeHeldDefault`。slice を有効化するには `PalwTransitionExtrasV1::court_service_inclusion_premise_v1` が `Some` である必要がある（本番定数は `None`、値 lane が決める）。harness では試験専用の premise を明示して渡すこと。他の family・job shape は対象外。

## 残件
- CODE: Panel=1 通常 original の受付で、court service binding と資金確保（fee/escrow）を同一の原子的 admission にまとめる経路は funded 経路のみ。通常経路の paid admission との結合は profile 値と funded 受付表 V2 の後に。
- CODE: 継続 turn（follow-up request）ごとの duty/packet 導出は `packets_per_signer` 固定の上限で、adaptive branch 全部を original から導出する列挙はまだない。
- TEST: native adversarial campaign（T01 の誤った challenger を tag 178 経路で全段、reorg 下の premise 境界）。実ノード E2E は未実施。
- 値: `PALW_COURT_SERVICE_PROFILE_V1`、`PALW_COURT_SERVICE_INCLUSION_PREMISE_V1`（inclusion_delay_daa）を値 lane が決める。f=0.25 と整合する根拠が必要。
- EXTERNAL: なし。
