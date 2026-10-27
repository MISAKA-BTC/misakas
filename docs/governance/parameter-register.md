# パラメーター台帳 — 数値変更1件(2026-10-22 記録: 平均期間 30→7 日)

初期台帳は RFC-0016 / 0017 と ADR-0176 / 0177 / 0179 の主要値です。他領域の全変数を網羅したとはしません。未掲載値は出典・条件・承認を登録してから調整します。これは node 設定ではありません。[機械可読版](parameters.json)

| キー / 変数 | 基準値 | 単位 | 状態・出典 |
| --- | --- | --- | --- |
| `alpha` / α | 1.5 | 無次元 | 確定設計値。今回変更なし。[出典](../adr/0177-model-bond-allocation-without-availability-consensus.md) |
| `average` / N_7（旧 N_30） | 7日相当 DAA（旧 30日相当） | DAA | **変更記録**: 2026-10-16 ユーザー決定、2026-10-22 ユーザー再確認。根拠: 通常 court 窓 DAA 3,120 ≈ 7 日に対応し、不正時に対話が始まった claim だけ DAA 10,000 まで延びる。実装 `PALW_ALLOCATION_AVERAGE_DAYS_V1 = 7`（consensus/core/src/palw_bond_budget_v1/allocation_v3.rs）。試験: allocation_v3 の既存 policy 検査。[出典](../rfc/0015-panel-free-permissionless-verification.md) |
| `hold_panel0` / W_hold(Panel=0) | 7日相当 DAA | DAA | 設計基準。今回変更なし。[出典](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) |
| `hold_panel1` / W_hold(Panel=1) | 1日相当 DAA | DAA | 後続改定の設計基準。今回変更なし。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `tier_approved` / c_m 指定階層 | 1.0 | 係数 | 現在の設計値。今回変更なし。[出典](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| `tier_registered` / c_m 登録階層 | 0.3 | 係数 | 固定設計値。今回変更なし。[出典](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| `registered_tier_cap` / κ_registered | 300‰ | 総枠比率 | **2026-10-22 pin**（ユーザー委任）。分母は縮めた後の ΣA。fence は未武装。[pin](../evidence/palw/studio-values-2026-10-22/pin-2026-10-22.md)。定義の出典は [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| `sigma` / σ | 未確定 | 倍率 | 測定で決定。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `ticket_tilt` / k（補助 ticket） | 未確定 | 無次元 | 測定・導入範囲確認。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `reissue_start` / φ / pace margin | 未確定 | 割合 / 無次元 | 測定で決定。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `bond_caps` / Q/B/R/F・予約・burst | 導出対象 | 各予算の単位 | 組合せ評価・本番値未確定。[出典](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) |
| `entrant` / ε | 0（tier fence下） | 総枠比率 | 旧審査廃止に伴う決定。[出典](../rfc/0017-model-eligibility-examination-and-renewal.md) |

## 調整しても外さない条件

**α:** 超線形配分という構造を廃止する変更は通常調整ではない。

**N_7（旧 N_30）:** 立上り・短期投入・退出後の履歴を評価。通常 court 窓 3,120 DAA との対応を保つ。

**W_hold(Panel=0):** 共通履歴・残存責任・burst を維持。

**W_hold(Panel=1):** 一律7日と混同せず、最早再利用と担保解放を分離。

**c_m 指定階層:** 二階層・identity 不変を維持。

**c_m 登録階層:** 自動昇格なし。制度の廃止は設計変更。

**κ_registered:** 2026-10-22 の pin は、分母を縮めた後の ΣA にする（登録資本で正規 tier の発行を薄めない）。その結果、縮めた分は正規 tier の取り分として発行される。ADR-0179 §5 の「縮小分を発行しない」とは両立しない。ADR 本文はこの pin では改定していない。κ の撤廃と、測定による登録モデルの昇格は通常調整でない。fence は未武装。

**σ:** 総権利・個別cap・登録tier上限を迂回しない。

**k（補助 ticket）:** DAGや認証の k と区別。導入自体を数値のみで承認しない。

**φ / pace margin:** 時計・他モデルの残り権利を保護。

**Q/B/R/F・予約・burst:** 同額元本・同mode・同期間の共通上限と責任を維持。

**ε:** 旧審査用枠の復活は設計変更。

RFC-0016 の旧 carry 上限 κ と RFC-0017 の登録総枠 κ は別の量です。台帳上で区別し、コード名を今回変更しません。

調整には旧新値、単位、対象、根拠、影響、試験、承認、適用条件を残します。既存の固定値も自動的に自由変更可能とはしません。[凍結規程](design-freeze.md)

画像・動画・モデル改善の `DISABLED` は [リリース方針](post-mainnet-scope.md) であり、単なる変数調整として自由に true にできる設定ではありません。

## 2026-10-18 RFC12 private consumer service の修正（private 専用・ユーザー承認済み）

- キー: `rfc12_private_paid_consumer_service_span_daa`。単位 DAA。対象は feature-gated private paid-v3/v4 factory のみ。
- 出典: `kaspad/src/private_integration/paid_claim_factory.rs`。旧値 1,000。提案は `max(1,000, window_bind + window_receipt + window_challenge_at(activation) + 1)`、現在の private T12 では **1,321**。dispute tail 3,000 と material tail 2,000 は維持。
- 根拠: 3 ノードとも最初の PoW block `dd87311a8ae561c5…` を `paid consumer service cannot fit this class's whole minimum path` で拒否。既存 ruleset の直接読取で bind=600、receipt 下限=600、challenge=120（`/tmp/misaka-rfc12-floor-clock-inspection.log`）。旧値は最短経路にも不足。
- 構造・影響: consensus の最短経路 guard は維持し、private service を延長する。class ごとに長い derived receipt が必要な場合は従来どおり planner が拒否する。material の総期間は 6,000 → 6,321 DAA。private parameter/policy fingerprint が変わるため次の検証は fresh factory/appdir を使用する。public preset、T12 公開 identity、認証、lottery、責任 H は変更しない。
- 実装・試験: review 用の変更を作成。private exact-policy / stale-mirror / public refusal 試験は **1/1 pass**（`/tmp/misaka-rfc12-private-service-derived-floor-stack64-controls.log`、1.93 s）。最初の cargo test は 16m02s のビルド後に既定 test-thread stack 不足で SIGABRT（`/tmp/misaka-rfc12-private-service-derived-floor-tests.log`）。同一バイナリを `RUST_MIN_STACK=67108864`、1 test thread で実行し検証した。修正 profile の live run は未実施。
- 承認: **2026-10-18 ユーザーが「private 専用の変更を承認する」と明示回答**。この private-only 数値修正と、新指紋・fresh DB による再試験を承認。公開・経済・security policy の採択は含まない。live run は承認後の再ビルド完了を待つ。

## 2026-10-22 R7 PALWQ361 の logit scale と f32 参照窓（目安・未実測）

- 承認: **ユーザー判断 2026-10-22（目安で可）**。理由: 35B MoE の f32 長系列が 32 GiB の機材に収まらない。状態はすべて **目安（未実測）**。導出・入力・余裕・保証しないこと: [r7-palwq361-estimate.md](../evidence/palw/studio-values-2026-10-22/r7-palwq361-estimate.md)。
- 対象は genesis #8（PALWQ361）の変換品質 fit（`scripts/genesis/genesis10_pipeline.py --palwq36-estimated-scale`）だけ。合意上の値ではない。実測の経路（`--palwq36-logit-scale` と 32K の参照窓）が優先で、そちらは変更していない。

| キー / 変数 | 値 | 単位 | 状態・出典 |
| --- | --- | --- | --- |
| `palwq361_logit_scale_estimate` / 2^-e_logits | 2^-9 = 0.001953125（e_logits = 9。較正 logit absmax A ∈ (16, 32] を仮定） | 物理 logit / 整数 code | 目安（未実測）。PALWQ361 では code = logit·2^e_logits が作りで決まるので、未知なのは A の 2 の冪の区間だけ。[導出 §1](../evidence/palw/studio-values-2026-10-22/r7-palwq361-estimate.md) |
| `palwq361_reference_prefix_tokens` / L | 2,048（上限 4,096） | token | 目安（未実測）。`qwen36-convert --reference-only --prompt` の f32 参照を held-out 列の先頭 L で使う。2 × L × 0.99 MB の見積りで 32 GiB に収まる。[導出 §2](../evidence/palw/studio-values-2026-10-22/r7-palwq361-estimate.md) |
| `palwq361_estimate_scale_spread_log2_max` | 0.25 | log2 | 目安（未実測）。窓の spread と目安が 2^±0.25 を超えて離れていれば拒否する（目安の外れも head の 2 の冪のずれも、通らずに止まる）。[導出 §1.3](../evidence/palw/studio-values-2026-10-22/r7-palwq361-estimate.md) |
| `palwq361_estimate_kl_margin` | KL 閾値 ×0.5、late/early ≤ 1.25、top-1 ≥ 0.8 | 倍率 / 比 | 目安（未実測）。prefix の外の 32K drift は比べていないことへの余裕。A16 の基準より緩い項目はない。[導出 §2.2](../evidence/palw/studio-values-2026-10-22/r7-palwq361-estimate.md) |

**外さない条件:** fit は `measured: False, estimated: True` として記録し、card には「ESTIMATE」と書く。spread に合わせて scale を選び直すこと（snap）はしない。spread の上限を 0.5 以上にする変更と、KL の基準を A16 より緩める変更は、通常の調整ではない。upload gate の例外は、この承認文字列と PALWQ361 に限る。

## 2026-10-22 P3・P4 の値の pin（fence は未武装）

- 承認: **ユーザー委任 2026-10-22**（値の決定と、目安で pin してよいこと）。機械可読の行は [parameters.json](parameters.json)。導出・監査・再計算: [pin-2026-10-22.md](../evidence/palw/studio-values-2026-10-22/pin-2026-10-22.md)。
- この節は台帳への pin である。`PALW_T12_FULL_ACTIVATION_DAA` は `None` のまま。arming record の一覧も空のまま。理由は [fence-readiness-2026-10-22.md](../evidence/palw/studio-values-2026-10-22/fence-readiness-2026-10-22.md)。
- 出荷 preset はこれらの値を読まない。コード上の置き場は `consensus/core/src/palw_values_p4_v1.rs` と、P3 が既存定数へ入れた値である。

### 決定値（再計算で一致。武装は別）

| キー | 値 | 単位 | 状態 |
| --- | --- | --- | --- |
| `post_final_default_slash` | `ClaimReserve`（その claim の回収可能な reserve） | 規則 | 決定。V1 profile が pin されるまで fold は休眠 |
| `d2_readiness_draw_cap` | 64 | 座標 | 決定。予約・料金移動・slash 配線が済むまで D2 は武装しない |
| `d2_disclosure_fee_sompi` | 1,300,000 | sompi / demand | 決定。fold はまだ料金を移動しない |
| `d2_conviction_slash_sompi` | 300,000,000（3 MSK） | sompi / root | 決定。適用する fold は無い |
| `tir_position_witness_budget_bytes` | 83,333 | B | 決定。chunk 1 part |
| `tir_lookahead_max_frontier_values` | 3,072 | 値 | 決定 |
| `staged_court_collection_daa` | 7,104 | DAA | 決定 |
| `staged_court_reorg_retention_daa` | 360 | DAA | 決定。t12 finality depth |
| `model_allocation_epoch_daa` | 720 | DAA | 決定。t12 bundle の epoch 1,000 とは V+ を組めない（ADJUST） |
| `model_allocation_seasoning_epochs` | 1 | epoch | 決定 |
| `model_allocation_max_models_per_bond` | 16 | 件 | 決定。安全上の上限ではない |
| `model_allocation_block_opportunity` | V+ と同じ高さ以下なら false、そうでなければ true | 真理値 | ADJUST。今の `validate_v3` は false を拒否し、V+ は true を拒否する。同時武装はコード変更が要る |
| `cert_m` / `cert_k` | 75 / 41 | 席 | 決定。前提 f = 250‰。この pin で二項分布を再計算し、誤認証の年間期待 7.87e-4、応答率 90% で認証率 0.9926 |
| `cert_premise_adversary_permille` | 250 | ‰ | 決定（前提）。崩れると上の確率は保証しない |
| `paid_claim_bootstrap_seed_domain` | `MISAKA/PALW/PAID-CLAIM-BOOTSTRAP-SEED/NETWORK/V1` | domain | 決定。seed は network 名と genesis から導出。承認済み profile には入れない |
| `material_session_min_providers` | 2 | provider | 決定。独立性の保証ではない |
| `budget_window_daa` | 720 | DAA | 決定（値）。court clock v2 の claim（最大 10,000）では `L ≤ W_budget` は不成立。単独では武装しない |
| `wallet_retirement_margin_daa` | 1,000 | DAA | 決定（t12、client）。名目 120 s では約 33 時間 |
| `legacy_evidence_horizon_daa` | network の `dns_params.evidence_window_blocks`（t12 は 10,080、mainnet は 12,096,000、devnet は 300） | DAA / block | ADJUST。単一の 10,080 を全 network に置かない |

### 目安（未実測。方針はあとから変える）

いずれも実測 record ではない。`PALW_RFC0016_MEASURED_RECORDS_V1`、`PALW_RFC0608_MEASURED_RECORDS_V1`、`PALW_TIR_FRAME_COST_RECORDS_V1`、`PALW_G14_ARMING_RECORDS_V1` は空のまま。beacon の k / D / delay / window / retry は承認一覧に入れていない。

| キー | 目安 | 見直しのきっかけ |
| --- | --- | --- |
| `bond_budget_capital_unit_kas` | 13,000 KAS | 対象 ruleset の最小 producer bond |
| `bond_budget_daily_per_unit` | 72（claim・reward block・Round rights） | ADR-0176 Stage 1 |
| `bond_budget_export_cap_permille` | 250 | Stage 1 の S/K |
| `bond_budget_liability_hold_daa` | 10,000 | X3 の実測 Δ |
| `bond_budget_final_weight_per_unit` | 4.0e13（旧 1,000）**目安** | 最重 claim の Final weight × 72 × 4 が動いたとき（新 class・weight 尺度変更）、または ADR-0176 Stage 1 実測 |
| `bond_budget_max_open_claims_per_bond` | 288（旧 72）**目安** | 実測ピーク open × 4 を超えたとき |
| `source_seal_deposit_kas` | 28.6 | beacon と受付料が pin されたとき |
| `source_max_concurrent_attempts` | 3 | 遅延の価値 ν |
| `work_beacon_k` / `settlement_depth_daa` / `anchor_delay_slots` / `window_slots` / `retry_limit` | 3 / 200 / 2 / 40 / 2 | 外部の bias review。承認ではない |
| `rfc0006_draw_retry` / `seats` / `outsiders` / `retained` | 2 / 1 / 1 / 4 | 実 sharded class |
| `rfc0006_worst_cell_bytes` / `verify_us` / `shard_court_sessions` | 33,629 / 134,000 / 42 | release ハードウェア |
| `rfc0016_sigma_permille` / `pace_margin_permille` / `capacity_safety_permille` | 250 / 1,200 / 500 | RFC-0016 §3.6 |
| `rfc0016_epsilon_permille` | 0 | tier fence と同じ高さ以下のときだけ。そうでなければ未決定 |
| `x3_b_cold_fetch_daa` / `b_check` / `b_localize` / `b_carrier` / `b_reorg_slack` | 512 / 512 / 512 / 120 / 360 | P1 の M-X3 と最低要件ハードウェア。0 にしない |
| `x3_c_retain_sompi_per_daa` | 36,058 | M-DA と V5 |

**外さない条件:** 目安を measured record や arming record の evidence digest として扱わない。0 の X3 項は採らない。D2 の slash 定数を、適用 fold が無いのに作動中の罰則と読まない。

## 2026-10-26 lane VAL — V1〜V5・V11 の目安（未実測・未配線、fence は未武装）

- 承認: **ユーザー委任 2026-10-22**（値の決定、目安で決めてよい・後で方針変更あり）。状態はすべて **目安（未実測）**。実測 record・arming record には入れていない。V6〜V10・V12 と cert m/k は 2026-10-22 の pin（上の節）のままで、この節では決め直していない。
- コード: `consensus/core/src/palw_g14_values_v1.rs`（値の集約、どの preset も読まない）。本番の置き場（`PALW_COURT_SERVICE_PROFILE_V1`、`palw_escrow_fee_schedule_v1`、`palw_court_ingress_schedule_v1`、`material_intake_schedule_v1`、funded original の schedule、`palw_court_capacity_v1`）は typed-missing のまま。配線は統括の統合時。
- 導出・経済監査: [lane-values-report.md](../design/palw/integration/g14-17000/lane-values-report.md)。再計算: [g14_values_v1_check.py](../design/palw/integration/g14-17000/g14_values_v1_check.py)。
- 共通の入力: 運搬単価 50 sompi/B（t12 の 49.96 を切上げ、P3 V7b）、最大の署名付き court object 33,821 B・session weight 13（P1 M-C/M-RHO、M1 Max 負荷下。最低要件ハードウェアではない）、block 125,000 B/DAA、court cap 42・turn 42 DAA、紛争窓 10,000 DAA、Panel=1 の責任署名者 2（producer＋seat）。

| # | キー | 目安 | 単位 | 根拠（式と入力） | 見直し条件・必要な実測 |
| --- | --- | --- | --- | --- | --- |
| V1 | `court_service.packet_cost` | bytes 34,000 / work 13 / io 1 / controls 1 | packet | 最大 object 33,821 B を 1 packet に収める | 実モデルの最大 court object（M-C 実モデル） |
| V1 | `court_service.budget` | **1 packet/DAA**（34,000 B・work 13・io 1・controls 1）。旧目安 2 packet（2026-10-26 統括判断で変更） | /DAA | 1 original の 24 packet は 833 DAA の各区間に 2 個。12 区間は a+9,997 で終わり、区間 0〜3 が notice a+3,120 を覆う。超過は受付で全拒否され、期限超過にはならない。持続 1/24 original/DAA（名目 30/日）、block の 27.2% | 受付需要が 30/日を超えたら 2 に戻す |
| V1 | `court_service.packets_per_signer` | 12 | packet | localization 11 turn（upper 6）＋close 1 | 実モデルの turn 数（step leaf 数で増える） |
| V1 | `court_service.service_span_daa` | 833 | DAA | ⌊(10,000−4)/12⌋、`1+12×833 = 9,997 ≤ 10,000` | packets_per_signer の変更時 |
| V2 | `funded.fee_sompi` | 81,600,000（0.816 MSK） | sompi | 2 × 24 packet × 34,000 B × 50 | V1 の変更時 |
| V2 | `funded.capital_floor_sompi` / `capital_per_turn` | 81,600,000 / 3,400,000 | sompi | turn ごとに 1 packet 運搬の 2 倍、24 turn | 同上 |
| V2 | `funded.capital_per_source_node` | 1,300,000 | sompi | D2 開示料と同じ（応答 12,672 B 運搬の 2 倍） | M-DA の source node 実測 |
| V2 | `funded.heap/retention_bytes_per_branch` | 65,536 / 65,536 | B | TIR prepared retained 最大 59,494 B（fixture） | 実モデル M-DA |
| V2 | numerical / cover / source graph 上限 | 4,096 leaves 等（module 参照） | 件 | 資源上限のみ。安全性の根拠ではない | 実モデルの graph 規模 |
| V3 | `escrow.shares_permille` | work 800 / relay 100 / court 0 / burn 100 | ‰ of F | 自己支払いの claim は F の 10% を失う（補助金なし）。court 料は opener 払い（V4/V11） | relay の実費、外部 review |
| V4 | `court_ingress.fee_sompi_per_session` | 1,100,000 | sompi | opening 16,571 B × 50 = 828,550 を切上げ。内部者（producer・seat）も払う。R = ⌊cap/2⌋ = 21 を維持 | ρ_court の変更時 |
| V5 | `material_intake.sompi_per_byte` / payer | 50 / submitter | sompi/B | 運搬単価。P3 `c_retain` = ⌈2,250,000×50/3,120⌉ = 36,058 と一致 | 保持費の実価格（M-DA） |
| V5 | `material_intake.draft_ttl_daa` | 720 | DAA | 名目 1 日 | intake の実測完了時間 |
| F03k | `kernel_panel_one_external_gain_bound` | 500 MSK（5e10 sompi） | sompi | 嘘の claim が通った場合の利得: claim_reward 5 MSK（escrow から）＋ V2 fee 0.816 MSK ＋ 結果の利用者への協調した誤誘導の余地 約 494 MSK。比較する検閲費用は出荷中の t12 ledger: `dismissed_proof_fee` 0.1 MSK × `max_adjudications_per_block` 64 = 6.4 MSK/block、`liability_daa` 200 で 1,280 MSK（bound の 2.56 倍）。1 件の Panel=1 claim に 500 MSK を超える価値を賭ける利用者は保証の外 | ledger の料金・上限・責任期間の変更、誤誘導の価値がこれを超える証拠。**目安（2026-10-22 委任）。ユーザーへの明示承認依頼を推奨** |
| F02 | `follow_up_branches_per_turn` | 2 | 分岐/turn | 二分 dissection の follow-up はどちらの半分にも進めるので両方を予約。Q = 2 署名者 × 12 turn × 2 = 48 ≤ c·W = 1 × 9,999。833 DAA 区間あたり 4 slot。持続 1/48 original/DAA（名目 15/日） | 実モデル class の dissection の分岐数。**ユーザー承認済みの目安（2026-10-26）、実測で置換** |
| F02 | `court_service_inclusion.inclusion_delay_daa` | 4 | DAA | B_carrier: ⌈34,000 / 最悪空き 13,051⌉ = 3 DAA ＋ 1 packet/DAA 予約で同区間の 2 packet 目が 1 DAA 待つ = 4（既存 `PALW_COURT_CLOSE_INCLUSION_MARGIN` と同じ）。184 s/DAA で約 12 分 | 混雑下の court packet 取り込み遅延の実測。**目安で pin、実測で置換**。型は 9803e043d、本 lane の base には無いので数値のみ |
| V11 | `court_capacity.rho_sompi` | 3,125（旧目安 6,250） | sompi/DAA/unit | ⌈34,000×50/42/13⌉ = 3,114 を切上げ。全 slot を 10,000 DAA 占有 = 171 MSK。正直な起訴（ingress＋8 move×42 DAA 占有＋運搬）0.28 MSK ≤ 最小 slash 3 MSK の 49% = 1.47 MSK | 最低要件ハードウェアの court op 実測（arming 要件 2） |
| V11 | `court_capacity.session_weight` / op_work | 13 / [5, 13, 1, 11] | unit | P1 M-RHO（中央値ベース、単位 126.5 µs） | 同上。**目安で pin、実測で置換** |

**外さない条件:** これらを measured record・arming record の evidence にしない。外部者の保証は予約 slot（ADR-0192）と期限順 queue に依る。ρ を producer bond 相当（≥ 238,096）にすると正直な起訴が約 10 MSK になり費用回収できないので採らない。V10（X3）・V11・P1-5 は目安で pin、実測で置換。cert m/k は 75/41 のまま。4,096/日の上限は抽選 position の数で、上限まで使っても認証は ⌊4,096/75⌋ = 54 件/日、誤認証の年間期待は 7.9e-4 < 1e-3（2026-10-22 pin のとおり）。lane VAL の最初の報告で「上限まで使うと 0.06/年」と書いたのは、position の数を claim の数として掛けた誤りで、撤回する。この誤りを根拠にした 89/50 への変更は実施していない（統括の再判断待ち）。

**production constructor（2026-10-26）:** `PalwFundedOriginalScheduleV1::new_v1`、`FundedAdmissionScheduleV1::new_v1`、`PalwEscrowFeeScheduleV1::new_v1` を追加した（範囲外は拒否）。目安は `palw_g14_values_v1` の `palw_g14_funded_original_schedule_estimate_v1`・`palw_g14_funded_admission_estimate_v1`・`palw_g14_escrow_schedule_estimate_v1` で組む。preset と `palw_escrow_fee_schedule_v1` は未配線のまま。

## 2026-10-26 lane VAL — `palw_bond_budget_v1` の承認 policy を目安で pin（未実測、preset は未変更）

- 承認: **ユーザー委任 2026-10-22**（値の決定と目安の使用）。統括の指示（slice の settlement once の前提）で、2026-10-22 V12-b の policy（`palw_values_p4_v1::palw_bond_budget_policy_provisional_v1`）の digest を `palw_bond_budget_approved_policy_digests_v1` に入れた。**目安であり実測ではない。**
- 根拠: capital unit 13,000 KAS（t12 最小 producer bond）、1 unit・1 日あたり claim・reward block・Round rights 各 72（名目 10 slot に 1、184 s/DAA で 1 日の block の約 15%）、1 日の reward は unit の 0.25（回収可能 slash 0.51 未満）、export cap 250‰、liability hold 10,000 DAA（court clock v2 の上限）。
- 見直し条件: ADR-0176 Stage 1（参照 miner の正直な速度、実測 S/K）、X3 の実測 Δ。値を変えれば digest が変わり、承認一覧の更新が要る。
- 範囲: preset には入れていない（fence 高さは統括の統合時）。`palw_model_bond_allocation_v1` は slice（`PALW_T12_KERNEL_SLICE_FENCES_V1`）に model tier が無いので、承認一覧は空のまま。

## 2026-10-26 lane CERT — kernel route の認証失敗時の保留報酬（**暫定**、統括の値決定委任）

| 項目 | 名前 | 値 | 単位 | 根拠 | 見直し条件 |
|---|---|---|---|---|---|
| CERT | `palw_kernel_uncertified_split_v1`（NotCertified・Expired・NoPopulation・Withdrawn・Final 後 conviction） | producer 0 / burn = V3 burn 100‰（切捨て）/ 残りを poster へ返却。burn + refund = 報酬 | ‰ of 保留報酬 | 全額返却だと自己支払い claim が認証抽選を無料で引ける。全額 burn だと正直な claim がまれに落ちたとき（約 0.7%）支払者の負担が重すぎる | m/k の確定（VAL・ユーザー承認）、正直 claim の実測失敗率、V3 burn の変更 |

## 2026-10-26 ユーザー承認（beacon policy・Gate C m/k・値の目安）

- ユーザー承認 2026-10-26（原文）:「本番の beacon policy（k/D）と m/k 75/41 を承認していただくこと。値の目安（V1〜V5、V11、包含遅延など）をそのまま使うことも了承　これは承認する」。
- **本番 Panel beacon policy — ユーザー承認 2026-10-26、目安から**: `palw_panel_beacon_v1::palw_panel_beacon_policy_approved_v1`、`approved_panel_beacon_policies_v1` に 1 件。k = 3（f = 0.25 で全 source 敵対の確率 1.6%）、D = 200 DAA（t12 の静穏時 finality 深さ 7〜10 h = 140〜200 DAA の上端）、anchor delay 2・window 40（econ B12 の interim shape）、retry 2（最後の寄与者の選択肢 ≤ 3）、repetition 1、security_bits 128。形は paid opt-in source（v5、route が有効なら seal-reveal v6）で、source は承認済み paid claim（2026-10-17 決定）。最初の承認済み paid Final までは固定 bootstrap seed（EXEC credit なし）。外部の bias review（RFC-0007 §VI.8 item 2、RFC-0010 P0-10）は未実施。見直し: その review、または finality 深さの実測。
- **本番 Gate C policy — m/k = 75/41 ユーザー承認 2026-10-26、目安から**: `PalwSecurityCertificationPolicyV1::approved_v1(day_daa)`（他の項は RFC-0018 の設計値）。f = 0.25 で誤認証 3.99e-8/claim、4,096 position 上限で 54 件/日・年 7.9e-4、正直認証 0.9926（応答 90%）。e2e の TEST_ONLY policy はこれに置き換えられる。
- **V1〜V5、V11、inclusion_delay_daa — ユーザー承認済みの目安 2026-10-26、実測で置換**: 値は「2026-10-26 lane VAL」節（V1 は 1 packet/DAA、V11 ρ_court 3,125、inclusion_delay_daa 4 DAA を含む改定後の値）。measured record・arming record には入れない。
- 範囲: どれも本番 preset には入れていない（slice の武装は統括の統合時）。panel v3 の fence は未武装なので、approved 一覧が consensus に効くのは panel v3 か slice が武装されてから。

## 2026-10-26 lane FPSEL — TIR public frame の費用 `PALW_G14_TIR_FRAME_COST_ESTIMATE_V1`（**目安**、未実測の箇所あり、record・preset には入れない）

| 項目 | 値 | 根拠 | 見直し条件 |
|---|---|---|---|
| clock `action_parser_steps` / `_per_custodian` / `frame_passes_per_check` / `control_bytes` | 15,625,804 / 18,575 / 2 / 14,320 | 2026-10-16 candidate（`docs/design/palw/evidence/tir-frame-measure-2026-10-16/`）。M1 Max の release build、小さな corpus class での実測 | 最遅の対応 node で release build、genesis 規模の class で `frame_measure.rs` と fold harness を再実測 |
| `parallel_bytes_per_daa` | 125,000 | t12 の block の上限（他の traffic なし）。network 入力で、観測していない | V5/V6 drill で混雑時の frame の取り分を観測 |
| `inclusion_margin_daa` / `reorg_replay_moves` / `acquisition_daa` / `collection_daa` | 4 / 1 / 1 / 8 | candidate のとおり | V7 restart・reorg drill の観測、clock v2 の dispute 延長条件との整合を確認 |
| `reorg_retention_daa` | **10,000**（candidate は 256、10-26 案は 360） | 10-27 修正: TIR original は受理時に F02 court service binding を張り、その packet は a+10,000（DISPUTE_WINDOW_DAA）まで続く。frame の署名済み custody（collect + これ）が覆わないと全 original が拒否される。t12 の finality 深さ 360 も覆う | F02 profile の窓の変更、custody 延長による seat bond 拘束・rent の実測 |
| `maximum_door_daa` | 3,120 | 責任期間 `window_challenge_at` 120 + `window_court` 3,000（16,000 fence で読んだ値） | 武装する fence での horizon を validation で再確認 |

`PALW_TIR_FRAME_COST_RECORDS_V1` は空のままにする。この目安だけでは `palw_tir_public_conviction_v1` を武装できない（`NoRecord`）。

## 2026-10-26 lane CLOCK — X3 を kernel route の claim に適用（fence は未武装、preset は未変更）
- **X3 の値 — ユーザー承認の目安（2026-10-22「パラメーターは目安で決めていい」）、実測で置換**: slice の X3 entry（`PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY`）は試験用 probe（hard = a+300）を入れず、`palw_x3_decided_policy_v1`（窓の合成 + Δ 512/512/512/120/360 + `c_retain` 36,058 sompi/DAA）を入れる。Codex の M-X3 実測が来たら `PALW_X3_DECIDED_DELTA_TERMS_V1` を置き換える。measured record・arming record には入れない（X3 の G14 arming record は空のまま）。
- **kernel route の時計**: X3 の高さ以降に commit された kernel claim の `liability_until` の下限を `committed + 3,120 + court_deadline (20) + proof_grace (10)` とする（`misaka_palw_kernel::claim_clock`、consumer 注入、root 外）。ADR-0198 の notice a+3,120（inclusive）と hard a+10,000 の内側。報酬保留・signer lock・reservation はこの horizon に従う。
- **M-1 飽和費用の不等式（X3 以降の Panel=1 class 登録条件）**: `runs × dismissal_fee × max(check+challenge+liability, 3,150) > claim_collateral + claim_reward + 外部利得上限`。interim policy で `64 × 0.1 MSK × 3,150 = 20,160 MSK > 1,000 + 5 + 500 = 1,505 MSK`（13.4 倍）。外部利得上限 500 MSK はユーザー承認（2026-10-26）。前提: DAA あたり 1 block（最悪側）、fee の市場は未 pin（第 1 部 MAJOR-2）。告発者・claim ごとの却下 4 件の上限（`palw_provider_court_v1` の court scope）を slice に追加。
- 見直し: M-X3 実測、DAA あたり block 数の実測、mempool の fee 優先順位。

## 2026-10-26 lane PANELDA — PanelDa 必須公開の目安（未実測・未配線、fence `palw_panel_da_disclosure_v1` は未武装）

ADR-0206(proposed)。値は `consensus/core/src/palw_g14_values_v1.rs` 末尾。すべて目安。

| 値 | 目安 | 根拠 | 見直し条件 |
|---|---|---|---|
| `PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1` | 1,040 DAA | notice 窓 3,120 の 1/3。外部者に 2,080 DAA が残る | notice 窓・turn 期限・外部者 replay 実測 |
| `PALW_G14_PANEL_DA_OUTSIDER_MIN_WINDOW_DAA_V1` | 2,000 DAA | 包含遅延 4+最大 class replay+turn 42 の 2 倍を大きく上回る | 外部者 replay 実測 |
| `PALW_G14_PANEL_DA_MAX_DISCLOSED_IDS_V1` | 32,768 | t12 最大 8K class に余裕 | 長 context class の提供 |
| `PALW_G14_PANEL_DA_DISCLOSURE_DEPOSIT_SOMPI_V1` | 13,107,200 sompi | 2 × 32,768 × 4 B × 50 sompi/B | 料率・最大 ids |

## 2026-10-27 lane AUDIT-FIX — 独立監査 AUDIT-1〜3 の指摘への記録（**目安**、未実測、fence・preset は変更なし）

承認: ユーザー委任 2026-10-22（目安で決めてよい）。どの値も measured record・arming record に入れない。

### AUDIT-1 M5 — Panel=1 seat/sample seed の grinding 上限（目安）

- 前提: seat/sample seed は commit の後に記録された最初の anchor（SEED 648544c46、`draw_panel_one_seats_v1`）。Gate C は m/k = 75/41、f = 0.25 で誤認証 p = 3.99e-8/抽選（2026-10-26 承認）。
- **攻撃者が得る選択肢:** (a) 最初の anchor を含む block を自分で出せた場合に公開するか保留するか。保留 1 回ごとに次の anchor が seed になり、選択肢が 1 増える。beacon policy の retry 2 と同じ扱いで、1 claim あたりの選択肢を c ≤ 3 とみる（保留を続けると block 報酬を失い、DAG の他者の anchor が先に入るので、f = 0.25 の採掘割合では 3 回連続で最初の anchor を握る確率は 0.25³ ≈ 1.6%）。(b) claim を出し直す（別の claim id → 別の seed）。試行回数 N。
- **弱まり方:** 1 claim あたり c·p ≤ 3 × 3.99e-8 = 1.2e-7。N 回の試行で ≤ N·c·p。成功確率 1/2 には N ≈ 4.2e6 回が要る。
- **費用:** 1 試行あたり少なくとも job fee 1 MSK ＋ V2 funded fee 0.816 MSK ＋ 認証失敗時の保留報酬 burn（V3 burn 100‰ × 5 MSK = 0.5 MSK）≈ 2.3 MSK。1 試行の期待利得は 500 MSK × 1.2e-7 = 6e-5 MSK で、費用の約 1/38,000。さらに認証は ⌊4,096/75⌋ = 54 件/日が上限なので、4.2e6 回には約 210 年かかる。保留する block の報酬はこれに加算される。
- **結論（目安）:** grinding は Gate C の上限を最大 3 倍（c）弱めるが、年間の誤認証期待は 7.9e-4 から ≤ 2.4e-3 にとどまり、経済的に引き合わない。
- **見直し条件:** beacon の retry・anchor delay の変更、1 claim あたりの費用（job fee・V2 fee・V3 burn）の低下、認証上限 4,096 position/日の引上げ、攻撃者の採掘割合が 0.25 を超える証拠、seed の取り方の変更（commit 後最初の anchor 以外）。

### AUDIT-3 m2 — PanelDa の結果は Final 前には保証されない

- PanelDa claim は公開期限 a + 1,040 までに prompt が公開されないと、Final の時点で `PanelDaDisclosureDefault` として void になる（報酬なし、deposit は burn）。よって **Final 前の PanelDa の結果は保証されない**。利用者・決済は Final（かつ SecurityCertified）だけを根拠にする。
- **deposit（13,107,200 sompi ≈ 0.131 MSK）が外部利得上限 500 MSK よりはるかに小さい理由:** 未公開の claim は Final に達しないので、保留で得られる利得がない。deposit は計算の誤りへの担保ではなく、公開データの運搬費（2 × 32,768 ids × 4 B × 50 sompi/B）を裏付けるだけ。嘘の計算への抑止は claim collateral と court（下の m3）に依る。
- 見直し条件: PanelDa の結果を Final 前に使う経路が追加された場合（その時は deposit を利得上限に合わせる必要がある）。

### AUDIT-3 m3 — 検出時の損失 ≥ 500 MSK × (1 / 検出率) の確認

- 検出時の損失 L（kernel route interim policy、`palw_kernel_route_v1.rs` の `palw_kernel_route_policy_v1`）: claim collateral 1,000 MSK の没収 ＋ 保留報酬 5 MSK ＋ V2 fee 0.816 MSK ≈ 1,005.8 MSK（総額）。告発者に 49%（ADR-0032）が渡るので、自分で告発する攻撃者の正味損失は 1,000 × 0.51 + 5 + 0.816 ≈ 515.8 MSK。Panel seat の bond の slash 額は kernel route では確認できず、ここでは 0 として数える（下限）。
- 必要額 500 / d: d = 0.90 → 555.6 MSK、d = 0.99 → 505.1 MSK。
- **判定:** 総額 1,005.8 MSK は 90〜99% の全域で満たす。**自己告発の正味 515.8 MSK では d ≥ 96.9% でのみ満たし、d = 90% では 39.8 MSK 不足する。** seat bond の slash（正味）が 40 MSK 以上あれば不足は埋まる。統括へ報告済み（値の変更はしていない）。
- 見直し条件: claim collateral・報告者取り分・外部利得上限・検出率の目安の変更、seat bond slash の額の確定。

### AUDIT-3 m1 — PanelDa deposit の burn は `slashed` に数えられる

- 公開 default の deposit は `burn_registration_fee` で burn され、bond の `slashed` を増やす（`palw_panel_da_disclosure_fold_v1.rs` の `settle_panel_da_at_final_v1`）。**これは有罪ではない**（`PalwVoidReasonV2::PanelDaDisclosureDefault`、strike なし、計算 bond は slash しない）。
- state に field を足さないため、表示は変えずに文書化する: RPC・台帳の `slashed` は「burn の総額」であり、有罪の合計として読まない。有罪の数は conviction の記録（court の verdict）から数える。

### 2026-10-27 ユーザー決定「担保を上げる」— kernel route の claim collateral 1,000 → **1,080 MSK**（目安）

- 定数: `PALW_G14_KERNEL_CLAIM_COLLATERAL_SOMPI_ESTIMATE_V1` = 1,080 MSK、本番 `PALW_G14_KERNEL_CLAIM_COLLATERAL_SOMPI_V1` がそれを指す（500 MSK pin と同じ形）。`palw_kernel_route_policy_v1` の `claim_collateral` が読む。
- 根拠: 自己告発の正味損失 0.51·C + 5 + 0.816 ≥ 500 / 0.90 = 555.6 MSK → C ≥ 1,078.0 MSK。指示の +60（1,060）では正味 546.4 MSK で不足するため、+80 の 1,080（正味 556.6 MSK）とした。99% では 505.1 MSK 以上で満たす。試験 `palw_kernel_route_detection_loss_covers_gain_at_90_percent`。
- 影響: kernel route の policy にだけ入る。fleet identity は fence の高さだけを hash し（`config/params/identity.rs` の `palw_kernel_route_panel_one_v1`）、policy 値は入らないので t12 の指紋は変わらない。ADR-0176 の到達上限（capital unit 13,000 MSK）は読まない。M-1 検閲費用の比は 20,160 / 1,585 = 12.7 倍（13.4 倍から低下、不等式は成立）。
- 見直し条件: 外部利得上限・報告者取り分・検出率の目安の変更、conviction 時の seat bond slash の確定（確定すれば下げられる）。

### 今後の修正（17,000 では行わない）— kernel route の FileProof 枠

- 現状: block あたり 64 run のうち FileProof だけが使える予約 32 run があり、block ごとに戻る。block の中では先着順で、外部者の FileProof に席は保証されない。junk の却下料は 0.1 MSK（作業量に比例）で filer 負担。全枠を責任期間 3,150 DAA 埋める費用は約 20,160 MSK で、賭け金の 12.7 倍。経済的な抑止は成立するので、構造的な修正は 17,000 では行わない（統括判断 2026-10-27）。
- 今後の修正案: `misaka/palw/kernel/src/ledger.rs` の `charged_v1`（:710）で、`liability_until` まで N DAA 以内の claim への FileProof に予約の一部を割り当てる。または :1719〜1721 で期限の早い順に予約を使う。

## 2026-10-27 lane G14-FIX SPEC-1 — kernel route の court admission schedule を本番に pin（**目安**、未実測）

- 定数: `PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1`（`consensus/core/src/palw_kernel_court_admission_v1.rs`）を `None` から `Some` に。version 1、profile = `PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1`（12 turn/signer、span 833 DAA、予算 1 packet/DAA）、`follow_up_branches_per_turn` = `PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1` = 2、`capital_per_packet_sompi` = `PALW_G14_CAPITAL_PER_TURN_SOMPI_V1` = 3,400,000（1 packet の carriage の 2 倍）。
- 根拠: すべて `palw_g14_values_v1` の承認済み目安（2026-10-26）から導出。新しい値は決めていない。Panel=1 の original は 2 signer × (1 + 11 × 2) = 46 packet を予約し、`c·W` = 9,999 の内側。court capital は 46 × 0.034 = 1.564 MSK を producer の空き collateral から引く（kernel claim collateral 1,080 → 1,100 MSK の約 0.14 %）。claim の B = F = 0、escrow は poster 払いのまま（補助金なし、escrow clip なし）。
- 武装: `palw_kernel_route_panel_one_v1`（G14 list、t12 では 17,400）より下では processor が extras に `None` を渡す（`PalwStateParamsV2::kernel_court_admission_schedule_v1(daa)`）。admission fold も同じ fence を再確認する。fence の下では block は以前と byte 単位で同一。fleet identity は fence の高さだけを hash するので、t12 の指紋は変わらない。
- 影響: producer の空き collateral が 1.564 MSK に満たない場合、その kernel-route original は admission で拒否される（全か無か）。
- 見直し条件: court の実測の分岐数が 2 と異なる場合、packet サイズ・carriage 単価・V1 profile が変わる場合、capital 不足による admission 拒否が実測で現れた場合。
- 試験: `spec1_production_schedule_is_gated_on_the_slice_fence`、`val_production_sites_take_the_user_approved_estimates`、spine の S1/S2/S3c''' を test override なしで実行。

## 2026-10-28 lane GC-DUTY — `palw_bond_budget_v1` の F と open claim 上限を実測規模へ（目安）

- 承認: 統括（Lead）2026-10-28、ユーザー委任 2026-10-22（目安で決め後から変更）。原則: ADR-0176 2026-10-15「枠不足で REAL が止まらない」。
- 問題: `final_weight_per_unit = 1,000` は機構尺度の仮置きで、claim の Final weight（実測 1.389e11）より約 10^8 小さく、17,400 で budget を武装すると自分の attempt がすべて `UnsettledWeight` で skip され、seed anchor・Panel=1 の席・Final が起きない（harness で確認）。
- 実測（testnet-12 DAA 16,693、読み取りのみ `getPalwCapacityShadow` / `getPalwClaims`）: 重い class の Final weight 1.389e11/claim（中央値 = p99 = 最大）、軽い class 4.33e6。1 bond の open claim ピーク 45（100,000 KAS bond、未確定 weight 合計 6.25e12）。accept→Final 中央値 236 DAA・最大 292。escrow 3,200.8 KAS/claim。
- 値（**目安**）: F = 4 × 72 × 1.39e11 ≈ **4.0e13** / unit、`max_open_claims_per_bond` = 4 × 72 = **288**。偽造で Q/B/R/F が増えない性質は不変（上限は資本に線形、weight は chain の値）。承認 digest は provisional policy から再計算される（`palw_bond_budget_approved_policy_digests_v1`）。
- **未解決（ユーザー判断待ち）: R。** 上限 0.25 × 担保/日（100,000 KAS bond で約 25,000 KAS/日）に対し、正直な REAL は約 88 claim/日 × 3,200 KAS ≈ 282,000 KAS/日。R を上げると「1 日分の偽造が回収可能 slash 0.51 を超えない」設計上限を破る。
- 見直し条件: 上の各行の調整条件。

### 2026-10-28 ユーザー決定 — R は変更しない

- ユーザー（原文）: 「R はこのまま」。R の上限 0.25 × 担保/日（`bond_budget_export_cap_permille` 250）を維持する。
- 抑止の上限は保たれる: 1 日分の発行由来報酬は担保の 0.25 以下で、回収可能 slash 0.51 を超えない（PESG §6）。
- 想定される REAL の処理量（17,400 以降）: 1 bond あたり約 0.25 × 担保 ÷ 3,200.8 KAS/claim / 日。100,000 KAS bond で約 8 claim/日、13,000 KAS bond で約 1 claim/日（実測では約 88/日 → R で約 11 分の 1 に絞られる）。R を超える attempt は skip され、block は有効のまま。
- 見直し条件: claim あたり報酬（escrow）の変更、担保の分布の変更、export cap の再検討（Stage 1 の S/K 実測）、REAL の停滞が観測されたとき。
