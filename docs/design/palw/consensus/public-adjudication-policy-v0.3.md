# 公開裁定の方針 v0.3 — 一人の反証と実行整合性

採択日: 2026-11-09 / 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

[RFC-0049](../../../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../../../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)の新claim/profileに適用する。詳細の規範所有者は[RFC-v2-0016〜0020](../../../rfc-v2/README.md)。[v0.1](public-adjudication-policy-v0.1.md)・[v0.2](public-adjudication-policy-v0.2.md)とその採択記録は保持し、以下の追加契約についてのみ新版が優先する。

## 1. 何を保証するか

対象は固定されたモデル・program・入力・実行規則・seed・依存状態に従う実行の整合性である。Miner本人や各Panel本人のGPU使用、独力実行、実作業時間を証明したとは扱わない。署名は主張と責任、Merkle openingは資料の同一性、native局所再計算は当該関係の正誤を確認する。

## 2. 小さな反証へ落とせるclaimだけを受理する

Minerは初期状態、全状態遷移、終端と出力の対応を含む記録を先に固定する。全relation familyが元claim・演算・座標・型・数値規則・依存へ結び付き、誤った実行なら少なくとも一つの認証された局所関係を反証できるprofileだけを受理する。後からPanelが作った未署名の細分traceをMinerの主張にしない。[16](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md)・[19](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md)。

## 3. 一人の異議と反証を多数決で消さない

一人の適格noticeはnative未解決状態を作り、解決まで成功を止める。一人の有効な反証は、他のPanelの承認数にかかわらず元claimを無効化する。完成証拠はMinerの再署名・通常Panelの許可・悪意側の共同提出を必要としない。不一致の局所化には対話を使えるが、提出済み反証の裁定を追加対話で遅らせない。[18](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)。

未認証資料、root不一致、私的な未受信申告だけを計算有罪としない。偽異議の棄却、供給義務違反、資源停止、未検証終了を区別する。

## 4. 成功の入口を一つにする

正常系では独立選出者が所定の全体検証を行い、[17](../../../rfc-v2/0017-noninteractive-positive-verification.md)の範囲付き承認を提出する。局所positiveや署名だけを全体の数学的証明としない。

必要承認・必須局所検査・公開検査機会・依存・資金条件の成立に加え、適格な未解決異議がなく、計算反証・失効・期限切れがないことを[20](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。同一blockでは、そのblockの適格異議・反証と失効を処理してから成功・creditを反映する。未成立は規定期限でAbortし、成功へ分類しない。

## 5. 資料・容量・時計

Nodeは全モデル・全推論・GPUを要求されず、認証された局所断片を上限内で扱う。資料欠落を全モデルfetchで補わない。

正直な一人の取得・提出・裁定経路を、他者の先着予約や多重claimで奪わない。資格・費用・帯域・包含・CPU・RAM・保持・復旧を含む有限の全分岐予約を原受理時に検査する。資料不足を隠した成功も、永久検閲下での無条件な反証完了の宣言も認めない。

原受理a、notice a+3,120、係争hard a+10,000、最遅notice後6,880 DAAと収集・保持の分離を維持する。draw、取得、交代、reorg後の再開で原点を作り直さない。EXECの1 BPS目標をcourt容量の実測と読み替えない。

## 6. 条件付き保証と有効化

一人の正直な検証者が、対象profileの完全な関係集合と必要資料にアクセスし、資格・予約・期限内包含の条件を満たして反証を到達させれば、他Panelの共謀はその反証を無効化できない。全員共謀、資料の永久秘匿、永久検閲、規則を実施しない合意ノードまで含む無条件保証ではない。

一つの鎖、モデル二階層、暗号学的計算証明の禁止、延期機能、旧署名済みclaimを保持する。今回の設計・文書レビューをnative実装・実モデル性能・本番activationの完了とせず、各profileの実装・測定・敵対試験を別に要求する。
