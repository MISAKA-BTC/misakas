# mainnet対象仕様・未解決事項の台帳 — 2026-10-12

**固定点:** コード・テストは `pre` @ `642344bb9`、文書は同SHAの `docs/`。数値・判定はこのSHAで読んだ結果であり、以後の変更で古くなる。
**目的:** レビュー整理（[rfc-adr-review-resolution-2026-10-12.md](rfc-adr-review-resolution-2026-10-12.md)）のX1〜X5・N1〜N5・W1・W2と、
モデル取得不能・court前の証拠取得・絶対期限・job version・fence名の各論点を、**対象ruleset**に固定し、**採択する要求 ↔ コード ↔ テスト**を一対一で結ぶ。
**状態:** 台帳（記録）。要求の採否のうち「要判断」と書いたものはユーザー判断待ちであり、この文書は実装も有効化もしない。

## 0. 対象ruleset

| 記号 | ruleset | 何が有効か（このSHAのコードで確認） |
| --- | --- | --- |
| **L** | 稼働中のtestnet-12（int-12 release、params `5ee7fd8e…`・schedule `1678e073…`） | `palw_t12_shipped_params()`（`params.rs:15276`）。`palw_t12_arm_every_rule_from_genesis()`（`:23553`）が `never()` 以外の全fenceをDAA 0に、`PALW_T12_INT11_FENCES_V1`（`:22593`、19件）がDAA 5,300（`PALW_T12_INT11_FLAG_DAY_DAA`）に置く。int-13・post-launch V4は `None`。固定テストは `palw_t12_flag_day_int11.rs`。 |
| **F** | full-activation目標（RFC-0008/0010/0011 K2/0012/0014/0015＋旧int-13の4件、[activation-readiness-matrix](activation-readiness-matrix.md)） | すべて休眠fence。多くは `validate_*` が有効化自体を拒否する。armableなfenceは0。 |
| **H** | 撤回・履歴 | 撤回された判断、過去ネットワークの記述。 |

分類: **A** = Lの現行規則同士またはLの挙動と現行文書の矛盾（稼働中のチェーンに関わる）／**B** = F（将来仕様）の中の矛盾・未設計／**C** = 撤回・履歴。

状態は4段階を混ぜない: **規範**（RFC/ADRに書かれている）→ **実装**（コードがある）→ **検証**（このSHAでテストが通る）→ **実証**（本番条件で第三者が実際に検証・訴追した）。**このSHAで「実証」に達した項目はない。**

## 1. 台帳

