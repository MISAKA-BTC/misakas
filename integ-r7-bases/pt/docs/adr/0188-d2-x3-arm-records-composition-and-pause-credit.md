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
- **Verification:** 下の「テスト結果」を参照。node-level の REQ-X3-1〜5 の block flow テストと、restart / reorg の replay 同一性は**未実施**（2026-10-18 の再確認でも未着手）。R-D2-COVER は 2026-10-18 に導出して試験を追加した（下の「R-D2-COVER」）。tag 230 の codec は実装せず、Proposed の選択肢を下に書いた。
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

## R-D2-COVER（2026-10-18）— IR / pipeline / evaluation finalize の読み取り集合

**結論:** finalize が読む inventory leaf の正確な集合は、opening 時には導出していない。導出できるのは finalize の評価を実行した後だけである。

- **何が決まるか。** IR の評価では `palw_tir_court_v1` の `param()` が、読んだ byte を持つ inventory leaf を `used.params` に入れる。どの byte を読むかは program と carried step の値で決まる（添字が step のデータに依存する）。よって集合は program だけの関数ではない。
- **opening の検査。** `palw_d2_named_leaf_operands_admit_v1` は `TirParams(None)` と `TirParams(Single)`（multiproof が登録 root に届くこと）を受理し、読み取り集合との一致は見ない。`EvalCone` の params は `CarriedUnchecked` で素通し。
- **厳密な関門。** root claim の finalize の Step 8（`check_canonical`: `used.params.len() != r.params.opened_count()` で拒否、`palw_tir_court_v1.rs`）。不足する leaf は被告が自分の copy から補うしかない。
- **露出の上限。** 被告が補う leaf は、その 1 件の告発の finalize が読む相異なる leaf に限られる。各 leaf は class の inventory の 1 piece 全体で、数は `PalwTirInventoryIndexV1::leaf_count()` で上から抑えられる。挑戦者が公開を怠った場合、この露出は挑戦者の告発の費用になる（棄却された claim では挑戦者が負担）。
- **残余（閉じていない）。** 課題は「被告に model byte を強制しない」の原則と、この露出の関係である。opening で読み取り集合を導出する（評価を一度走らせて検証する）案は未実装。費用は告発 1 件あたり finalize 1 回で、有界。採否は設計判断。
- **試験:** `r_d2_cover_the_opening_does_not_derive_the_finalize_read_set_and_the_finalize_is_the_exact_gate`（`palw_court_model_scope_routes_v1/tests.rs`）が上の 2 点（opening は空の carriage を受理、厳密な関門は finalize の Step 8）を固定する。

## R-D2-READY — tag 230 の codec（Proposed、ユーザー判断待ち）

tag 230 の wire 形は設計判断のため実装しない。選択肢:

- **案 A（per-leaf digest 列）。** 新 object `SeatReadinessDigest { beacon: Hash64, class_id: Hash64, seat: PalwBondKeyV2, span: u64, digests: Vec<(u32 leaf_index, Hash64 digest)> }`。digest は `palw_d2_readiness_digest_v1` の値。borsh。`digests` の長さは draw の上限で有界（上限は policy の名前付き値、未決）。A-2 への影響: kind 表に tag 230 の行を 1 つ追加し、fence 未満は「decode 不能と同じ扱い」（判定せず通す、ADR の既存規則）。fingerprint と A-2 pin が変わるので、pin の再測定が要る。休眠（`palw_court_model_scope_v1` 配下）なので、arm まで網の挙動は変わらない。
- **案 B（Merkle 根 1 個）。** `SeatReadinessRoot { beacon, class_id, seat, span, root: Hash64, leaf_count: u32 }` の 1 個だけを置き、反証は leaf index と Merkle path を伴う。object は小さいが、反証に path の codec（sibling 列の上限と順序の規則）が増える。A-2 への影響は案 A と同じ行数、追加の pin は path codec 分。
- **案 C（既存の readiness proof の tag 47/50/93 を使う）。** 不採用。readiness proof は model bytes を被告に出させる（D2 の目的と反する）。

