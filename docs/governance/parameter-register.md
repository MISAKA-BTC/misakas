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
| `registered_tier_cap` / κ_registered | 300‰（設計値） | 総枠比率 | 未承認という原文状態を保持。[出典](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
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

**κ_registered:** 縮小分不発行・再配分なし。上限無効化は通常調整でない。

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
