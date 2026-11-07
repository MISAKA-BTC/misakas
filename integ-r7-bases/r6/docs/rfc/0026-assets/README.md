# RFC整合性監査パッケージ

対象: 添付 `misakas-pre (1)(1).zip`。2026-10-19。

ZIP SHA-256: `367c7bd16c9136fe434eacd2ae70c59dd2a63b5a5ce865f431b9347412288583`

## 読む順序

[監査報告](audit-report.md) → [RFC25件の確認範囲](coverage.md) → [パス・原本行番号付き根拠](source-evidence.md)。

その他:
- `findings.json`: 指摘の分類と参照箇所。
- `inventory.json`: 全RFCと索引の行数・bytes・SHA-256・見出し。
- `cross-rfc-normative-scan.json`: 全文横断照合の検索ヒット。
- `checks.json`: 算術・ソースマーカー・参照ファイル有無の確認結果。
- `reproduce_checks.py`: 上記の静的確認を再現するPythonスクリプト。
- `MANIFEST.json`: 配布物のSHA-256。

## 静的確認の再実行

Python 3.9以降、標準ライブラリのみ。元ZIPを展開したリポジトリを渡す。

```bash
python reproduce_checks.py --repo /path/to/misakas-pre --out checks-reproduced.json
```

これはRustのbuild/test、実モデル評価、負荷試験、DDoS試験を実行しない。12件の算術・マーカー確認が通っても、courtの安全性・容量・deadlineの達成を証明しない。

## 範囲

番号付きRFC25件を全文横断照合し、現行規範・改定・上位原則・関連するコードを重点確認した文書監査。全履歴ログや全コードの逐語監査・形式証明ではない。現行t12の出荷binary・live設定は未確認。

元のRFC、ソースコード、fence、ネットワーク設定は変更していない。Git commit/pushも行っていない。

このパッケージは元リポジトリ全体を再配布せず、監査結果と必要な原本抜粋だけを含む。
