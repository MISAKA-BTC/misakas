# ADR-0208 — 再利用 prefix の生成元 identity を明示する案

Status: **Accepted / 採択済み** (2026-10-28)。実装中、検証・有効化は未完。

## 背景

[RFC-0040](../rfc/0040-authenticated-prefix-work-dependencies.md) のとおり、現行の prefix descriptor は生成元 work identity を含まない。[RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md) が要求する入力依存を root の偶然の一致から推定することはできない。

## 決定

既存 wire は保ち、新 version の署名済み prefix export statement と消費側 reference を追加する。生成元は受理前に network、work namespace、nonce、自己参照しない base claim commitment、完全な descriptor と有限な責任・材料期限を署名する。consensus が実際の claim ID と受理点に結び付け、消費側はその受理済み identity を署名対象にする。将来の受理 block を producer に先取り署名させない。export は生成元の受理時に commitment され、その正しさは生成元の claim 単位の再実行・court で検査する。署名・hash は同一性だけを認証する。

新 fence 後の非 genesis prefix を再利用する新 claim は、受理済み export への明示 reference を必要とする。source 不在を独立な入力に変換しない。消費側の hard と材料保持は推移的な全 source の既存署名期限・予約内に収め、新たな消費で source を延長しない。旧 cohort の条件は変えず、旧 claim に export を後付けしない。

## 帰結

消費側の実際の依存、生成元退役後の責任、pruning/IBD 後の資料を同じ identity で追跡できる。一方、新規 export/envelope の wire、検査経路、資料保持、移行試験が必要になる。既存の未測定値や欠けた資料を、この提案の採択で埋まったと扱わない。

## 採択記録

2026-10-28、ユーザーが「採択して実装・検証を進める」と明示承認した。生成元と消費側の自己参照しない base commitment、および推移的な署名済み保持期限の範囲を含む。採択は実装完了や試験通過の記録ではない。preset と arming は実装・検証・証拠 review の完了後に変更する。