**ユーザーに決めてほしいこと:** A か B か。反証の 1 件あたりの費用（B は path 分だけ重い）と、digest 列の上限（draw 数）の値。どちらも arm 前の判断で、今は実装しない。

## node-level 試験（2026-10-19、`consensus/src/pipeline/virtual_processor/tests/lg14a_legacy_filer_e2e.rs`）

- **arm の形:** D2 と X3 は test config だけで arm する（`lg14a_config_with(armed, d2, x3)`、`sync_palw_2026_10_12_ruling_fences_v1`）。X3 の policy は `unapproved_probe_at`（probe 値、test 限定）の `resolve_daa` を **composition の値 `palw_claim_pursuit_budget_v1` に置き換える**（下の発見 1）。preset には入れない。
- **PASS:** `x3_req_1_2_3_the_hard_deadline_is_fixed_at_acceptance_on_the_node`（期限は accepted + 合成値、min、後続ブロックで不変）、`x3_req_4_a_pursuit_starts_only_inside_its_budget_on_the_node`（許容 start は accepted から cutoff まで、cutoff+1 以後は reservation なし）、`x3_the_fixed_deadline_survives_a_restart_and_replays`（再起動後も期限・tip・delta 行が一致、fresh node が同じ root に再生）、`d2_a_reorg_undoes_and_returns_the_reservation_exactly`（D2 の reorg、旧 legacy シナリオを D2 arm で実行）。`cargo test -p kaspa-consensus --lib lg14a_legacy_filer_e2e`: **29 passed, 3 ignored**（2026-10-19、commit a9e81e94f の後）。
- **BLOCKED（無視フラグ付きで記録）:**
  - `d2_a_full_collusion_on_a_new_claim_is_convicted_before_final_on_the_node`、`d2_the_pursuit_survives_a_restart`（R-D2-NODE）: 新 claim の held StepLeaf の告発は mempool で拒否される（ADR-0177 D2 の「モデル bytes を強制しない」）。replacement route（`palw_d2_one_move_on_claim_part_v1` など）は node の admission に配線されていない。配線は consensus 経路の実装で、この session の範囲を超える。
  - ~~R-X3-HOLD~~ **解消（2026-10-19、commit 0bc3f67b5）:** 規則の不具合ではなく、試験の前提の誤りだった。X3 では、claim の Final を止めるのは開いている DA session だけである（DL-1 の X3 行は `open_sessions() > 0` のときに期限を None にする）。session のない reservation は、有効期限 `min(提案, hard)` を超えて Final を止めない（REQ-X3-2）。試験を 2 本に分けた。`x3_req_5_reaching_the_deadline_alone_never_finals_an_unresolved_claim_on_the_node` は、reserved の held DA session を開いて producer が答えない場合に、hard を過ぎても Final にならず DA default（Voided）に至ることを確かめる。`x3_req_5b_a_reservation_without_a_session_finals_at_its_effective_deadline` は、session のない reservation の claim が hard 以下で Final になることを確かめる。試験環境では X3 と LG14-B（held DA）を一緒に武装する。`lg14a_legacy_filer_e2e`: **31 passed, 2 ignored**（R-D2-NODE の 2 件）。
- **発見 1（期限の composition）:** probe の `resolve_daa`（200）は `B_disclose + B_court + Δ` より短く、どの pursuit も許可されない（`palw_claim_pursuit_budget_v1` は `window_court` と disclose window を含む）。registers の「X3 と L の窓」の未解決項と同じ。値の再測定が arm の前提。
- **発見 2（held claim の期限）:** X3 を arm すると、node 上の held claim の bind 期限は legacy の `None` ではなく hard に従う（REQ-X3-2 の min）。試験は legacy 値（None）を固定する。

## 残り（この ADR の範囲外、または未実施）

- node-level の BLOCKED 1 件（R-D2-NODE：filer が D2 claim に KernelWitness を要求し、fold が claim-part 経路で裁定する変更が要る）。X3 の reorg 再生（pre-cutoff の reservation と session を使うシナリオは未作成）。
- tag 230 の codec と A-2 kind-table 行（上の選択）。
- X3 の Δ（5 項）と `c_retain` の測定・承認（未決のまま）。
