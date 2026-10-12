# RFC/ADRレビューの整理 — 2026-10-12

ユーザーのレビュー結論を記録する。対象はADR-0173/0176/0177、RFC-0001〜0015の関連節。
**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。

## X1の撤回 — モデル配布の自由と、参加者の保有義務・claimの検証条件は別問題

「モデル配布を合意で管理しない（ADR-0177 D1）」と「検証できないclaimに報酬を払う」を同一視したX1は誤読であり、確定した設計矛盾として扱わない。
次の3つを区別する。

1. **Miner・Panel:** 計算や検証に参加する以上、正しい登録モデルを保有し、モデル同一性を確認できることが必要。
2. **モデル登録者:** モデルを第三者へ配布できない、または配布を拒否したという理由だけで、登録モデルの報酬資格を停止したり、登録者に制裁を科したりしない。
3. **個別claim:** 実際の計算・検証・裁定・Finalに必要な条件は満たさなければならない。モデルの配布自由と、未検証の計算への無条件支払いは別問題。

ADR-0177 D2・D7もモデル同一性の検証とclaim固有の証拠裁定を維持する。残るのは「第三者がモデルを取得できなかった場合」と「必要なPanelが成立しなかった場合」を仕様上区別する文言整理であり、ADR-0177 D1補足（2026-10-12）で行った。

閉鎖モデルなら不正検出確率が必ず0になる、という説明も強すぎた。独立したモデル保有Panelが検証できる場合、検出確率は0とは限らない。残る課題は、Panelの共謀・独立性の偽装・検証能力不足をどの程度抑止できるかである（operator IDが別でも実質所有者が同一である可能性は残り、ADR-0177もこの限界を認める）。

## 残る問題と推奨方針

| 問題 | 推奨方針 | 優先度 | 反映先 |
| --- | --- | --- | --- |
| N1・N2 モデル非公開と不正検出 | Miner・Panelのモデル保有を必須とし、独立Panelの検証能力を維持（RFC-0002 Proposal Aの `class_seated`：executor以外のモデル保有operatorを `seat_count` 以上、registrant・executorを除いた独立operatorの下限） | 最優先 | ADR-0177 D1補足・D7注記、RFC-0002 |
| N3 Work Beacon循環 | 独立したbootstrap成立条件を設計。成立まで新profileはDormant | 最優先 | RFC-0007、RFC-0011 |
| X3 Challenge期限延長 | claim単位の絶対期限を固定し、延長を禁止 | 高 | RFC-0014、RFC-0003 |
| N4 プライバシー（X2と一体） | 公開検証不能な経路を報酬対象から除外 | 高 | RFC-0003、RFC-0004、RFC-0001 |
| N5 Panel=0の監視不確実性 | 現行Panelを維持。Panel=0は独立した実証後に判断 | 高 | RFC-0015、RFC-0010 |
| W1 Seed写像 | `le64(n+1)` へ統一。オーバーフローとなる入力は拒否 | 中 | RFC-0001 |
| W2 Text規則 | FP V4自体のtext規則は変えず、TextStream pipelineに追加規則を適用する、と範囲を分離 | 中 | RFC-0001、RFC-0003 |
| X4・X5 文書の不一致 | 実装状態・fence名の対応を整理 | 低 | RFC-0001、RFC-0012 |

### N3 — Work Beaconのbootstrap
RFC-0007 §VI.3は既存の独立して有効なPALW workを検証用乱数のsourceとする。条件を満たす既存profileがない場合、最初のprofileの有効化が循環する。
通常のWork Beaconと、最初のsourceを成立させるbootstrapを分け、bootstrap候補には独立検証可能性に加え、将来の乱数の予測困難性、選別・再試行によるgrinding耐性、reorg時の再現性を要求する。
既存block hashでの代用や管理者seedはRFC-0007が禁止しており採らない。十分なbootstrap sourceが設計・検証されない場合、新しい報酬profileはDormantに維持する（既存チェーンの通常動作は止めない）。

### X3 — Challenge期限
RFC-0014 §7.2を統一ルールとし、各claimに絶対期限 `dispute_hard_deadline` を固定する。どのchallengeもこれを超えない。

```text
effective_deadline = min(proposed_challenge_deadline, dispute_hard_deadline)
```

残り時間が検証・証拠開示・court完了に足りない場合はchallengeを開始させない。旧claimには旧rulesetを適用し、新しい期限規則は別のversioned fenceで有効化する。

### N4（X2）— プライバシーと公開訴追

