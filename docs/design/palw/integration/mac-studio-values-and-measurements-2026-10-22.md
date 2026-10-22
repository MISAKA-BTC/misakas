# Mac Studio への作業指示 (2) — 値と実測をすべて揃える（2026-10-22）

前の指示書 [mac-studio-handoff-2026-10-22.md](mac-studio-handoff-2026-10-22.md) の §1 の規則（push は `studio/*` だけ／live の t12・VPS には接続しない／実行していないことを実行済みと書かない／ハードウェアを併記）は、この指示書でもすべて守ってください。準備（§2）も同じです。branch は `g14/integration-r7` を最新まで fetch してください。

## 0. この指示書の目的

testnet-12 で DAA 16,000 の fence に全機能を載せるには、各 fence の **値**（金額・上限・予算・期間）と **実測**（arming record の evidence）が必要です。現在、すべての record list は空で、値は `None`（typed-missing）なので、fence を武装しても validation が拒否します。あなたの担当は次の 3 つです。

1. **実測する。** 計測 harness を実行し、生データ・環境・binary の sha256 を残す。
2. **値の案を導出する。** 各値の定義元（RFC / ADR の該当節）にある式・制約に、実測値を入れて案を出す。式が無い値は、選択肢と、それぞれを選んだときの影響を並べる。
3. **arming record の候補を作る。** 各 gated fence の requirement 1 件ごとに evidence 文書を作り、その sha256 を requirement の順で並べた候補表を作る。

**値の採択はユーザーが行います。** あなたは値を決定せず、コードの定数（`None` の値、`*_RECORDS_V1` の list）にも書き込みません。案と根拠を返し、ユーザーが承認した後に MacBook 側がコードへ pin します。

## 1. 計測の共通ルール

- **「最低要件ハードウェア」での値が必要な項目**（court 検証時間、X3 の `B_*` 項など）について:
  - Mac Studio での実測値はそのまま採用しない。必ず `sysctl hw.model hw.memsize hw.ncpu` を併記する。
  - 最低要件の機械で同じ計測を再現できるように、計測用 binary またはスクリプトと、手順を `docs/evidence/palw/studio-values-2026-10-22/rerun/` に置く。MacBook 側が VPS（例: 6 コア・11 GB の x86_64 Linux）で再実行する。
  - 換算倍率の推定を併記してよいが、推定であると明記する。
- 計測は最低 20 回。中央値・p95・最大を報告し、最初の 1 回（warm-up）は分けて扱う。
- 成長の次数（線形・二次など）が問題になる項目は、N を少なくとも 3 段階変えて測る。
- 既存の計測記録とその形式に合わせる:
  - `docs/measurements/2026-10-19-court-verify-cpu.md` / `.json`
  - `consensus/core/tests/palw_tir_court_verify_cpu.rs`（`MEASURE_OP` / `MEASURE_N`、1 process 1 operation）
  - `misaka/palw/tir/exec/tests/frame_measure.rs`
  - `consensus/core/tests/t12_producer_exposure_measured.rs`
  - `docs/evidence/palw/g14-measure-2026-10-18/`
  - `docs/measurements/2026-10-19-t12-passive-daa-blocks.md`（t12 の実測ペース: 中央値 184 s/DAA、block の tx bytes の最大 111,949 B）

## 2. 値の一覧（すべて案を出す）

定義の正本は `docs/design/palw/integration/g14-t12-16000-decisions-2026-10-22.md`（V1〜V12）と、各 lane の notes（`docs/evidence/palw/g14-parallel-2026-10-22/*.md`、`docs/evidence/palw/rfc-gaps-2026-10-22/*.md`）です。

