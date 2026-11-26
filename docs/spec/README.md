# 技術仕様

現在の制度・適用条件は[設計原則](../PRINCIPLES.md)と[20本のRFC](../rfc/README.md)から読みます。仕様章はtensor・codec・署名・状態構造の詳細を補います。古いprofileの値や番号を現行の新規採掘へ自動適用しません。

- [PALW-TIR](palw/04b-tensor-ir.md)
- [モデルの共通管理](palw/model-tiers.md)
- [配分と仕事量](palw/model-allocation.md)
- [bond予算](palw/bond-budgets.md)
- [claim認証](palw/claim-certification.md)
- [PQ仕様](pq/kaspa-pq-spec.md)

モデル追加はBFTのみ。全モデルにMOE3の共通管理・係数1.0・市場を適用し、Panel/Courtと独立Gate Cを維持します。未知kernel・codec不正・未定義checkerはBFTのYESでも免除しません。

画像・動画採掘とモデル改善はmainnet後へ延期・当面無効です。[適用範囲](../governance/post-mainnet-scope.md)。設計・実装・検証・公開有効化は別です。
