# PALW詳細設計への入口

規範は[現行RFC20本](../../rfc/README.md)です。モデル追加はBFT承認だけとし、kernelの自動loweringは実行準備として使います。新規採掘はTB1/FOW1、個別claimはG14/Court/独立Gate C、共通経済はMOE3/BMR1を参照します。MWA1は提案のままです。

- [モデル・実行](models/README.md)
- [検証](verification/README.md)
- [実行と決済](execution/README.md)
- [QF1参照codec](models/qf1/README.md)
- [BFT採用policy](models/mad2-admission-policy.json)
- [任意評価policy](models/mas1-admission-policy.json)
- [MOE3経済policy](economics/moe3-economic-policy.json)

日付・commitを持つ検証メモは当時の入力・実装を示し、新profileの採用・公開有効化を宣言しません。文書再編でwire識別子、署名domain、旧claimの期限・責任は変えません。
