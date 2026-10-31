# RFC・ADR統合版の取り込み — 2026-10-31

現行の閲覧・更新先を [RFC 15本](../rfc-v2/README.md) と [ADR 50本](../adr-v2/README.md) に切り替えた。入力はユーザー指定の `misakas-adr-v2-50`。配布物ではフォルダ名が `adr/`・`rfc/` だが、文書内の識別子・manifest・リンクは `adr-v2/`・`rfc-v2/` を前提としていたため、その名前空間で導入した。

旧204 ADRの48,236行・bytesと旧41 RFCの既存パスを保持した。RFC-0018の作業中の2026-10-31検証記録リンク1行をRFC-v2-0010へ継承し、原文430行を対応表へ反映した。既存のコード・実装記録・証拠の作業差分は保持した。

旧索引を履歴入口へ変更し、root README・総合入口・索引生成器から統合版を案内する。元資料の旧索引は移動前の相対リンクを含むため `.md.txt` としてbytesを保存した。統合本文の原条文に残っていた旧RFC間の相対リンク2件だけを旧パスへ修正し、該当収録断片のhashを更新した。

配布時のmanifest・source map・root READMEは `docs/history/documentation/2026-10-31-v2-import/` に保存した。ADRのsource manifestには、今回変更したnavigation・RFC差分等の配布時hashと取り込み後hashを `integration_changes` として列挙した。既知リンク問題のbaselineは変更しない。

設計の採択状態、PRINCIPLES、モデル二階層、claim認証、合意設定、有効化を変更しない。配布物のVALIDATIONは配布時点の記録として保持し、今回の結果は以下で再実行する。

```sh
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
python3 docs/adr-v2/test_integrity.py
python3 docs/maintenance/check_docs.py
```

## 同日並行作業の追加文書

取り込み中に別の作業から ADR-0210／RFC-0042 が追加されたため、その原文と番号を保持し、両v2 registryの `post_consolidation_documents` とREADMEから案内する。原資料204 ADR／41 RFCの所有・bytes検査と追加文書の存在検査を分ける。追加を消したり、元snapshotの本文へ遡及的に混ぜたり、今回の整理だけで採択・有効化を判断したりしない。
