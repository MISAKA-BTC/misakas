# Lane ACT2 報告 — DAA 17,000 武装一覧と record/waiver(2026-10-27)

branch `claude/g14-act2`。push・配備なし。preset には適用していない(`PALW_T12_FULL_ACTIVATION_DAA` などは `None` のまま)。

ユーザー決定(2026-10-27): drill を省略して有効化する。testnet-12 の DAA 17,000(当初 19,000 から変更)で、Panel=1 kernel slice と G14 の 6 family に加え PanelDa(後から追加)を武装する。実行していない要件は waiver として記録する。完了・合格としては記録しない。

## 1. release の取り込み

- live release の機能 commit を cherry-pick した: `5f7e22707`、`c313e903e`(BASE-0 audit)、`2b16ba257`(node-less)、`3064832d5`、`00a9e1aec`、`ba93d9d2f`(kit)、`ddf5485d8`(= 24d756077 の bind-after-4-slots)。
  - 衝突は 2b16ba257 の 3 file だけで、HEAD 側が空の追加だった。incoming 側を採った。
  - fa21dd9a7 は、この tree に既にある試験修正と重なったので skip した。
- `ddf5485d8` の通常 merge は約 400 file が衝突した。原因は main の公開用刈り込み(`d2214e057` ほか)。機械的には解けない。
  - そこで `-s ours` で merge し、ancestry だけを記録した。
  - 247ade1f9 の testnet-12 ruleset(int-13 @15,500)は、lane FLEET がこの tree に既に再現している。`fleet_ids_match_247ade1f9` は pass した。fleet identity の確認は §5 にある。
  - 注意: 247ade1f9 が持ち込んだ非 consensus の追加(`palw_paid_bootstrap_economics_v1` など)は、ids に出ない範囲では取り込まれていない。

## 2. 一覧 `PALW_T12_G14_17000_FENCES_V1`(`full_activation_v1.rs`、高さ `PALW_T12_G14_17000_DAA_V1 = 17_000`、preset は読まない)

full 一覧の順に並べた(前提が先)。武装は `palw_t12_arm_after_int13_at_v1` を通す。int-13 の 4 本は 15,500 のまま残る。

| # | fence | 由来 |
|---|---|---|
| 1–4 | audit_1004 / gen_range_twin / model_court_window / receipt_spend_v4 | int-13(15,500 で保持、動かない) |
| 5 | palw_probabilistic_constraints_v1 | slice, KernelRoutePanelOne |
| 6 | palw_kernel_route_panel_one_v1 | slice |
| 7 | palw_provider_court_v1 | slice |
| 8 | palw_legacy_public_filer_v1 | Legacy×2, FpConstraint/Prefix/Tokenizer |
| 9 | palw_legacy_held_da_v2 | LegacyPanelAttempt/FusedLeaf |
| 10 | palw_staged_public_court_v1 | FpConstraint/Prefix/Tokenizer |
| 11 | palw_staged_tir_route_v1 | tir_public_conviction の前提 |
| 12 | palw_tir_container_identity_v1 | 同上 |
| 13 | palw_tir_public_conviction_v1 | FpSelection |
| 14 | palw_court_outsider_lane_v1 | F03 outsider lane |
| 15 | palw_court_clock_v2 | PanelDa の前提 |
| 16 | palw_court_capacity_v1 | LegacyFusedLeaf |
| 17 | palw_bond_budget_v1 | slice(settlement once の前提) |
| 18 | palw_claim_hard_deadline_v1 | slice(X3) |
| 19 | palw_settlement_once_v1 | slice |
| 20 | palw_independently_work_certified_v1 | slice(Gate C) |
| — | ~~palw_fp_tokenizer_match~~ | FpTokenizer。**t12 は DAA 5,300 から既に有効**。17,000 で再武装すると 17,000 未満の fence が動くので、一覧から外した(identity 試験で検出) |
| 22 | palw_panel_da_disclosure_v1 | PanelDa(`palw_panel_da` は t12 で既に有効) |
| 23 | palw_g14_route_required_v1 | 全 family |

- `palw_claim_challenge_v2`(RFC07 probe)は含めていない。試験で確認している。
- 試験:
  - `palw_t12_g14_17000_arming.rs`: slice と 7 family の fence を網羅していることを確かめる。拒否を列挙する。
  - `t12_fleet_identity_247ade1f9.rs::g14_17000_list_keeps_everything_below_17000`

## 3. validation(`validate_palw_v2`、出荷 records)の拒否

方法: 一覧の順に 1 本ずつ足し、拒否された fence は外したまま次へ進む。最終版の 22 本(int-13 の 4 本は保持)で再実測した。**18 本のうち 16 本が拒否、2 本(tir_container_identity、bond_budget)が OK**。`validate_palw_v2` は最初の拒否しか返さない。前提で止まった fence は、前提が通った後に自分の record gate でも止まる(コードで確認済み)。

