# 文書統合の検査結果

実行対象: `misakas-pre(5).zip`由来の旧ADRと、先行`misakas-rfc-v2-15.zip`。検査日: 2026-10-30。

**文書構造・原文保存の検査は成功しました。** これは設計の完全性・Rust実装・node動作・有効化・mainnet公開の認定ではありません。

| 検査 | 結果 |
| --- | --- |
| 現行ADR | 50本、ADR-v2-0001〜0050 |
| 旧ADR | 204ファイル、201個の番号 |
| 旧番号衝突 | 0026・0173・0193の別文書を全て保持 |
| 原文行数 | 48,236行、主担当へ欠落・重複なく割当 |
| 原文bytes | 3,981,701 bytes、204ファイル全てSHA-256一致 |
| 原条文の収録 | 204原文を担当する新ADRへ全文・同じbytesで収録 |
| 原文のStatus等 | 212宣言の原文行範囲と内容を照合 |
| 元ZIPとの直接比較 | 204/204ファイルがbyte単位で一致 |
| 既存RFC | 旧41本・新15本を含む既存RFC関連160ファイルを変更せず保持 |
| その他を含む既存資料 | 元パッケージ404ファイルの不変性を確認（変更対象のnavigation等を除く） |
| 新規リンク | 2,747参照の存在、うち204個の明示anchor参照も確認 |
| 検査器の回帰試験 | 12/12成功。欠落・改変・二重所有・番号衝突の潰し込みを検出 |
| 先行RFC検査 | 15本／41原文／21,728行／89分割／370新規リンクで成功 |
| policy台帳 | 31項目。部分撤回と維持する規則を分離 |
| 重要な優先関係 | 19項目。対象節・network・承認段階を区別 |

## 証跡と再実行

[ADR検査のJSON](validation/adr-validation.json) · [検査器12試験のログ](validation/validator-tests.txt) · [RFC検査のログ](validation/rfc-validation.txt) · [元ZIPとの比較記録](validation/source-comparison.json)

Python 3.10以降の標準ライブラリのみを使います。

```sh
python3 docs/adr-v2/check_integrity.py
python3 docs/adr-v2/test_integrity.py
python3 docs/rfc-v2/check_integrity.py
```

`source-manifest.json`は各旧ファイルの完全パス・SHA-256・行範囲・主担当・元の状態宣言を保持し、`registry.json`は50本の現行入口と編集本文の検査用hashを保持します。意図的に新しい統合本文を改訂する場合は、レビューのうえ該当metadataも更新します。元の判断を変える改訂には別途、既存変更承認手続きを適用します。

## 保証していないこと

本文の意味が全て形式的に同値であること、旧文書の全競合が解消済みであること、全ての過去リンクや添付証拠が小型パッケージ内に存在することは保証しません。旧原文のリンクは保存したままです。既存CIの全てが新namespaceへ対応したとはしていません。

runtime・合意定数・wire ID・署名domain・有効化高さは編集していません。Cargo build、node試験、公開配備は実行していません。提出時のZIPを別directoryへ展開した状態でも、ADRとRFCの同じ検査を再実行しています。