| # | 値 | コード上の置き場（現在 `None`） | 主な入力 |
|---|---|---|---|
| V1 | court service profile（DAA あたりの予約予算、packet 単価、署名者あたりの packet 数、span） | `PALW_COURT_SERVICE_PROFILE_V1` | court 操作ごとの実測（§3 M-C）、block の容量（tx bytes・mass）、ADR-0192 |
| V2 | funded original の受付表（fee、capital の下限、分岐ごとの capital、heap・保持 bytes） | `PalwFundedOriginalScheduleV1` / `FundedAdmissionScheduleV1` | 分岐数の導出（`palw_funded_original_admission_v1.rs`）、M-C、RFC-0035 §7.1 |
| V3 | escrow の精算表（作業料・court 料・relay 料・burn の dues と取り分） | `palw_escrow_fee_schedule_v1` | V1・V2、RFC-0035 §7 |
| V4 | 外部者の court 受付料、内部者も払うか、予約枠 R=⌊cap/2⌋ の妥当性 | `PalwCourtIngressScheduleV1` | M-C、court cap 42、L-COURT 実測 |
| V5 | material 受付の保管料（sompi/byte）、draft の TTL、payer | `MaterialIntakeScheduleV1` | 保持 bytes の実測、RFC-0032 |
| V6 | Final 後 default の slash 額 | `PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1` | ADR-0184、既存 default の slash 規則との整合 |
| V7 | D2 readiness の draw cap、disclosure の料金、slash 額 | `PALW_D2_READINESS_DRAW_CAP_V1`、`PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1` | ADR-0188 B / ADR-0200、seat 数 |
| V8 | 1 位置 terminal の witness 予算、lookahead の frontier 上限 | `PALW_TIR_POSITION_WITNESS_BUDGET_V2`、`PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2` | RFC-0024 §4.4 / §5.5（「profile から導出」）、`r5-p2-terminal.md` の byte 計算（署名 2 本だけで 9,262 B） |
| V9 | staged court config の保持期間（a+10,000 + reorg 保持を覆うか） | staged court config（`network_v1` では不足） | `r5-clock-deps-relay.md`、clock v2、X3 |
| V10 | X3 の `B_cold_fetch`・`B_check`・`B_localize`・`B_carrier`・`B_reorg_slack`、および `c_retain` | X3 arming record（`palw_claim_hard_deadline_v1`） | §3 M-X3（実測）。どの項も 0 にしない |
| V11 | court capacity の `rho_court`（sompi / 占有 DAA / weight 単位）と session weight | G14 arming record（`palw_court_capacity_v1`、G-07） | §3 M-C、占有の経済性 |
| V12 | 前提 fence の policy: `palw_bond_budget_v1`、`palw_model_bond_allocation_v1`、`palw_material_income_v3` | 各 fence の policy | ADR-0176/0177、配分の平均期間は **7 日**（ユーザー決定済み） |
| R1 | RFC-0006: draw と価格の値、配分曲線、cell closure を武装するか | `palw_rfc0006_*`、cell coverage | `rfcgap-a` の報告 |
| R2 | RFC-0007: claim-challenge policy の値、source の deposit と cap、安全余裕・retry 予算、attempt あたりの deposit 上限、bootstrap profile | 各 policy | trap の実測 |
| R3 | RFC-0010: 本番 beacon policy | `palw_panel_beacon_v1` の承認済み list（空） | 選出・交代の実測 |
| R4 | RFC-0016: 実測 record とクラス別の DA・court 負荷表 | `PALW_RFC0016_MEASURED_RECORDS_V1`、class load table | M-C、M-DA |
| R5 | RFC-0006/0008 の実測 record | `PALW_RFC0608_MEASURED_RECORDS_V1` | m/k、取得・検査・court の費用 |
| R6 | TIR frame の費用 record | `PALW_TIR_FRAME_COST_RECORDS_V1` | `frame_measure.rs` |
| R7 | RFC-0002: PALWQ361 の logit scale と f32 参照窓 | `--palwq36-logit-scale`、reference-windows file | §3 M-Q36 |

値ごとに必ず書くこと: 定義元（RFC / ADR の節）、式、入力した実測値、案の値、余裕（何倍か）、その値で何が保証され何が保証されないか、ほかの値との依存関係。式が RFC / ADR に無い場合は、候補 2〜3 個と、それぞれの影響を書く。

## 3. 実測の一覧

