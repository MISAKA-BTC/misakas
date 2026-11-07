# 文書の保守・検査

[整理・反映手順](REORGANIZATION.md) / [検査結果](validation-report.md) / [旧新パス](path-map.md) / [番号台帳](numbering.md) / [全文書](catalog.md)。

## 実行

リポジトリまたは配布packageのルートから、Python 3の標準ライブラリだけで実行します。

```sh
python3 docs/maintenance/check_docs.py
python3 -m unittest discover -s docs/maintenance -p 'test_*.py' -v
python3 docs/maintenance/generate_indexes.py
```

通常モードは新規不備の回帰検査です。既存の欠落参照は [known-link-issues.json](known-link-issues.json) に原文との対応付きで残し、PASSを「全リンク正常」としません。既知の不備も失敗させる場合は `--strict` を指定します。baselineを一括更新して新規不備を隠しません。

## 対象と限界

新しいリンク不備、番号衝突、主文書索引の不整合、必須入口、rootへの未分類ファイル、パラメーター台帳の基本形式、画像・動画・改善の文書上の延期/無効化方針を確認します。実際のnode無効化、プロトコルの意味、承認の真正性、build/test/有効化は検証しません。

外部URLとdocs外のコード参照は取得しません。Markdownの完全なCommonMark実装ではなく、見出しfragmentはGitHub形式の近似です。[機械出力](validation-summary.json)。

## CI

既存CIのdocs変更ジョブからcheckerとunit testを呼び出します。入力ZIPに含まれない `.github/workflows/` や `scripts/ci-gates.sh` は変更していません。文書チェックを実装の安全性検証の代用にしません。

[元資料manifest](migration-manifest.json) / [リンク修正](link-rewrites.json) / [反映ツール試験](tool-test-results.json)。
