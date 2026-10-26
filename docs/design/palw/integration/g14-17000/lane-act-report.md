# Lane ACT 報告 — Panel=1 有効化経路と fleet 整合(2026-10-26)

branch `claude/g14-act`(base `55510b56a`)。fence は武装していない。push・配備なし。

## 1. fleet 整合 — 未完了(統括判断が要る)

調べた事実:

- 公開 fleet の source は `247ade1f9`(親 `6549f9be2`)。`origin/main` の `d2214e057` はこの tree をそのまま採った merge。`832d9279e`/`1acfbc916`(full activation を 15,100/15,400 に武装)は main の first-parent にあるが、`d2214e057` で捨てられている。fleet の tree には `full_activation_v1.rs` が無い。
- この branch と main の merge-base は `6a128eb65`。`git merge origin/main` は 269 file 衝突、約 180 万行の削除を持ち込む(main の公開用の刈り込み)。機械的な merge は不可。
- `247ade1f9` だけを cherry-pick すると 42 file 衝突になる。その中に Codex が編集中の `palw_claim_challenge_fold_v2.rs` がある。`247ade1f9` は int-13 を 15,500 に置くだけでなく、新 module `palw_paid_bootstrap_economics_v1`(751 行)、wire scan の追加、`PalwPanelV3*` の module 移動、`TransitionBuilder::try_new` を持ち込む。
- この branch には `6549f9be2` 以降の 229 commit がある。`params.rs`/`identity.rs` だけで約 1,070 行の差分がある。fingerprint `ef379378f09bf33f…` を再現するには、`247ade1f9` を移したうえで release build 2 本の比較が要る。

依頼: `palw_claim_challenge_fold_v2.rs` の Codex 作業が終わってから、`247ade1f9` を移す。手順は `git cherry-pick 247ade1f9` → 衝突を解く(int-13 = `Some(15_500)`、handshake は int-12)→ `kaspad --version` などが出す fingerprint を、`247ade1f9` の release build と比べる。この比較で fingerprint が一致したら、試験として pin する。

## 2. 有効化の機構 — 完了

- `PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1 = 17_000`。`PALW_T12_FULL_ACTIVATION_DAA` は `None` のまま。武装時は `Some(PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1)` にする。drill と試験では既存の `palw_t12_arm_full_activation_at_v1(.., Some(h))` と `--palw-drill-full-activation-at` が高さを差し込む。
- `scripts/drills/g14_t12_16000_fence_drill.py` に `--public-target` を追加した(既定値 17000)。`fleet.env.example`、`t12_deploy_kit_constants`、`palw_t12_full_activation_16000` は 17,000 を前提にした。

## 3. 同時武装の拒否

- **block_opportunity と V+ — 修正した。** V3 policy の `validate` は `false` を拒否しなくなった。fence 側は「V+ が同じ高さ以下にあるなら `false`、そうでなければ `true`」を要求する(pin 2026-10-22 ADJUST 1)。
- **rights epoch 720 と bundle epoch 1,000 — 未修正。** V+ は rights epoch = bundle の `epoch_length` = 1,000 とする。allocation epoch は 1,000 の倍数で、かつ 5,040 を割り切る必要がある。両方を満たす値は無い。直すには、V+ に bundle と独立した rights epoch を持たせる(RFC-0016 の改訂)か、t12 bundle の epoch を変える(consensus 変更)しかない。どちらも採択済み設計の範囲を超えるので、統括とユーザーの判断に回す。
- **一覧外の 3 fence:**
  - `palw_bond_budget_v1`: 値以外の包括拒否をやめた。承認済み policy digest の一覧 `PALW_BOND_BUDGET_APPROVED_POLICY_DIGESTS_V1` は空。値が入るまでは typed-missing で拒否する。
  - `palw_model_bond_allocation_v1`: ADR-0177 D6(モデル単位の開閉評価)を gate から外した(PRINCIPLES §9 によりモデル単位の審査は不採用)。承認済み policy 一覧 `palw_model_bond_allocation_approved_policies_v1` は空。
  - `palw_material_income_v3`: 未修正。拒否理由は値ではなく、「signed material/TTL と consumer lifecycle の検証が未完」。前提の `palw_reporter_income_v2` も包括拒否のまま。record gate に移すには要件の定義が要る。

