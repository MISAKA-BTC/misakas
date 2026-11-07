# 引き継ぎ:G14 native(v39 mining → action 80)・GDN filer・RFC18(2026-10-18)

branch `integ/g14-court-capacity-r2`(worktree `.claude/worktrees/agent-a76d982ea89a88e54`)。[G14 完全性証明 §13.7〜§13.10](../../../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md) を参照。

## 状態

- **v39 の料金を運ぶ bytes で計る(ユーザー決定 10-18)。** 実装済み。`palw_native_master_fold_v39.rs` と `palw_native_mining_fold_v39.rs` から固定の envelope を外した。audit の再計算も同じ式。ADR-0187 に追記した。
  - node 上で pass:Master 65 の登録(97,337,344 work)、`owning39_master_component_job_reaches_final_and_the_master_is_final`(job Final → Master Final 66)。73 と 74 も通る。
  - 試験の値:mining policy の honest premise 7/10(probe の 2/3 は設計値 k/m = 32/47 を満たさない:2·47 < 32·3)、`network_quantum_mac = 1`(probe の 2^30 だと fixture の Master は 1 quantum 未満で、75 が拒否される)。
- **Gate C の draw(m = 47)。** draw は重複ありの抽選なので、bond 数は原因ではなかった。`NoPopulation` の原因は verifier maturity(7 日 = 5,040 DAA)である。試験はそこまで進めるようにした。
  - **現在の阻害要因。** seed anchor を作るための attempt(card 5、floor class)が、ADR-0176 の bond budget に skip される:"unsettled pre-Final weight would pass its Final-weight cap (PESG §6)"。そのため anchor が記録されず、draw も起きない。
  - 次の手:anchor 用の attempt は、Final-weight cap に余裕のある card で出す(card を順に回す)。または、その間に過去の attempt を Final にする。そのあと attest(inner 75)→ 認証 → 77 → 80 の残りの assert を通す。
- **GDN filer。**
  - service の tick に接続した:`kaspad/src/palw_panel/gdn_filer_tick.rs` を `replay_filer_tick_v1` の後に置いた。`PalwExecutionBackendV1::as_any_v1`(既定は None)と、base0 の `Qwen36Backend::gdn_inventory_openings_v2` を追加した。
  - fold 上の E2E(`held_court_e2e.rs` の `rfc06_held_gdn_filer_e2e_*`:全 seat 共謀、外部 bond STRANGER、cold cache、担保の減額)は書いたが、**未 build・未実行**。
- **`rfc0018_dag_siblings`。** 3 件とも pass して commit 済み。fence は `Config::new` で武装した。fixture の model 行の `verification_ccu` を 1 にし、`ClassDeadlineUnmeasured` を避けた。fence 未満の順は変えていない。
- **receipt lane のリリースノート。** `docs/operations/releases/release-notes-pending.md` に書いた。protocol-duty の既定で、consensus への影響はない。

## 実行中のプロセス(止めていない)

- PID 93187 / 96247:`cargo test -j 3 -p kaspa-consensus --lib -- g14n2_node --nocapture`。log は scratchpad の `m7.log`(`/private/tmp/claude-501/…/scratchpad/m7.log`)。上の budget の skip で失敗する見込み。
- PID 509、1202、98116 は他の lane のもので、この lane のものではない。

## 次の手順

1. g14n2 の node 試験で anchor 用 attempt の budget skip を直して完走させる(`--nocapture` で約 14 分。build は約 17 分)。
2. `cargo test -j 3 -p kaspad --lib -- rfc06_held_gdn_filer carrier_wait_many_classes` で、tick の接続と fold E2E を build・実行する。
3. 結果を §13 に追記し、`python3 docs/maintenance/check_docs.py` を通して push する。
