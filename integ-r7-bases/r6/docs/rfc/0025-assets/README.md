# 多モデル・大量court RFC v2

## 入口

- **統合RFC:** [RFC-0025](../0025-scalable-objective-court-many-models-mass-disputes.md)(旧 Draft 名 RFC-DRAFT-palw-scalable-objective-court-v2.md)
- **決定記録:** [ADR-DRAFT-scalable-court-v2.md](ADR-DRAFT-scalable-court-v2.md)
- **今回のソース確認:** [source-audit-v2.md](source-audit-v2.md)
- **算術の出力:** [capacity-report.json](capacity-report.json)
- **採択用テンプレート:** [arming-record.template.json](arming-record.template.json)

## 設計の中心

モデル数や独立claim数が増えても、courtの証拠と検証workを有界にする。通常時のoriginal受付起点のdoorは3,200 DAAを目標とし、有限burstで混雑した案件は予約に基づき期限・Final保留・証拠保持・bond返還を連動して延長する。

将来のcourt容量もoriginal claim受付時に確保し、持続過負荷は新しい経済的責任の受付を抑制する。新たなモデルやアドレスを増やして無料の容量枠を作らせない。既存の正規courtを容量不足で捨ててFinalへ逃がさない。

A/B/C、固定サイズ上限、1,000/10,000件試験、control帯域、owner枠占有、bounded extension、block容量拡張、checkpoint/pruning/IBDを一体化した。

## 主な前版との差分

| 前版 | v2 |
|---|---|
| 単発のdoor短縮が主目的 | 多モデル・大量courtの安全な受理/完了/復帰が主目的 |
| 3,200の固定期限を目指す | 通常3,200と、容量予約に基づく混雑時延長を区別 |
| 1位置の最終証拠に注目 | fragment/close/全transcript/work/RAM/保持を別々に制限 |
| 締切は非延長 | 事前に署名・料金・上限を合意したv2のみ原子的に延長 |
| 同時負荷の試験条件 | globalな容量予約・control保護・新規claim admissionを設計 |
| 保持と回復への配慮 | checkpoint10,000と安全な証拠保持、検証可能IBDを具体化 |

前版のファイルは RFC-0024 の `docs/rfc/0024-assets/` に変更せず保存している。矛盾する期限/混雑規則はv2本文を優先する。

## 計算補助

Python 3.10以上の標準ライブラリだけで動作する。インターネット接続は不要。

```bash
python -m unittest -v test_capacity_planner
python capacity_planner.py --out capacity-report.json
```

`capacity_planner.py`には、転送容量の必要条件、整数切上げ/overflow検査、排他的deadline、保守的なlane予約toy modelを含む。

**27試験を実行し合格。** 内容は算術・原子的予約・順序・snapshot再生・合成入力1万件での予約整合性。実測network、Rust実装、実ノード1万court E2E、暗号学的被覆の試験ではない。

合成schedulerのbandwidth/CPU/IO/fixed_daaは、状態・順序を試すための人工入力である。出荷nodeの性能、実際のdoor、推奨運用パラメータとして使わない。`wire_table`は理論容量下限であり、完了期限ではない。

## 未確定事項

17,408 B/32 KiB/64 KiBはdraft目標候補。全codecとfault routeからの上限導出前はUNPINNED。`E_max`、保証帯域、control収録条件、価格・保持料、block容量、one-outsider枠占有耐性、検証可能checkpointの接続も採択前に確定する。

`arming-record.template.json`のnullは未確認を意味する。0や仮hashに置き換えて有効化してはならない。JSONは記録様式の例であり、既存Rust APIに実装済みのschemaではない。

## 今回行っていないこと

live branchの取得・変更、Rustコンパイル、node起動、実性能測定、プロトコルの形式証明、t12/fence/main変更は行っていない。対象は添付source snapshotの限定的読解、RFC作成、計算補助の試験である。

## パッケージの検査

`arithmetic-test-results.txt`は今回実行した27試験の出力、`package-checks.txt`は文書リンク/JSON等の検査、`PACKAGE-MANIFEST.json`は各ファイルのSHA-256を記録する。前版の証跡と今回の新しい試験を混同しない。
