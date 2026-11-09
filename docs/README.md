# MISAKA ドキュメント

> **未完gate:** 大きな資料の供給・取得後検査を有界なnative根拠で客観終結するprofile別手順は、具体化できていない場合 **PROFILE PROCEDURE NOT ESTABLISHED** として有効化不可である。最低承認数1や公開方針だけでは、この手順の成立を証明しない。実装・実測の未完と区別し、18 §2.4／20 §4.3のgateを維持する。

## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

> **2026-11-08追加採択:** EXEC creditは[RFC07](rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

> **2026-11-08 現行入口:** [RFC 20本（RFC-v2-0001〜0020）](rfc-v2/README.md) と [統合 ADR 50本](adr-v2/README.md)。新しい設計の採択は [ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) に記録し、50本の統合単位とは別の post-consolidation 文書として保持します。[今回の改訂](maintenance/revisions/2026-11-08/README.md)。
>
> **2026-10-31 の統合履歴:** 当時は RFC 15本・ADR 50本に整理しました。旧番号の本文・参照を保持しています。[取り込み記録](maintenance/v2-integration-2026-10-31.md)。

**使う・運用する・仕組みを理解する・変更を提案する。目的から選んでください。**

> **採用済み設計は原則凍結。** 設計変数・パラメーターの承認された調整を除き、通常は変更しません。重大な問題がある場合に限り、**新規 RFC と新規 ADR の両方**をレビュー・明示的採択して変更できます。[変更管理](governance/design-freeze.md)
>
> **初期 mainnet の対象外:** RFC-0003 の画像・動画経路と RFC-0004 のモデル改善は、**mainnet 後に延期し、当面は無効**とします。mainnet 開始による自動有効化はありません。[延期・無効化方針](governance/post-mainnet-scope.md)

先行改訂の入力はユーザー提供 `docs(2).zip` です。今回の追加改訂は、そのRFC20本更新版とEXEC・1 BPSの指示、添付court予算分析を基準としています。2026-11-08 に RFC-v2-0016〜0020 の設計を採択しましたが、状態は **ADOPTED_DESIGN_NOT_ACTIVATED** です。設計採択、実装、測定・検証、稼働ネットワークへの有効化は別であり、この文書改訂によってコードや稼働 Node の適合・有効化を宣言しません。[状態](STATUS.md)

新しい claim 版では、**Node はモデル本体を取得・保持せず、全推論・GPU を要求しません**。モデル未共有とモデル内の Miner／Panel の全結託を想定し、Miner の資料供給、独立選出 Panel の非対話 positive、不一致時の対話 negative、検証成立後の成功 Final を定義します。global pool の独立選出と正直な検証者の前提は残ります。[設計の入口](rfc-v2/0020-verification-gated-final-and-adversarial-models.md) / [公開裁定の方針 v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md)。

## 目的別の入口

| 目的 | 最初に読む |
| --- | --- |
| ビルド・ノード接続・採掘 | [はじめに](getting-started/README.md) |
| ノード・モデル・RPC を運用 | [運用](operations/README.md) |
| PALW の claim・court・bond・fence を CLI で読む・object を出す | [PALW CLI ガイド](guides/palw-cli.md) |
| 全体の構成を理解 | [アーキテクチャ](architecture/overview.md) |
| 固定する構造・責務とモデル別DAG並列生成を確認 | [PALW Architecture Specification](architecture/palw-specification.md) |
| 採用済み規則と適用条件を確認 | [仕様](spec/README.md) |
| API・設定・manifest を参照 | [リファレンス](reference/README.md) |
| 採用理由・技術設計・未決事項を読む | [統合 ADR（50本）](adr-v2/README.md) / [RFC（20本）](rfc-v2/README.md) / [新設計の採択 ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) / [詳細設計](design/README.md) |
| 試験・監査・証拠・履歴をたどる | [testing](testing/README.md) / [audit](audit/README.md) / [evidence](evidence/README.md) / [history](history/README.md) |

## 設計を読む順序

[PRINCIPLES](PRINCIPLES.md) → [全体像](architecture/overview.md) → [PALW のトピック別仕様](spec/README.md) → 必要な ADR / RFC。
モデル階層と旧審査廃止は RFC-0017 / ADR-0179 の採択済み改定を優先します。`α=1.5`・7日平均（PRINCIPLESと後続承認記録に従う）・bond 予算・claim 認証は今回撤回していません。

## 文書を更新する人へ

[凍結規程](governance/design-freeze.md) / [パラメーター台帳](governance/parameter-register.md) / [文書の役割](governance/document-lifecycle.md) / [テンプレート](governance/templates/README.md)。

[2026-11-08 の変更・反映手順](maintenance/revisions/2026-11-08/README.md) / [今回の検査結果](maintenance/revisions/2026-11-08/VALIDATION.md) / [番号台帳](maintenance/numbering.md)。

[以前の整理・反映手順](maintenance/REORGANIZATION.md) / [旧パスから探す](maintenance/path-map.md) / [以前の検査結果と残存課題](maintenance/validation-report.md)。
