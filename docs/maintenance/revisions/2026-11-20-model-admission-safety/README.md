# MAS1改訂記録 — 2026-11-20

## 採択と入力

今回のユーザー指示によるモデル追加経路の構造検査・実用性条件・自動判定不能時のBFT停止への接続。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。公開設定・配備・支払は変更しない。

- 正本: [RFC0065](../../../rfc/0065-model-admission-screening-and-bft-stop.md) / [ADR0233](../../../adr/0233-model-admission-screening-and-bft-stop.md)
- 適用: [MAS1移行](../../../governance/model-admission-safety-transition.md)
- 設計契約: [JSON](../../../design/palw/models/mas1-admission-policy.json)
- 公開裁定: [v0.6](../../../design/palw/consensus/public-adjudication-policy-v0.6.md)

入力ZIP: `misaka_docs_MOE2_20261120 2(1).zip`。元ファイルの全hashとZIP digestはsource-manifest.json、変更前bytesはbefore/、変更一覧と新hashはchange-manifest.jsonに保持する。過去の改訂記録・検査結果・source snapshotは改変しない。

## 主な変更

1. kernel、lowering、challenge、Readout、useful state、cache/MoE、既知の短絡をPASS/FAIL/UNKNOWNで検査する。
2. 実用性G-USEを追加し、非報酬評価をTB1-MODEL_AUDITと別PANELへ接続する。評価からcreditや報酬を作らない。
3. UNKNOWNは閉じたままBFTへ送る。全weight属性を自己申告から推定せず、BFTもG-COST不明を承認できない。
4. BFT署名権限をモデル/ruleの安全停止と条件付き解除へ限定し、native canonical順序へ統合する。
5. 停止をFinal・新規発行・未消費受賞権利より先に確認し、burn・owner・core指定・旧解除証明による迂回を拒否する。
6. MOE2の市場原子生成、経済比率、永久LP、確定残高、旧claim責任を維持する。

## 読む順序

設計の詳細はRFC0065、方針変更の理由はADR0233、実装時の否定試験は[DESIGN_REVIEW.md](DESIGN_REVIEW.md)、実際に実施した文書検査は[VALIDATION.md](VALIDATION.md)、変更対象は[CHANGES.md](CHANGES.md)を参照する。後者の検査はQwenの費用下界・native実装・BFT停止の実ネットワーク試験ではない。

## 既存の検査結果について

入力版に参照先の未同梱と旧snapshotとの差分がある。既知baselineを上書きして成功を装わず、入力時と改訂後の結果、今回のリンク差分、保存原本の一致を別々に記録する。