| ID | 分類 | 採択する要求（案） | 実装 | L | テスト | 欠落・矛盾 |
| --- | --- | --- | --- | --- | --- | --- |
| **X1** | C | （撤回）配布自由 ≠ 無検証支払い。下記N1/N2/T6の3区別に分解済み | — | — | — | 撤回をADR-0177 D1補足に反映済み |
| **X3** 絶対期限 | **A**＋B | REQ-X3-1: claimごとの `dispute_hard_deadline` を受理時に固定する。REQ-X3-2: `effective = min(proposed, hard)`。REQ-X3-3: 延長しない。REQ-X3-4: `start + B_remaining ≤ hard` でなければpursuitを始めない。REQ-X3-5: 期限到達だけで未解決claimを自動Finalにしない。旧claimは旧ruleset、新規則は別versioned fence | `min()`・d0固定・開始予算検査は**なし**。既存: `palw_dispute_hard_deadline_v1`（`palw_legacy_public_filer_v1.rs:243`、休眠） | **延長がLで有効**: `rearm_after_challenger_side_close`（`palw_state_v2.rs:31945`）がheld decoy敗北後に `licensed_daa := now`（窓の再付与、`palw_offence_attribution` DAA 0）。DA session pause credit（`:32108`）。休眠の有料staged courtは `hard.max(new)` で期限自体を後ろへ動かす（`palw_staged_court_fold_v1.rs:333,419`） | 再付与を**肯定**するテスト `a_conceded_held_decoy_leaves_the_seats_a_whole_challenge_window`（`palw_state_v2.rs:71560`）。REQ-X3-1〜5のテストは**なし**（受入D01〜D04も未作成） | RFC-0014 §7.2本文（最初のpursuit受理で固定）と同2026-10-12改定節（d0で固定）が**自己矛盾**。RFC-0003 Decision 22（再付与）と同改定節（再付与禁止）も矛盾。LG14-Aは期限経過の次blockでFinalにする（REQ-X3-5と逆） |
| **N3** Work Beacon bootstrap | B | REQ-N3-1: 通常Beaconとbootstrapを分ける。REQ-N3-2: bootstrap候補は独立検証可能性に加え予測困難性・grinding耐性・reorg再現性を満たす（`IndependentValidityVerified` と `EntropyEligibilityVerified` を分離）。REQ-N3-3: 成立まで新profileはDormant | `palw_opv_bootstrap_v1.rs`（complete-check基底・E1〜E7・依存グラフ）。opt-in source（kernel inner 40）、source fee（inner 61/62）。`BootstrapExact`・entropy適格性の述語は**なし** | 全fence None・有効化拒否 | `g14_opv_bootstrap_*`（node E2E 7本）、`the_graph_reaches_opv_from_genesis_only_through_the_complete_check` ほか。S03（正しいが予測可能な出力にentropy適格を与えない）・S04は**なし** | コードのbootstrapは `OpvFinal`（Panel=0のFinal）経由でしかsourceに届かない。RFC-0007改定（Panel=0に依らない）・RFC-0015改定（Panel=0を有効化しない）と矛盾し、改定どおりなら到達不能。`palw_opv_bootstrap_v1.rs:1` は循環を「閉じた」と書くが整理文書はN3を最優先の未解決とする |
| **N4/X2** プライバシーと公開訴追 | **A**＋B | REQ-N4-1: 必要な証拠を公開できない生成物・画像・source経路を、新規の報酬・weight対象から除外する。REQ-N4-2: 過去の有効claimは旧rulesetで処理し遡及しない | 除外規則は**なし**。非公開経路は実在: FP Job V5（source ID・画像はworkerとPanelのみ）、`PanelDa`。新規報酬ゲート `palw_reward_gate_v1` は休眠でlegacy Panel routeを免除 | **Lで有効**: `palw_gen_v1`・`palw_fp_job_v5` はDAA 5,300、`palw_panel_da` は導出でDAA 0 | 近傍のみ（`g14_rewards_*` 3本、`palw_panel_da` 休眠テストはt12を対象外）。除外を確かめるテストは**なし** | RFC-0003改定（:2613）とRFC-0003 Decision 12・§I.4.3、およびLの挙動が矛盾。**要判断:** Lの非公開経路を新fenceで報酬対象外にするか |
| **N1/N2** モデル保有と独立Panel | **A**（文書） | REQ-N1-1: Panel seatは保有証明（`artifact_root_matches`・`all_chunks_held`）を出す。REQ-N1-2: executor以外の保有operator ≥ `seat_count`、registrant・executorを除く独立operator ≥ `independent_floor`。REQ-N1-3: Miner（executor）も正しい登録モデルを保有する | `palw_class_seating_v1`（`:143,237`）、`PalwReadinessEvidenceV1`（`palw_model_registry_v1.rs:323`）。executor保有の明示検査は**なし**（root束縛と反証のみ） | **Lで有効**: `palw_class_seating` DAA 5,300（floor 3、seat 5）、`palw_admission_independence` DAA 0 | `palw_class_seating_fence.rs` の `seat_1`〜`seat_10` ほか（REQ-N1-1/2に一致）。REQ-N1-3・D7（閉鎖モデルでも独立Panelが検出）のテストは**なし** | 文書が古い: `palw_class_seating_fence_v1.rs:6-10,114`・`params.rs:3233` は「どのpresetでもNone」、RFC-0002 :1579 は「未実装」。RFC-0002 :1757 の「7 operator」は述語（≥ `seat_count`=5）と不一致。Panel=1下の `independent_floor` は未定義 |
| **N5** Panel=0 | B | REQ-N5-1: Panel=0を有効化しない。独立した安全性実証（監視者離脱・共謀・検証費用・最大claim負荷・cold model取得時間・不正発覚からcourtまで）を先に行う。REQ-N5-2: timeoutでPanel=0へ自動遷移しない | `palw_panel_free_v1.rs`（休眠、`validate_*` が実高さを拒否）。fallbackは無く、終了は `BeaconUnavailable`・`SealUnavailable`・`PermissionlessNoCapablePanel`（無課金） | None・拒否 | `rfc0015_every_preset_is_dormant_and_every_real_activation_is_refused` ほか。安全性実証のテストは**なし** | matrix :9-11・:32 はfull-activationでRFC-0015を有効化と書き、同 :330 とRFC-0015改定（:878）は有効化しないと書く。`palw_panel_free_v1.rs:10-13`「consensusは読まない」は誤り（`palw_model_conformance_fold_v2.rs:26` が読む） |
| **T6** モデル取得不能 | **A**＋B | REQ-T6-1: 第三者が取得できなかったことだけでは、claim受理・報酬・Final weightを無効にせず、court開始・void・slash・期間延長も起こさない。REQ-T6-2: 必要なPanelが成立しない場合は非不正の未受理/未Final（無課金）。REQ-T6-3: courtはモデル本体・範囲を要求しない（ADR-0177 D2） | seatの `Incapable`（無罰）、`NoCapablePanel`・`PanelUnavailable`（無課金）、合意は取得しない（`palw_public_model_source_v1.rs`）。新経路のcourt範囲は `palw_court_demand_allowed_v1` | **Lで有効**: `palw_unavailable_abstains`・`palw_audit_2026_09_23`・`palw_public_model_source_required` DAA 0、`palw_panel_unavailable_expiry` DAA 5,300。**モデルbytesを強制するcourt経路4本もLで有効**（`HeldStepLeaf`・`AttnRootClaimOperands`・`TirGenRootClaimParams`・`SeatReadinessLeaves`、`palw_court_scope_v1.rs:478`） | REQ-T6-1/2の近傍: `honest_unavailable_and_incapable_are_zero_penalty`、`t02_bind_timeout_and_no_capable_panel_charge_nothing`、`with_nothing_pending_the_second_panels_silence_expires_panel_unavailable_uncharged`、`g14_canonical_*_a_peer_refusing_the_model_*`。D1補足の区別そのもの（閉鎖モデル→Panel不成立→非不正の未受理・登録者資格は不変）を一本で確かめるテストは**なし** | ADR-0177 D2とLの4経路が矛盾（RFC-0014 §16.4は認め、制限は休眠fence案）。閉鎖モデルはLでseat不成立になり報酬が出ない — D1補足の「Panel不成立」と整合するが、D1本文の文言と緊張（ADR-0177 :48 自身が「要検証」） |
| **T12** court前の証拠取得 | **A**＋B | REQ-T12-1: 欠けた回答は客観的DA/default（計算fraudとして記録しない）。REQ-T12-2: verifierは自分のモデルで検証し、合意はモデルを取得・要求しない。REQ-T12-3: 開始予算 `start + B_cold_fetch + B_check + B_localize + B_disclose + B_court + B_carrier + B_reorg_slack ≤ hard` を満たさないpursuitは始めない | DA demand → reserve（tag 154）→ binding → 認証付き降下 → terminal（`kaspad/src/palw_fraud_filer.rs`）。OPV経路は登録時に最悪pursuitを拒否（`kernel/src/budget.rs:105`）。legacy経路の予約は `now > hard` しか見ない（`palw_legacy_public_filer_fold_v1.rs:449`） | `palw_da_court` DAA 0。filer・staged court・OPVは休眠 | 差し控え→default: `g14_*_withheld_*`（6本）、期限: `g14_*_spam_demands_cannot_hold_final_*` ほか。「verifierが窓内にモデルを取れず、チェーンは何も反応しない」テストは**なし** | REQ-T12-3はlegacy経路に**なし**。`palw_legacy_public_filer_v1.rs:203`「期限は動かない」とstaged courtの `max()` 延長が矛盾 |
| **W1** 整数seed | **A**（文書） | **要判断:** (a) gatewayは整数seedを拒否する（現行コード）か、(b) `le64(n+1) ‖ 0^24` に写し、overflowを拒否する（RFC-0001改定） | `le64(n+1)` は**なし**。chatは64桁hexのみ受け付け整数を拒否（`gateway/src/main.rs:1176`）、画像も64桁hex、VLM・tensorは固定seed | gateway側（fenceなし） | 整数42の拒否を確かめる `identity_valued_knobs_are_accepted_…_refused_by_name`（`surface.rs:1428`）。写像のテストは**なし** | RFC-0003が自己矛盾: §I.1.3（:299）`le64(n)`、§I.0（:200）と表（:1096）`le64(n+1)`。RFC-0003改定節はW1を含まず :299 が残る |
| **W2** text規則 | **A**（文書） | REQ-W2-1: FP V4自体のtext・stop規則は変えない。REQ-W2-2: TextStream pipelineに追加規則を適用し、その規則を列挙する | V4: stop列の上限（4本×1〜16 id）・最初のstopで止める（`palw_decode_pipeline_v4.rs`）。TextStream側の規則は既に3つ: NF-P9′（出力stageのみ・1本、`tir/core/src/pipeline.rs:550`）、Q24 unit規則（IR・生成classに温度等を出さない、`palw_freeprompt_v3.rs:1835`）、強制prefix（`palw_fp_job_v5.rs:315`） | `palw_fp_decode_rules`・`palw_fp_job_v5`・`palw_gen_v1` DAA 5,300 | V4: `stop_sequences_are_bounded_non_empty_and_sorted` ほか4本、TextStream: `only_the_output_stage_may_be_the_text_stage`、Q24: `a_class_that_does_not_commit_q24_logits_is_offered_only_the_unit_free_controls`、V5がV4を変えない: `palw_fp_v5_leaves_v4_unchanged.rs` | 改定は追加規則を名指ししない一方、RFC-0003 PALW-GEN-10は「text規則を追加しない」。Q24規則はpipeline層でなくV4の検査内でclass種別により適用。RFC-0001 :90 の「後のfenceでIR classに全controlを与える」fenceは存在しない |
| **X4** FP Job V6 | **A** | REQ-X4-1: 状態を「実装済み・**Lで有効（DAA 5,300）**・court未完了」と記録する。REQ-X4-2: 未完了＝nodeが直接証明を生成しない、位置別公開openingの取得、100,000 byte直接証拠上限を超える大jobの多分割terminal、モデル/表関係・最大profile・自動producer・restart/reorg/pruning | job version 6（`PALW_FP_CONSTRAINT_VERSION`、`palw_fp_constraint_job_v1.rs:63`）。court tag 17・offence tag 14/15（後者は休眠の `palw_legacy_public_filer_v1` が要る）、`render_answer_v2` | **Lで有効**: `palw_fp_decode_constraint` DAA 5,300（`params.rs:22609`）。稼働ログにも「ordinary FP Job V6 admission」 | `the_doors_admit_the_constrained_claim_only_where_and_when_the_fence_says` ほか。どれも合成状態で述語を検査し、nodeの直接証明生成は検証していない | X4・RFC-0001 §7.1（:392「どのpresetでもarmしない」）の「dormant」はコードと**矛盾**（Lで有効）。整理文書のX4自体を訂正する |
| **X5** fence名 | C（矛盾ではない） | REQ-X5-1: 設計上のfence識別子は `Params::palw_fences_v1()` の名前を正とし、文書はその名前で書く | 登録簿 `palw_fences_v1()`（`params.rs:10376`）。173組を照合し、名前とフィールドの不一致は `palw_dns_retirement_v1` ↔ `palw_dns_retirement` の1件のみ（意図どおり） | — | `rfc0012_fork_id_and_cost.rs`、`fork_id_v1::set_fence_for_probe` がずれを名前で落とす | matrixに載らないfence: `palw_legacy_public_filer_v1`・`palw_staged_public_court_v1`（RFC-0014のコードfence）、rule Eは名前（`palw_fork_choice_rule_e_v1`）でなく「rule E」と表記。RFC-0014 :23 の `palw_dispute_complete_v1` は未割当。`palw_reward_gate_v1` はfenceではなく関数 |
| **T11** job version | **A**（文書） | REQ-T11-1: job versionごとに「入口fence・Lでの状態・court/判定コード」を一表で持ち、文書・コメントはそれに従う | 下表 | 下表 | `palw_t12_flag_day_int11.rs`、`a_decode_close_does_not_try_a_claim_accepted_past_the_decode_rules`（`palw_court_v2.rs:4349`） | `params.rs` の4か所（`palw_fp_job_v5` :2945、`palw_fp_prefix_state` :3104、`palw_fp_prefix_inherit` :3095、`palw_fp_decode_constraint` :2847）が「どのpresetでもNone」と書くが、4件ともLでDAA 5,300。gateway :1130、`palw_step_refute.rs:3417` も古い |

