# ADR-0188 — D2・X3 の休眠 fence を G14 の arm-readiness record で閉じ、X3 の期限を窓の合成で定める

- **Decision:** Accepted — ユーザー決定 2026-10-18 の内容を実装・記録する: X3 の期限は `prepare = window_bind + window_receipt`、`observe = window_challenge_at(activation)`、`resolve = B_disclose + B_court + Δ`（`Δ = B_cold_fetch + B_check + B_localize + B_carrier + B_reorg_slack`）、`hard = accepted + prepare + observe + resolve`。Δ の各項と `c_retain` は policy の名前付き入力で、0 は未測定として拒否する（既定値にしない）。
- **Scope:** `palw_court_model_scope_v1`（D2）と `palw_claim_hard_deadline_v1`（X3）の refusal。X3 の econ 接続（`C_upper` と beacon source の material 項）。X3 の DA pause 返還（`resume_claim_after_da_session_v2`）。D2 の readiness object の tag（230）。
- **Implementation:** branch `codex-free/d2-x3-finish`（起点 `release/t12-16000` 17cbdee9e）。
  - `PalwG14GatedFenceV1::{CourtModelScopeV1, ClaimHardDeadlineV1}` を追加し、無条件 refusal を `validate_palw_g14_fence_record_in_v1` の record gate に置き換えた。`PALW_G14_ARMING_RECORDS_V1` は**空のまま**（record を追加していない）。requirements は `PALW_D2_ARMING_REQUIREMENTS_V1` / `PALW_X3_ARMING_REQUIREMENTS_V1`。
  - X3 は `palw_x3_unmeasured_terms_v1` が空でない policy を record の有無に関係なく refuse する。spans は `palw_x3_composition_refusal_v1` で窓から読んだ合成と一致しなければ refuse する（activation で窓を読み直す）。
  - `palw_x3_composed_policy_v1` で合成 policy を作る。`candidate_from_horizons_v1` は未測定の B_* を 0 のまま残す（未承認の候補）。
  - X3 の claim には DA pause credit を与えない（`palw_x3_pause_credit_v1`）。旧 claim は pause をそのまま返す。
  - econ: `palw_minimum_price_c_upper_x3_v1`（`palw_econ_contracts_v1`）と `PalwBeaconSourceCostBoundsV1::minimum_gross_fee_x3`（`palw_beacon_source_fee_v1`）が retention の価格を `C_upper` と material 項に入れる。`c_retain = 0` で拡張が正なら UNKNOWN として refuse する。
- **Tag:** R-D2-READY は tag **230**（`PALW_A2_TAG_ALLOCATIONS_V1` に 2026-10-15 から予約済みの行）を確定した。kind の codec と A-2 kind-table の行は未着手。
- **Verification:** 下の「テスト結果」を参照。node-level の REQ-X3-1〜5 の block flow テストと、restart / reorg の replay 同一性は**未実施**。
- **Activation:** なし。`PALW_G14_ARMING_RECORDS_V1` は空で、D2 と X3 は全 real network で refuse される。X3 の policy 値（B_cold_fetch・B_check・B_localize・B_carrier・B_reorg_slack・c_retain）は**ユーザー承認と測定が必要で、未決**。
- **Related:** [mainnet-spec-register §3.1.1](../design/palw/integration/mainnet-spec-register-2026-10-12.md)、[RFC-0014 §7.2](../rfc/0014-panel-independent-fraud-prosecution.md)、[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)（D2 の出典）、[ADR-0187](0187-g14-native-public-filing-and-post-final-mining-accusation.md)。

## 背景と決定

1. **arm の形。** 他の G14 fence と同じく、armed の高さで pinned record（network・高さ・evidence・review）が無ければ refuse する。record の一覧は空。record を pin するのは審査された変更だけで、この ADR は値を入れない。
2. **X3 の合成。** 期限を窓と Δ から組む。窓の変更は合成を通じて期限に反映され、activation で `Params` の窓を読み直して食い違えば refuse する。Δ は「測定されるまで 0 にできない」項として扱う。
3. **X3 の pause。** X3 の期限は d0 で固定される（REQ-X3-3）。DA session の pause を返すと claim の anchor が後ろに動き、期限の延長になるため、X3 claim は 0 とする。旧 claim は既存どおり。
4. **R-D2-READY。** tag 230 を確定する。kind 自体は codec を伴うので、この ADR の範囲では未実装。

## 採用理由・代替案

- record gate は既存の G14 fence と同じ経路にした。新しい承認の仕組みを作らず、空の一覧で全 network を refuse し続けられる。
- Δ を `resolve` に含めたのは、REQ-T12-3 の `B_remaining` と合成式を一致させるため。Δ を 0 にして既定に残す案は採らない（未測定を無料とみなすことになる）。
- X3 の pause 返還を credit 0 にしたのは、d0 固定（REQ-X3-3）を優先するため。代替の「pause を上限付きで返す」は、期限延長の上限を別に定める必要があり、この ADR では決めない。

## テスト結果（2026-10-18、`CARGO_INCREMENTAL=0`、`-j 4`、branch `codex-free/d2-x3-finish`）

- `--lib palw_ruling_fences_v1`: 16 件 PASS（新規 3 件を含む: `d2_and_x3_are_refused_without_a_record_and_admitted_exactly_at_the_records_network_and_height`、`x3_refuses_an_unmeasured_policy_term_even_with_a_record`、`x3_spans_are_the_composition_of_the_windows_read_at_activation`）。
- `--lib` の `palw_court_model_scope_routes_v1`: 5 件を含めて PASS。
- `--test palw_ruling_fences_2026_10_12`: 3 件 PASS。`--test t12_repin_values`: 1 件 PASS。
- `--test g14_close_gates`: 12 件 PASS（`testnet12_identity_did_not_move`・`armable_g14_fences_refuse_without_a_measured_record` を含む）。`--test g14_close_legacy`: 8 件 PASS。`--test g14_court_capacity`: 15 件 PASS（1 件 ignored）。
- `--lib` 全体で 8 件が FAIL（4,612 PASS）: `fork_id_v1` の 2 件、`palw_lifecycle_objects_v2::tests::a2u` の 3 件、`palw_native_resource_v1` の 1 件、`palw_state_v2::tests` の 2 件。**base（17cbdee9e）でも同じ 8 件が FAIL（4,609 PASS）を確認済み（2026-10-18）。本ブランチが原因ではないため修正していない。**
- 未実施: node-level（block flow）の REQ-X3-1〜5 試験、restart / reorg 再生の同一性、R-D2-COVER、node-level の D2 full-collusion 試験、R-D2-READY の codec。
- econ（2026-10-18、commit 4cc05d5ec）: `x3_gross_floor_*` 2 件・`c_upper_x3_*` 2 件 PASS（`c_retain` 0 の正の延長は UNKNOWN、overflow・未測定 class も拒否）。