| ID | 何を測るか | 方法・harness | 使う値 |
|---|---|---|---|
| M-C | court 操作クラスごとの検証時間・evidence bytes・working set（close cone acquit / forged last stage、root claim admit、dissect round apply、dissect bottom honest、position close、lookahead、shared bottom v2、material intake の part / finish、D2 demand / answer、post-Final duty の accusation / answer） | `palw_tir_court_verify_cpu.rs` の `MEASURE_OP` を拡張し、新しい object の op を追加する（テスト側のみ）。peak RSS は `/usr/bin/time -l` で取り、N=0 との差を setup 分として分ける | V1・V2・V4・V11・R4 |
| M-X3 | `B_cold_fetch`（材料の cold 取得）、`B_check`、`B_localize`（二分探索の round 数 × round 時間）、`B_carrier`（carrier の包含遅延: t12 の DAA ペースと block 容量から）、`B_reorg_slack` | court の全 round を fixture class の最大 profile で回して DAA に換算する。t12 のペースは既存の実測（184 s/DAA）を使う | V10 |
| M-ENV | legacy filer の hard 期限 envelope（`start + B_cold_fetch + B_check + B_localize + B_disclose + B_court + B_carrier + … ≤ held to final + window_court`）と、legacy held DA の descent round 数・answer bytes・34 session の予約予算 | g14 の gate requirement の文言どおり（`palw_g14_gates_v1.rs` の `PALW_LEGACY_*_ARMING_REQUIREMENTS_V1`） | legacy filer / held DA の record |
| M-RHO | court の占有の経済性（1 session が DAA あたり占有する capacity と、その費用が攻撃者に課される額） | M-C と court cap から算出し、表で報告 | V11 |
| M-DA | クラスごとの DA 取得量（material bytes / claim）と保持 bytes / DAA | 既存の fixture class と、手元にある実モデル artifact（あれば） | R4・V5 |
| M-L | liveness: 拒否された court / H-1 carrier 1 回あたりの CPU（L-COURT-01: 組み立て部分は M1 Max で 0.72 ns/B 実測済み。裁定部分が未計測）と、object ごとの全 state 複製（L-LC-01/02: claim 行数 1k〜1M） | `origin/liveness/court`（`f74ca87d3`）、`origin/liveness/lifecycle`（`7abbaaed3`） | liveness 修正の優先度、mempool policy の上限 |
| M-Q36 | PALWQ361 の f32 参照窓（35B MoE を f32 で長系列実行。メモリが足りるかを先に確認）と logit scale | `misaka/palw/base0/src/bin/qwen36-window-compare.rs`、`scripts/genesis/genesis10_pipeline.py` の `fit_plan` | R7 |
| M-MEM | RFC-0020: 実 artifact での 8K / 16K / 32K の RAM・RSS・IO・時間・bit 一致・忠実度 | RFC-0020 の計測 driver | RFC-0020 の受入 |
| M-STO | 30 GB ストレージの M1〜M9（`docs/design/palw/storage/capacity.md` §8） | devnet と縮小 cap | 容量予算の調整 |
| M-UPG | RFC-0019 の R7 `model-add`、R8 `local-llm`（実 artifact が必要） | `scripts/drills/rfc0019_rolling_upgrade_drill.py` | RFC-0019 の受入 |

## 4. arming record の候補

対象の gate と、requirement の正本は次のとおりです。

- `consensus/core/src/palw_g14_gates_v1.rs` の 12 fence（`PalwG14GatedFenceV1`）と、各 `PALW_*_ARMING_REQUIREMENTS_V1`
- `consensus/core/src/palw_kernel_route_arming_v1.rs`（kernel route、署名付き登録、provider court）
- `consensus/core/src/palw_rfc0018_v1.rs` の record gate
- RFC-0006 の cell closure の record gate
- `consensus/core/src/palw_rfc0016_v1/values.rs`
- `consensus/core/src/palw_rfc0608_measured_v1.rs`
- `consensus/core/src/palw_tir_frame_cost_v1.rs`

requirement 1 件ごとに、evidence 文書を `docs/evidence/palw/studio-values-2026-10-22/records/<fence>/<n>-<slug>.md` として作ります。中身は、実行した suite・E2E・drill・実測の結果と、その生ログへの参照です。

候補表 `records-candidates.md` には、fence ごとに次の列を並べます。

- network: testnet-12 の genesis hash
- activation_daa: 16000
- source_commit: evidence を作った binary の commit
- evidence: requirement 順に並べた各文書の sha256
- 未充足の requirement と、その理由

実行できない requirement（実ノード E2E や、出荷 binary での drill のうち未実施のもの）は、空欄にして「未充足」と書いてください。埋めた形にしてはいけません。

## 5. 返し方

- push 先: `studio/values` branch。evidence は `docs/evidence/palw/studio-values-2026-10-22/`。
- 中心の成果物は次の 3 つです。
  - `values-proposal.md`: §2 の全値について、案・根拠・依存関係
  - `measurements.md`: §3 の全実測について、結果表・環境・未実行の項目
  - `records-candidates.md`: §4 の候補表
- 最後に `SUMMARY.md` を書きます。中身は次のとおりです。
  - ユーザーが承認すべき値の一覧（そのまま判断できる形）
  - 最低要件ハードウェアでの再計測が必要な項目
  - DAA 16,000 に武装できる fence と、できない fence の区別
- docs を編集したら `python3 docs/maintenance/generate_indexes.py` と `python3 docs/maintenance/check_docs.py` を実行する。
- commit trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