### job version（Lの状態、このSHA）

| wire version | job | 入口fence | L | court・判定 |
| --- | --- | --- | --- | --- |
| 5 | FP V3 | genesis | DAA 5,300で新規V3 commitmentを拒否 | `DecodeToken`・`DecodeTokenTiled`・`Arithmetic*`・`AttnDissection`・seat replay |
| 7 | FP V4 | `palw_fp_decode_rules` | 5,300 | fence後に受理したFP claimのdecode-token closeは**拒否**。選択はseat replayとarithmetic closeのみが判定 |
| 6 | FP V6（制約付き） | `palw_fp_decode_constraint` | 5,300 | tag 17、offence tag 14/15（X4） |
| 8 | FP V5（VLM） | `palw_fp_job_v5` | 5,300 | `GenCone`・`GenDecodeToken`・`GenDissection` |
| 9 | 評価 | `palw_improvement_v1` | 5,300 | `EvalCone`・`EvalDecodeToken`・`EvalDissection` |
| 10 | 生成tensor | `palw_gen_v1` | 5,300 | `GenCone`・`GenDissection`・`GenOutputTile` |
| 11 | prefix state | `palw_fp_prefix_state` | 5,300 | seat検証、有料prefix court |
| 12 | prefix inherit | `palw_fp_prefix_inherit` | 5,300 | 11と同じ＋class安全規則 |