| fence | 拒否(実測の最初の理由) | 解除に要るもの | 分類 |
|---|---|---|---|
| probabilistic_constraints_v1 | kernel-route record なし | kernel-route record(8 要件) | (b) waiver。ただし §6-1 |
| kernel_route_panel_one_v1 | 前提(5) | 5 | 前提 |
| provider_court_v1 | 前提(5)。その後に自分の kernel-route record(4 要件) | record | (b) |
| legacy_public_filer_v1 | G14 record なし(LG14-A 7 要件) | record | (b)。V5 は測定 |
| legacy_held_da_v2 | 前提(8)。その後に自分の record(7) | record | (b) |
| staged_public_court_v1 | 前提(8)。その後に V1–V7 record | record | (b) |
| staged_tir_route_v1 | 前提(10)。その後に V1–V7 record | record | (b) |
| tir_container_identity_v1 | OK | — | — |
| tir_public_conviction_v1 | G14 V1–V7 record なし。加えて `PALW_TIR_FRAME_COST_RECORDS_V1`(空) | record 2 種 | (b)。frame cost は測定値 → §6-3 |
| court_outsider_lane_v1 | 前提(8) | 8 | 前提 |
| court_clock_v2 | (PanelDa 追加分)前提 legacy_public_filer(8) | 8 | 前提 |
| court_capacity_v1 | G14 record なし。policy は `PalwCourtMeasurementV1` から導出が一致すること(安全検査) | 測定 record(ARM-LEGACY V11) | (a) 測定が入れば。waiver では不可 |
| bond_budget_v1 | OK(承認済み V3 目安 policy) | — | — |
| claim_hard_deadline_v1 | X3 record なし(4 要件、2 番目が B_* 測定) | record | (b)。測定項目は目安 pin 済み、waiver を明記 |
| settlement_once_v1 | 前提(5, 18)。その後 **`PALW_RFC0608_OPEN_BLOCKERS_V1` の RFC08-2** | blocker の解消 | **(c)** §6-2 |
| independently_work_certified_v1 | 前提(5) | 5 | 前提 |
| fp_tokenizer_match | (一覧外。5,300 で有効) | — | — |
| panel_da_disclosure_v1 | 無条件拒否(実ノード E2E 未) | lane PANELDA が 245f36f54 の配線を要件とし、E2E を waiver にする変更 | (b)(その変更が前提) |
| g14_route_required_v1 | G14 record なし(T-N8/T-N10/E2E/drill) | record | (b) |

## 4. record / waiver の対応(fence × 要件 × 証拠)

既存証拠のうち pin script の形式(`passed=true`・`exitCode=0`)を満たすものは無い。TNODE の run-c335f4e4e は全部 `passed:false` だった(slice が settlement once で拒否され、node が起動しなかったため)。family lane の結果は報告文の中にしか無く、結果 JSON は無い。

| fence | 要件 | 証拠候補 | 記録 |
|---|---|---|---|
| probabilistic_constraints_v1 | #1 V1 suite | lane KNODE/F03k の fold 試験(報告のみ) | 結果 JSON を作り直すまで waived: user decision 2026-10-27 (suite result not recorded as run JSON) |
| 〃 | #2 node E2E | g14_kernel_route_e2e(TNODE で blocked) | waived: user decision 2026-10-27 (E2E not executed) |
| 〃 | #3 adversarial suite | — | waived: user decision 2026-10-27 (not run) |
| 〃 | #4 review | 独立レビュー 2026-10-26 part1/2(実装者と独立した Claude agent) | 候補(review digest) |
| 〃 | #5 shadow | — | waived: user decision 2026-10-26 (TNODE req-4 に既記) |
| 〃 | #6, #7 measured | — | waived: user decision 2026-10-27 (unmeasured) |
| 〃 | #8 drill | — | waived: user decision 2026-10-27 (drill skipped) |
| provider_court_v1 | V1 / E2E / measured / drill | lane CLOCK suite(報告のみ) | V1 は候補、残りは waived: user decision 2026-10-27 |
| legacy_public_filer_v1 | V1 | family-legacy: close_legacy 8 passed / 1 failed(pin) | 失敗が残るため waiver は不可、修正後に run JSON |
| 〃 | V2–V4, V6, V7 | lg14a node E2E(未) | waived: user decision 2026-10-27 (E2E not executed) |
| 〃 | V5 measured | — | waived: user decision 2026-10-27 (unmeasured) |
| 〃 | drill | — | waived: user decision 2026-10-27 (drill skipped) |
| legacy_held_da_v2 | V1..V7, drill | lg14b suite(報告のみ) | V1/V4 は候補、残りは waived |
| staged_public_court_v1 / staged_tir_route_v1 | V1–V7 | g14_fam_fpcp 4 passed(fold)。kaspad fpcp_ は 43 passed / 6 failed | 失敗 6 件を直すまで V1 は不可。V2–V7 は waived |
| tir_public_conviction_v1 | V1–V7 + frame cost | fpsel: lib 93 passed / 1 failed(無関係)。node E2E 未 | V1 候補、V2–V7 waived。frame cost は §6-3 |
| court_capacity_v1 | 7 要件 | ARM-LEGACY の V11 測定(進行中) | 測定値は waiver 不可、(a) |
| claim_hard_deadline_v1 | V1 / measured / E2E / drill | ruling suite(報告のみ)。B_* は目安 pin | measured は waived: user decision 2026-10-27 (estimates pinned, unmeasured)、E2E と drill も waived |
| g14_route_required_v1 | T-N8, T-N10 | g14_close_gates 12/12 passed(fptok 報告) | 候補(run JSON の再取得が要る) |
| 〃 | E2E, drill | TNODE は blocked | waived: user decision 2026-10-27 |
| panel_da_disclosure_v1 | wiring / E2E | 245f36f54 の fold 試験 | E2E は waived(lane PANELDA の変更後) |
| settlement_once_v1 | RFC0608 measured record | — | blocker が先(c) |

