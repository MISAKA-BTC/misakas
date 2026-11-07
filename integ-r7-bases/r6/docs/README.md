# MISAKA ドキュメント

**使う・運用する・仕組みを理解する・変更を提案する。目的から選んでください。**

> **採用済み設計は原則凍結。** 設計変数・パラメーターの承認された調整を除き、通常は変更しません。重大な問題がある場合に限り、**新規 RFC と新規 ADR の両方**をレビュー・明示的採択して変更できます。[変更管理](governance/design-freeze.md)
>
> **初期 mainnet の対象外:** RFC-0003 の画像・動画経路と RFC-0004 のモデル改善は、**mainnet 後に延期し、当面は無効**とします。mainnet 開始による自動有効化はありません。[延期・無効化方針](governance/post-mainnet-scope.md)

この文書セットはユーザー提供 `docs(1).zip` の整理版です。設計採択、実装、検証、稼働ネットワークへの有効化は別です。今回、コードや稼働 node を変更・検証していません。[状態](STATUS.md)

## 目的別の入口

| 目的 | 最初に読む |
| --- | --- |
| ビルド・ノード接続・採掘 | [はじめに](getting-started/README.md) |
| ノード・モデル・RPC を運用 | [運用](operations/README.md) |
| 全体の構成を理解 | [アーキテクチャ](architecture/overview.md) |
| 採用済み規則と適用条件を確認 | [仕様](spec/README.md) |
| API・設定・manifest を参照 | [リファレンス](reference/README.md) |
| 採用理由・技術設計・未決事項を読む | [ADR](adr/README.md) / [RFC](rfc/README.md) / [詳細設計](design/README.md) |
| 試験・監査・証拠・履歴をたどる | [testing](testing/README.md) / [audit](audit/README.md) / [evidence](evidence/README.md) / [history](history/README.md) |

## 設計を読む順序

[PRINCIPLES](PRINCIPLES.md) → [全体像](architecture/overview.md) → [PALW のトピック別仕様](spec/README.md) → 必要な ADR / RFC。
モデル階層と旧審査廃止は RFC-0017 / ADR-0179 の採択済み改定を優先します。`α=1.5`・30日平均・bond 予算・claim 認証は今回撤回していません。

## 文書を更新する人へ

[凍結規程](governance/design-freeze.md) / [パラメーター台帳](governance/parameter-register.md) / [文書の役割](governance/document-lifecycle.md) / [テンプレート](governance/templates/README.md)。

[今回の変更・反映手順](maintenance/REORGANIZATION.md) / [旧パスから探す](maintenance/path-map.md) / [番号台帳](maintenance/numbering.md) / [検査結果と残存課題](maintenance/validation-report.md)。