## 2. 要求 ↔ テスト（一対一）

「既存」は要求そのものを確かめるテスト、「近傍」は隣の挙動だけ、「未作成」は無い。

| 要求 | テスト | 状態 |
| --- | --- | --- |
| REQ-X3-1 受理時固定 | — | 未作成 |
| REQ-X3-2 `min(proposed, hard)` | — | 未作成 |
| REQ-X3-3 延長禁止 | `a_conceded_held_decoy_leaves_the_seats_a_whole_challenge_window` は**逆**を固定 | 未作成（新fence側に作る。L側は旧規則として残す） |
| REQ-X3-4 開始予算 | — | 未作成 |
| REQ-X3-5 期限だけで自動Finalにしない | `lg14a_a_reservation_holds_final_to_the_hard_deadline_then_lapses_…` は逆 | 未作成 |
| REQ-N3-1/2/3 | `g14_opv_bootstrap_*`（Panel=0経由） | 近傍。S03・S04 未作成 |
| REQ-N4-1 非公開経路の報酬除外 | — | 未作成（**要判断**） |
| REQ-N4-2 遡及しない | — | 未作成 |
| REQ-N1-1/2 | `palw_class_seating_fence.rs` seat_1〜seat_10 | 既存 |
| REQ-N1-3 executor保有 | — | 未作成 |
| REQ-N5-1/2 | `rfc0015_every_preset_is_dormant_…`、`panel_unavailable_redraws_…_expires_uncharged` | 既存（有効化拒否まで）。安全性実証は未作成 |
| REQ-T6-1 | `honest_unavailable_and_incapable_are_zero_penalty`、`g14_canonical_*_a_peer_refusing_the_model_*` | 近傍 |
| REQ-T6-2 | `t02_bind_timeout_and_no_capable_panel_charge_nothing`、`with_nothing_pending_…_panel_unavailable_uncharged` | 既存 |
| REQ-T6-1+2 の区別そのもの | — | 未作成 |
| REQ-T6-3 モデルbytesを要求しない | `the_inventory_compels_no_model_bytes_on_the_dormant_routes_and_names_the_armed_ones` | 既存（L の4経路を例外として固定） |
| REQ-T12-1 | `g14_*_withheld_*`（6本） | 既存 |
| REQ-T12-2 | `network_unavailability_cannot_change_the_verdict` | 近傍 |
| REQ-T12-3 開始予算（legacy経路） | — | 未作成（REQ-X3-4と同じ検査） |
| W1 | 整数拒否: `identity_valued_knobs_…`（`surface.rs:1428`） | (a)を採るなら既存、(b)なら未作成 |
| REQ-W2-1 | `palw_fp_v5_leaves_v4_unchanged.rs`、V4 stop系4本 | 既存 |
| REQ-W2-2 | NF-P9′・Q24・強制prefixの各テスト | 既存（規則の列挙が文書に未記載） |
| REQ-X4-2 nodeの直接証明生成 | — | 未作成 |
| REQ-X5-1 | `set_fence_for_probe`、`rfc0012_fork_id_and_cost.rs` | 既存 |
| REQ-T11-1 | `palw_t12_flag_day_int11.rs` | 既存 |

