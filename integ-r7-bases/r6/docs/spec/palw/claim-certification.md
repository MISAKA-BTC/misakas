# claim 認証 — Final と SecurityCertified

> 採用済み条項の読解用統合版。新規規則・実装検証・本番有効化ではありません。[状態](../../STATUS.md) / [凍結](../../governance/design-freeze.md)。

モデル単位の旧資格審査を廃止しても、claimごとの認証・確率的検出・公開証拠・決定論的裁定は全階層で維持します。

RFC-0018のGate C/SecurityCertifiedは、claim固定後の乱数によるglobal poolからの抽選検証者の肯定承認とG14公開告発windowに基づく認証です。正直な割合・参加・検査能力などの明示的前提があり、無条件な計算正当性の数学的証明ではありません。

Finalは手続きの完了で、SecurityCertifiedと別です。未認証claimはfork choice、決済深度、Rule E参加、settled anchor、weight等の合意creditを進めません。有効な公開反証によるvoidと取消しは出典に従います。

当面無効の画像・動画・改善経路を、claim認証だけ満たせば有効としません。提供・受理の無効化と、残す経路の認証条件は別です。

## 出典と適用条件

[RFC-0017](../../rfc/0017-model-eligibility-examination-and-renewal.md) / [RFC-0018](../../rfc/0018-dag-parallel-multi-model-consensus.md) / [ADR-0180](../../adr/0180-palw-only-consensus-no-bft-validators.md)。

[パラメーター台帳](../../governance/parameter-register.md) / [mainnet後への延期](../../governance/post-mainnet-scope.md) / [仕様索引](../README.md)。