| 経路 | 方針 |
| --- | --- |
| 通常の非報酬AI推論 | プライバシーを維持して利用可能 |
| 公開検証できるPALW claim | 必要条件を満たせば報酬対象 |
| 必要な証拠を公開できない生成物・画像・source経路 | 新規の報酬・weight対象から除外 |
| 過去の有効claim | 旧rulesetで処理し、遡及変更しない |

公開検証のためにユーザーの非公開データを強制公開しない。既存の実装記録は削除せず、将来の有効化対象を限定する。プライバシー保護と独立した客観的検証を両立できる証明方式が成立した場合だけ、該当profileの再導入を検討する。

### N5 — Panel=0
RFC-0015 §7のとおり、監視市場を作るだけではhonest verifierの存在を保証できない。モデル保有Panelを必要とするmodeを維持し、Panel=0は別の安全性実証後に判断する。実証では監視者の離脱、Panelの共謀、検証費用、最大claim負荷、cold model取得時間、不正発覚からcourtまでの時間を測る。多数の監視者を登録させるだけでは、誰かが必ず正直に検査することの証明にはならない。

### W1・W2・X4・X5
- **W1:** Gatewayの整数seed変換を `le64(n+1)` に統一し、オーバーフローとなる入力を拒否する。gateway側の表示・入力変換規則であり、直ちにコンセンサス分岐が起こると断定しない。
- **W2:** 「FP V4自体のtext規則は変更しない。TextStream pipelineには追加規則を適用する」と明記する。
- **X4:** FP Job V6を「実装済み・**testnet-12でDAA 5,300から有効**（int-11 flag day、`palw_fp_decode_constraint`）。nodeによる直接証明の生成などcourtの一部に未完了事項あり」と記録する。*（2026-10-12訂正: 当初の「dormant」はコードと異なる。[台帳](mainnet-spec-register-2026-10-12.md) X4）*
- **X5:** `palw_dns_retirement_v1` は設計上のfence識別子、`Params::palw_dns_retirement` はRustの内部フィールド名である（異なる階層の名前であり矛盾ではない）。

## 最終的な方針

モデル配布は自由。Miner・Panelは保有必須。claimには客観的な検証と裁定が必要。配布できないというだけでは登録者を罰しない。公開検証が成立しない生成経路は報酬対象から外す。
残る優先課題は、Work Beaconのbootstrap循環の解決、Challenge期限の統一、実効的な不正検出能力の評価である。
これは仕様の推奨整理であり、実装や敵対的試験によって安全性が証明されたものではない。

追加の経済設計（Miner＋Panelの適格claim bondの合算と1.3〜2倍の配分係数、Panel=1、Bond/time予算の具体化、受入試験B01〜M01・C01〜C18）は
[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案](panel1-bond-budget-model-capital-amendments-2026-10-12.md)に置く。

## ユーザー判断 — 2026-10-12（台帳 §3）

[mainnet対象仕様・未解決事項の台帳](mainnet-spec-register-2026-10-12.md) の判断5点を次のとおり決定した。いずれも稼働中のtestnet-12（L）の挙動は変えず、旧claimは旧rulesetで処理する。

1. **X3 期限:** 延長禁止（窓の再付与・期限の後ろ倒しを含む）は**新しいversioned fenceだけ**で適用する。Lの `rearm_after_challenger_side_close` とstaged courtの延長は旧規則として残す。固定時点はRFC-0014 2026-10-12改定節（d0＝claim受理時）に従う（§7.2本文の「最初のpursuit受理」より新しい記述を採る）。
2. **N4 プライバシー:** 必要な証拠を公開できない経路（V5の画像・source ID、`PanelDa` 等）は、**新しいfence**以後に受理したclaimを新規の報酬・weight対象から除外する。
3. **ADR-0177 D2:** Lで有効なモデルbytes強制のcourt経路4本（`HeldStepLeaf`・`AttnRootClaimOperands`・`TirGenRootClaimParams`・`SeatReadinessLeaves`）を、**休眠の制限fence**で閉じる（RFC-0014 §16.4の案）。
4. **W1 整数seed:** gatewayは整数seedを**拒否のまま**とする（64桁hexの32 byteのみ）。`le64(n+1)` 写像案は撤回し、RFC-0003の記述を訂正した。
5. **N3 bootstrap:** 独立したbootstrapが設計・検証で成立するまで、新しい報酬profileは**休眠のまま**とする（Panel=0の `OpvFinal` 経由のbootstrapは有効化しない）。