## 3. 判断（ユーザー、2026-10-12 決定）

決定内容は [レビュー整理](rfc-adr-review-resolution-2026-10-12.md) の「ユーザー判断 — 2026-10-12」に記録した。要約: X3 は新fenceのみで延長禁止（固定時点はd0）、N4 は新fence以後の非公開経路を新規報酬・weight対象外、D2 の4経路は休眠の制限fenceで閉じる、W1 は整数seed拒否のまま、N3 は成立まで新profile休眠。これにより REQ-X3-1〜5・REQ-N4-1 は「新fence側の要求」、REQ-T6-3 は「制限fence側の要求」、W1 は既存テスト `identity_valued_knobs_…` で充足、REQ-N3-3 は現状（全fence None・拒否）で充足となる。新fence（X3・N4・D2制限）の名前と実装は未着手（2026-10-12時点。2026-10-15に休眠の未検証ソースを作成 — §3.1）。

以下は判断前の論点（記録として残す）:

1. **X3:** Lで有効な窓の再付与（`rearm_after_challenger_side_close`）と有料staged courtの期限延長を、旧rulesetとして残し、新しいversioned fenceでのみ禁止するか。RFC-0014 §7.2本文と改定節のどちらの固定時点（最初のpursuit受理 / d0）を採るか。
2. **N4:** Lで有効な非公開経路（V5の画像・source ID、`PanelDa`）を、新fenceで新規報酬・weightの対象外にするか。
3. **T6/ADR-0177 D2:** Lで有効なモデルbytes強制のcourt経路4本を、休眠の制限fenceで閉じるか（RFC-0014 §16.4の案）。
4. **W1:** gatewayの整数seedを拒否のままにするか、`le64(n+1)` に写すか（どちらでもRFC-0003の :299 を直す）。
5. **N3:** bootstrapをPanel=0（`OpvFinal`）に依らずに成立させる設計。成立までは新profileを休眠に置く（REQ-N3-3）。