## 5. identity

- `t12-fleet-identity.sh 24d756077`(cherry-pick と merge の後、tokenizer 除去の前): **IDENTICAL to 24d756077**(params、identity、schedule id、fork id、17,000 未満の fence)。
- 最終 tree での `g14_17000_list_keeps_everything_below_17000` は pass。`fleet_ids_match_247ade1f9` も pass(4 passed)。
- 武装後の schedule id は `a2759b1c…`(出荷時は `9ef2b1a7…`)。17,000 の高さが加わるので変わるのは想定どおり。17,000 未満の schedule は同一。
- 分岐専用の試験 `g14_17000_list_keeps_everything_below_17000` を置いた。一覧を 17,000 で武装しても、次のものは出荷 params と同一であることを確かめる。
  - handshake identity
  - 15,500 以前を含む各 DAA の fired fork id
  - 17,000 未満の schedule
  - 17,000 未満で武装済みの fence
- 実測で、fork id の `next` は 17,000 と none とで異なった。fork id は次の fence を告知する設計(ADR-0072 SA-2)なので、これは想定どおりで、`fired` は同一だった。旧 node は `next` の違う peer を受け入れる。ただし、これは配備時の handshake 観察で確かめること。

## 6. 実装の穴(c)

1. **record 形式に waiver を表す欄が無い。** `PalwG14ArmingRecordV1`、`PalwKernelRouteArmingRecordV1`、`PalwRfc0608MeasuredRecordV1` は、要件ごとに evidence digest を 1 つ要求する。`pin_arming_record.py` は `passed:false` を拒否する。
   - waiver の文書の digest を evidence 欄に入れると、binary の中で「実行済み」と区別できなくなる。これは禁止事項に当たる。
   - 必要な対応: evidence 要素を型付きにして(`Run(digest)` / `Waived{decision_digest}`)、pin script に `--waive REQ="user decision 2026-10-27 …"` を足すこと。統括の判断が要る。
2. **settlement_once_v1 は RFC08-2 の open blocker で拒否される。** `PALW_RFC0608_OPEN_BLOCKERS_V1` に載っている。
   - fence を越えると、結合 liability 述語は認証された期限が無い leg を名指しで拒否する。対象は次のとおり: native mining receipt、native funded/text/job/mining admission(signed hard deadline が無い)、paid V2 外の kernel claim、material provider fee、beacon source-fee、paid V2 外の verifier fee。
   - これは安全のための拒否なので、waiver では外せない。武装すると、これらの経路が 17,000 で止まる(liveness)。
   - 選択肢: (i) settlement_once を一覧から外す(slice の定義変更)。(ii) 各 leg の source を実装する。
   - FPTOK が報告した「test record を入れても slice が拒否される」も、この blocker が原因。
3. **`PALW_TIR_FRAME_COST_RECORDS_V1` が空。** tir_public_conviction には測定値の record が要る(目安は `PALW_G14_TIR_FRAME_COST_ESTIMATE_V1`)。測定値は waiver にできない。対応は、測定するか、目安を値として pin する規則をユーザーが承認するかのどちらか。
4. **court_capacity の policy は、測定 record から導出したものと一致することを検査される。** ARM-LEGACY の V11 測定が来ないと武装できない。
5. 失敗のまま残っている試験がある: kaspad fpcp_ の 6 件、close_legacy と court_capacity の pin 各 1 件。V1 の evidence にするには、先に直す必要がある。

## 7. その他

- integration tip `1b9f9a45f`(FPTOK)を merge した。衝突はなかった。
- compile の修正:
  - `misaka-palw-remote` に `misaka-endpoints` の依存を追加した(2b16ba257 の palw-evidence が使う)。
  - `protocol/flows` の orphans 試験を `BlockTransactionsV3` に合わせた。
  - `misaka-palw-worker` は llama.cpp(`MISAKA_LLAMA_SRC`)が必要なので、check から外した。
- 全体 check: 上記 2 件を除けば error は無かった。修正後に該当 crate だけを check して exit 0。
