# ADR-0211 — 学習済みcheckpointのlogit範囲を損なわず扱う

> A0への取り込み注記（2026-11-01）: 本文の実装・検証実績は元checkoutでの私設実験を指す。A0の実装統合・再検証・有効化は別で、[統合境界](../design/palw/positive-court/WIDE-Q24-INTEGRATION.md)を参照。

- Status: Accepted for implementation / 2026-10-31 ユーザーが両案を明示採択。正式番号を台帳・全local refの履歴と照合済み。
- Implementation / Verification / Activation: 登録・v2 APIの私設実装と実checkpoint検証は完了。公開網は未有効化。公開運用入口の接続・独立レビューは未完了。
- 対応する新規RFC: [RFC-0043](../rfc/0043-wide-q24-llm-output.md)。

## 背景・提案する決定

実際の学習済みPythia-70mが公式Transformersでも約1082のlogitを出すため、「学習済みならi32 Q24に収まる」という変換器の説明は誤りだった。診断文は修正する。一方、旧i32 profileの範囲・意味は変更せず、明示的な別のi64 Q24出力profileを採択する。

## 理由・採らない処置

i32の閾値を緩めるだけでは値が表現できない。clipや温度の再解釈、中心化・縮小で元の出力と異なるものを同じモデルとして成功に数えない。checkpointの差し替えや最大文脈長の短縮も対処にしない。

TIRにi64が存在することだけでは、旧trace・IPC・sampling・courtがi64 logitを読める保証にならない。新しい出力profile、開示version、consumerの接続と全経路の試験を一緒に扱う。新しいprofileの計算creditは必要なGate C等の条件が揃うまで進めない。

## 維持する条件・残余リスク

二階層、claimごとの認証、正直な検証者割合の前提、court、bond責任は維持する。モデル審査や暗号学的な計算証明は導入しない。幅の追加でも任意の実数範囲やsourceの全入力での精度を保証しない。資料量・演算overflow・samplingの単位を試験する。

## 旧文書との関係・承認

現行LOGITS_Q24_V1のi32契約は未変更。本案の採択後に、新profileに対応するRFC-v2-0002/0003の追加条項と実装を明示する。承認者はユーザー、日付は2026-10-31。公開networkの有効化は別承認。

## 2026-10-31 採択記録

ユーザー回答「両案を採択し、実装を進める」により、対応するRFC案とADR案を実装対象として採択した。公開networkの有効化承認ではない。旧i32契約・ID・履歴を維持し、新profileの全consumerと回帰試験を接続する。