### 3.1 2026-10-15 — X3・N4・D2 制限の休眠fence（未検証のソース）

§3の判断を受け、3つのfenceの規則を**休眠のソース**として作成した。**ビルド・テストは未実行**で、PASSを主張しない。3つともどのpresetでも `None`、有効化は `validate_palw_2026_10_12_ruling_fences_v1` が拒否する（2026-10-12の骨組みcommit `3b9ed5ede` の名前をそのまま使う）。fence未満では各hookが何もしない（claimの受理DAA、readinessはclassの登録DAAで判定）ので、Lの状態root・判定は変わらない設計である（A-2: `PALW_A2_KIND_FENCE_TABLE_V1` に3行、`t12_a2u_new_kinds_uniform.rs` のnode Cで遠い高さに武装）。

| fence | 規則（中心: `consensus/core/src/palw_ruling_fences_v1.rs`） | fold側のhook | REQテスト（`palw_ruling_fences_v1/tests.rs`） |
|---|---|---|---|
| `palw_claim_hard_deadline_v1`（X3） | d0固定: `receipt = d0+T_prepare`、`open_cutoff = receipt+T_observe`、`hard = min(open_cutoff+T_resolve, trace_retention_daa)`。`effective = min(proposed, hard)`。`B_remaining = B_cold_fetch+B_check+B_localize+B_disclose+B_court+B_carrier+B_reorg_slack`（`B_disclose`=DA開示窓、`B_court`=`window_court`、他5項とT_*はfenceのpolicy項目で**本番値なし**、probe値のみ）。fenceの型は `Option<PalwClaimHardDeadlineFenceV1>` に変更し、policy idを両fingerprintにSome-onlyで入れた。他agent向けhelper: `palw_claim_hard_deadline_of_v1`・`palw_claim_effective_deadline_v1`・`palw_claim_pursuit_may_start_v1` | DL-1のlicensed行でmin、X3 claimは開いたDA session中はFinal期限なし（`palw_rcore_deadline_v1`、session開始でdisarm・最後のcloseで再導出）。延長禁止: held decoy再付与（`rearm_after_challenger_side_close`）、DA pause credit・V6 credit、staged courtの2つの延長、model court窓のretention延長。開始予算: reservation（tag 154）とnamed-leaf dissection。lapse sweepはDA sessionが開いた予約を残す | `req_x3_1_the_hard_deadline_is_fixed_at_acceptance`、`req_x3_2_the_effective_deadline_is_min_of_proposed_and_hard`、`req_x3_3_no_extension_or_regrant_for_a_new_claim_and_the_old_rules_stay_for_old_claims`、`req_x3_4_a_pursuit_starts_only_inside_its_budget`、`req_x3_5_reaching_the_deadline_alone_never_finals_an_unresolved_claim` |
| `palw_public_evidence_reward_v1`（N4） | fence以後に受理するFP claimで、証拠を公開できない経路（`PanelDa`、FP Job V5の画像・source ID）は新規の報酬・Final weightを得ない — G14-for-rewardsと同じくclaim gateで受理しない（code `N4_NONPUBLIC_EVIDENCE`）。判定は受理時だけ | `palw_state_v2/objects.rs` の `FreePromptCommitted` 腕（評価claimの分岐の後） | `req_n4_1_a_nonpublic_evidence_route_earns_no_new_reward_or_weight`、`req_n4_2_claims_accepted_before_the_fence_keep_the_old_ruleset` |
| `palw_court_model_scope_v1`（D2制限） | fence以後に受理したclaimでは、Lで有効な4経路（`HeldStepLeaf`・`AttnRootClaimOperands`・`TirGenRootClaimParams`・`SeatReadinessLeaves`）を認めない。旧claimは維持 | held `StepLeaf` DA要求を拒否（`open_da_session_rcore_mode_v1`）、named-leaf dissection（tag 38/62/88/90経由）を開かない、readiness証明（tag 47/50/93）はfence以後に登録したclassだけ拒否 | `req_t6_3_restricted_the_armed_model_byte_routes_close_for_new_claims_only` |

未解決（有効化前に要判断・要実装）:

