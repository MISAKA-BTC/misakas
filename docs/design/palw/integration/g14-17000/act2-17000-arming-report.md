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

## 8. 第 2 段(2026-10-27 後半)

### settlement_once(RFC08-2) — 推奨は (ii) 一覧から外す(実施済み)

- (i) 閉じる場合: 各 leg に認証された期限を持たせる。対象は native mining receipt、native funded/text/job/mining admission(signed hard deadline)、paid V2 外の kernel claim(producer 署名の lifetime)、material provider fee、beacon source-fee、verifier fee。wire(新しい object 欄)、fold、mempool/template、SDK にまたがる。変更先は `palw_settlement_liability_v1/_fold_v1`、`palw_native_mining_fold_v39`、`palw_monetary_material_fold_v1`、kernel route、beacon fee など。複数日の作業になり、freeze に間に合わない。
- (ii) 外した場合: 一覧の他の entry で settlement_once を「以下」に要求するものは無い(要求するのは paid_claim_trap と native_liability_horizon で、どちらも一覧外)。
  - Panel=1 の Final 決済は、各支払点の既存の once-guard が守る。escrow 行の削除、phase、paid flag で、t12 が今動かしている方式と同じ。
  - 失うものは、consumer をまたぐ二重決済の構造的な排除(aux 140 marker)と、結合 liability 述語。
  - 既知の残存リスク: 同じ obligation に別々の consumer が支払う経路(kernel GAP-5 escrow の tick・object arm・paid-claim projection・EXEC fee)は、escrow 行の共有で防いでいる。cross-consumer の試験は未実行。
  - 外れるのは slice の要件「settlement once」だけ。family の要件で外れるものは無い。

### typed waiver(実装済み)

- `consensus/core/src/palw_arming_waiver_v1.rs`: 種類は `NotRun` / `UserDecision`。`decided_by` は "user …" でなければならない。(fence, network, height, slot) に束縛される。
- G14 / kernel-route / RFC-0018 の record 形式検査に組み込んだ。waiver を別の slot に移すと拒否される。
- `pin_arming_record.py` に `--waive` / `--decision` / `--waive-review` を追加した。genesis の 128 hex 対応も直した(Hash64 は 64 byte、旧版は 32 byte しか受け付けなかった)。
- 試験(`palw_t12_g14_17000_arming.rs`、4 passed):
  - pin した全 slot が waiver であること(Python と Rust の digest の一致も兼ねる)。
  - waiver で record は通るが、順序(held DA → filer)、値(court capacity の placeholder)、open blocker(RFC08-2)の拒否は残ること。

### record の pin(実施済み)

- `scripts/release/gen_t12_17000_records.py` で `palw_t12_17000_records_v1.rs` を生成し、binary の G14 / kernel-route / RFC-0018 / frame-cost record にした。対象は testnet-12 の 17,000 だけ。
- **全 slot が waiver。** pin 形式(`passed:true` JSON)の run 出力が一つも無いため。ARM-LEGACY の 7/7 E2E などは「報告済み、出力ファイルは無い、未実行として扱う」と明記した。
- frame cost: lane VAL の目安値。measurement と review は waiver。
- Gate C U-1(17,000)と U-5(75/41)は `UserDecision`。U-6 と U-2/U-3 は個別の裁定が無いので `NotRun`。

### 再実測(merge `6aaa2e0ed` の前の tree)

- 21 本(int-13 の 4 本は保持)を 1 本ずつ足すと、10 本が拒否された。
- そのうち filer と tir_public_conviction の F03 拒否は、1 本ずつ足す方式の副作用(outsider lane が後ろにあるため)。同じ高さに全部置けば解消する。
- 一覧全体を武装したときの最初の拒否は次のとおり:
  `palw_independently_work_certified_v1 needs palw_model_tier_v1 at or below it`

### (c) 残り

1. **Gate C には `palw_model_tier_v1` が要る。** これは一覧に無い。
   - t12 には tier fence が install されていない。entry の既定は空の catalog(genesis 8 モデルが無い)と設計値の κ で、自前の record(c・κ の承認、catalog tip)も無い。
   - 必要なもの: genesis 8 モデルの catalog の値、κ のユーザー承認、tier record。これが無いと Gate C も、Gate C を要する KernelRoutePanelOne も武装できない。
2. **court_capacity**: t12 に policy が install されていない(placeholder は拒否される)。
   - `PalwCourtMeasurementV1::policy()` は全 class の測定(時間・bytes・working set)を要求する。さらに `session_weight` を 0 に固定する。そのため、one-move class だけの M-CAP 実測からは V11 目安(weight 13)を導けない。
   - 安全検査なので waiver では解けない。LegacyFusedLeaf はここで止まる。
3. 一覧全体に対する validation の、model tier より後ろの拒否は、まだ列挙していない(validation は最初の拒否しか返さないため)。

## 9. 第 3 段(統括の決定を反映、tip `d466fcd40` 以降)

- model tier を一覧に追加した(`PALW_T12_G14_MODEL_TIER_ENTRY`)。値は c 300‰、κ 1000‰、catalog edition 0 は空で、すべてユーザー決定。
  - tier record も pin した。U-6 は `UserDecision`。「Gate C の非共有モデル試験(legacy/FP)」は `NotRun`。
  - catalog の root は Python で計算した。Rust 側の照合は通っている。
- court capacity: V11 の目安 policy を本番 policy として install する entry にした。
  - validation が測定 record 無しで受け付けるのは、**その目安の policy と完全に一致し、かつ record の測定 slot が pin 済みの `UserDecision` waiver** である場合だけ。
  - placeholder や他の policy は引き続き拒否する(試験あり)。
- peel(拒否された fence を外して再検証、を繰り返す)の結果:
  1. `palw_model_tier_v1 needs palw_model_bond_allocation_v1 at or below it` → **(c)**。ADR-0177 allocation の承認済み policy(曲線 f、allocation epoch、seasoning)が空。加えて、ACT が先に報告した rights epoch 720 と bundle 1,000 の不整合がある。値の決定が要る。
  2. Gate C は model tier に依存するので、ここで止まる。
  3. `palw_tir_public_conviction_v1` には frame cost の値(`palw_tir_frame_cost_v1`)が要る。目安値を install すると、**handshake identity が 17,000 未満で変わる**(高さで区切られずに Some-only で hash されるため)。fleet と peer できなくなるので、一覧から外した → **(c)**。identity 側で高さ区切りの hash にする central 変更が要る。
- それ以外の fence は全部通る(peel の残り = Ok)。fleet identity の 4 試験は、frame cost を外した状態で通ることを確認する必要がある(未実行)。