## 4. record pin — 機構だけ作った

`scripts/release/pin_arming_record.py` を追加した。受け付ける evidence は、`exitCode=0`・`passed=true`・`sourceCommit` が一致する JSON の実行結果だけ。文書、推定値、失敗した run、重複した bytes、evidence と同じ review は拒否する。出力は Rust の literal で、一覧は編集しない。record は空のまま。

未実装: suite、node E2E、drill の各 runner が、この形式の結果 JSON を書き出すこと。

## 4b. 縦切り slice(統括 2026-10-26 の方針変更)

- 調べた結果: full activation 一覧は全体を一括で武装する作りだった。record の鍵は (fence, network, height) だけで、class の欄は無い。そのため class・job shape では record を絞れない。class は route 自身の policy に持たせるしかない(SPINE が名前を決める)。
- 作った機構: `PALW_T12_KERNEL_SLICE_FENCES_V1` を置いた。中身は int-13 の 4 本、probabilistic_constraints、panel_one、claim hard deadline、settlement once、Gate C、g14_route_required で、full 一覧の部分集合として順序も保つ。汎用の `palw_t12_arm_fence_list_at_v1(params, list, at)` も追加した。試験 `the_kernel_slice_is_an_ordered_subset_and_clears_back` を足した。
- settlement once は `palw_bond_budget_v1` を前提にするが、これは値が入るまで拒否される。record は空のまま。

- 統括の決定(2026-10-26)に従い、`PalwG14RouteFamilyV1::KernelRoutePanelOne` を末尾に追加した。既存の variant は番号を変えていない。この enum は serialize も hash もされないので、版は上げていない。
  - `route_armed` は、panel_one と Gate C の両方が有効なときだけ true を返す。
  - mirror には `kernel_panel_one_from` と `gate_c_from` を加えた。route_required が武装されていなければ mirror は `None` のままなので、fence 未満の挙動は変わらない。
  - `palw_g14_route_family_fences_v1(family)` を追加した。各 family が必要とする fence 名を返し、record の集合がそれを満たすかを確かめられる。
  - 試験 `the_kernel_slice_covers_the_kernel_route_family` を足した。
  - class(card 1)と one-prompt decode の判定は SPINE の classifier に属する。CERT が終わるまでは武装できない。

## 5. Panel=1 の実経路の洗い出し — 未着手

時間の都合で手を付けていない。

## 試験

- `python3 -m pytest scripts/drills/tests/test_g14_t12_16000_fence_drill.py`: 17 passed
- `python3 -m pytest scripts/release/test_pin_arming_record.py`: 3 passed
- `cargo test -p kaspa-consensus-core --lib -- palw_values_p4 palw_bond_budget ...`: exit 0、135 passed
- `--test adr0177_allocation_v3`: 5 passed。`rfc0016_v1`: 19 passed。`t12_deploy_kit_constants`: 4 passed。`g14_close_gates`: 12 passed
- `--test palw_t12_full_activation_16000`: 7 passed(新規 2 本を含む)、1 failed。落ちたのは `armed_at_a_test_height_..._comes_back_byte_for_byte` の「identity unchanged」の検査。私の変更を外しても同じ hash で落ちるので、base `55510b56a` から既にある。main の `832d9279e` が identity.rs で直した件と同じものに見える。fleet 整合の作業で一緒に扱うこと。
- `g14_close_legacy`: 7 failed。`g14_court_capacity`: 12 failed。どちらも理由は同じ `OneOutsiderCapacityIncomplete`(F03)と `ServiceOpportunityIncomplete`(F02)で、F02/F03 lane の担当。