- **X3のpolicy値**（T_prepare/T_observe/T_resolveと5つのB項）は未承認。`B_localize` はclass/plan別のenvelopeにすべきだが、今は単一値。
- **X3とLの窓の関係**: `hard` がretention（bind+receipt+challenge+court）で頭打ちのため、policy次第でlicence後のpursuitがほぼ開始できない。T_*とretention義務の関係（retentionをhardまで延ばす要件にするか）は設計判断が要る。遅いlicenceは `min()` により観測窓が短くなる（RFC-0014改定節どおり）。`open_cutoff` 前のFinalは禁止していない（REQ-X3-2の字義どおり）。
- **D2の代替経路は未構築**: dissectionのroot claimを座標のみにし挑戦者がopeningを出す経路（DA16 design §7.5 1–2）がないため、fence以後の新claimでone-moveで決まらないsiteは訴追できない（G14の穴）。readinessの制限はclass単位で、旧classの新claimはreadinessを使い続ける。
- **N4**: 生成tensor claim（tag 87）は現状weightlessなので対象外。将来weightを与えるfenceは同じ述語を使う必要がある。
- R-core+未満のADR-0062経路（`resume_claim_after_da_session_v2`）のpause返却にはX3のguardを入れていない（t12はR-core+をgenesisで武装）。

#### 3.1.1 2026-10-15（round 3）— D2代替経路・N4のtag 87・X3のretention延長（未検証のソース）

上の未解決4項目のうち3つに休眠のソースを追加した。**ビルド・テストは未実行**で、PASSを主張しない。新しいobject種別・fence・tag・state tableは追加していない（既存3 fenceの裏側。fence未満・`None` presetではどのhookも読まれず、A-2の表とharnessは変更不要）。

- **D2の代替経路（REQ-T6-4、`consensus/core/src/palw_court_model_scope_routes_v1.rs`）**: 制限した4 unitごとに、公開の認証済み材料だけで有罪にする経路を置いた（`PALW_D2_REPLACEMENT_ROUTES_V1`）。
  - `HeldStepLeaf` → claim part のみ（LG14-Bの `KernelWitness`、または `palw_legacy_held_da_v2` 以後の `StepLeaf` 回答。rowなし、沈黙はDA default）＋告発者自身のrow → one-move（`palw_d2_one_move_on_claim_part_v1` = `palw_one_move_verdict_bound_v2`）またはtag 159。claim partがrowを運ぶと拒否。
  - `AttnRootClaimOperands`・`TirGenRootClaimParams` → **座標のみに署名するroot claim＋挑戦者が公開したoperandの中継**。被告の署名は元から座標とclaim値だけを覆う（attention: `palw_attn_root_claim_message_v1(session, root)`、IR: `(version, elements, totals)`）。fence以後の新claimでは、named-leaf dissectionを開くobjectがroot claimの必要とするmodel operandを**公開**していなければ開かない（`open_dissection_at_named_leaf_v1` → `palw_d2_named_leaf_operands_admit_v1`。attentionは告発のrowからsiteが導出できること、IR/pipelineは運ばれたparamが登録rootで認証できること、tag 90はdigestのみなので新claimでは開かない）。被告は自分のコピーを読まずに、chain上の公開bytesを中継してroot claimを出せる（`palw_d2_attn_root_claim_relayed_v1`・`palw_d2_tir_root_claim_relayed_v1`）。dissectionとbottomの判定は既存のまま。
  - `SeatReadinessLeaves` → 抽選leafごとのkeyed digest（`palw_d2_readiness_digest_v1`、bytesは出さない）を、モデル保有者が自分の認証済みopeningで反証（`palw_d2_readiness_refute_v1`）。**object種別は未割当**（R-D2-READY、Leadがtagを割り当てる）。
  - テスト（`palw_court_model_scope_routes_v1/tests.rs`）: `every_restricted_unit_has_a_replacement_route_that_compels_no_model_bytes`（棚卸し）、`req_t6_4_full_collusion_on_a_new_claim_is_convicted_by_the_replacement_route`（producer＋Panel全員共謀の新claimを、Panel外の公開bonded verifierが公開材料と自分のrowで有罪にする。非fused・fusedの不正tile、正直なfusedはdissectionへ、公開operandのない・認証できないopeningは開かない）、`a_cone_opening_publishes_authenticated_params_or_states_it_reads_none`、`the_root_claim_signatures_cover_coordinates_only_so_a_relay_keeps_them`、`a_readiness_digest_is_refuted_only_by_the_registered_bytes`。
  - 残り（R-D2-COVER）: IR/pipeline/evaluationのfinalizeが読むparamの**正確な集合**はopening時に導出していない。挑戦者の公開が足りなければ被告が不足分を補うことになる（告発1件あたりfinalize 1回分の読み取りに有界、claimが正直なら挑戦者のexposureで支払い）。composite artifactとevaluation closeのparamはfinalizeが認証する。
- **N4のtag 87（REQ-N4-3）**: tensor claimは現状weightless（quanta・pwu・escrow・rights・immatureすべて0）なので拒否は起きない。報酬・weightを与えるなら必ず通る唯一の書き込み地点（`palw_gen_claim_fold_v1` のclaim生成）に、`palw_claim_earns_reward_or_weight_v1(&claim) && palw_public_evidence_tensor_refusal_v1(...)` のguardを置いた（`PanelDa`・画像入力は非公開、code `N4_NONPUBLIC_EVIDENCE`）。テスト `req_n4_3_a_weightless_tensor_claim_is_guarded_where_weight_would_be_granted`。
- **X3とretention（Lead既定、REQ-X3-6）**: retentionを**hardまで延長**する側に決めた。`hard = d0 + T_prepare + T_observe + T_resolve`（retentionでの頭打ちを削除、`palw_x3_hard_deadline_at_v1`）。X3 claimは生成時に一度だけ `trace_retention_daa = max(trace_retention_daa, hard)`（`TransitionBuilder::write_claim`、REQ-X3-3の「受理後の延長」ではなくd0固定の一部）。これでclaim trace・DA material unit・recordを読む全holdがhardまで義務になる。paid consumerのmaterial holdは `material_until ≥ hard` を要求（`palw_bond_budget_fold_v1/consumer.rs`）。追加retentionの費用はpolicyの名前付き項目 `retention_cost_per_daa_sompi`（`c_retain`、**本番値なし**、probe値1）で、`palw_x3_retention_cost_sompi_v1 = extra_daa × c_retain`（0は未測定として拒否）、`palw_x3_material_term_with_retention_v1 = material + extra_daa × c_retain` を `C_upper` のclass cost（`palw_minimum_price_c_upper_v1`）と `PalwBeaconSourceCostBoundsV1.material_sompi` に入れる入力とした（econ側の呼び出しは未接続、econ laneの担当）。テスト `req_x3_6_retention_extends_to_the_hard_deadline_and_is_priced`、`req_x3_1` は頭打ち削除に合わせて更新。monetary material（`PalwMonetaryMaterialPolicyV1`、`misaka/palw/remote`）の保持期間をhardまで伸ばす接続は未実装（provider lane）。

## 4. 文書・コメントの訂正一覧（実装の挙動を変えない）— 2026-10-12 実施済み

`palw_panel_free_v1.rs:174` の有効化拒否メッセージ（「no palw_bond_budget_v1 engine」）は実行時文字列のため残した（`palw_bond_budget_v1` 自体は休眠で存在する）。

コードがLで有効にしているのに「休眠」「どのpresetでもNone」と書く箇所。訂正はコメント・文書のみで、fingerprintも挙動も動かさない。

- `consensus/core/src/config/params.rs`: :1900（`palw_public_model_source_required`）、:2584（`palw_panel_unavailable_expiry`）、:2847-2852（`palw_fp_decode_constraint`、tag 17実装済みも反映）、:2925-2937（`palw_gen_v1`）、:2945（`palw_fp_job_v5`）、:3095（`palw_fp_prefix_inherit`）、:3104（`palw_fp_prefix_state`）、:3233-3242（`palw_class_seating`）
- `consensus/core/src/palw_class_seating_fence_v1.rs:6-10,114`、`palw_fp_constraint_job_v1.rs:36-37`、`palw_fp_prefix_v1.rs:1`、`palw_freeprompt_v3.rs:58`・`palw_fp_job_v5.rs:61`・`palw_gen_claim_v1.rs:52`（version 6「未構築」）、`palw_panel_free_v1.rs:10-13`、`:174`（`palw_bond_budget_v1` は存在する）、`palw_step_refute.rs:3417-3422`
- `misaka/palw/gateway/src/main.rs:1130`
- RFC-0001 §7.1（:392）・§7.3、整理文書のX4、RFC-0002 :1579・:1757、RFC-0003 :299（W1の判断後）、activation-readiness-matrix :9-11・:32・:99-100

## 5. 準備表の扱い

- [mainnet-readiness.md](../../mainnet-readiness.md) の各行は「最終確認 2026-09-25 / `0e8ec984e`」のままで、冒頭の2026-10-10の設計注記は別基準である。これを単一の「現在のPASS表」として読まない。
- 今後は行ごとに **検証SHA** と **テスト実行への参照**（CI run または `cargo test` の対象とSHA）を付け、ページ全体の更新日だけで状態を表さない。
- [activation-readiness-matrix.md](activation-readiness-matrix.md) の Implemented / Verified / Armable はこの台帳の 実装 / 検証 と同じ意味で使い、「実証」（本番条件での第三者による検証・訴追）を別の段として加える。
