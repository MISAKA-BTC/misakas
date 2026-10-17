# 整理内容と安全な反映手順

**入力:** ユーザー提供 `docs(1).zip`。**整理日:** 2026-10-17。GitHubや稼働networkへの変更ではなく、添付資料を整理した配布版です。

## 変更内容

rootをREADME / PRINCIPLES / STATUS / INDEX / AGENTSの5ファイルに整理し、運用・仕様・設計・参照・監査・証拠・履歴へ配置しました。短い入口と、番号順・トピック別・全文書索引を分けています。

設計凍結はADR-0181/RFC-0021、PRINCIPLES §11とgovernanceに記録しました。設計変数・パラメーター以外は通常変更禁止、重大問題時は新規RFCと新規ADRの両方、レビュー、明示的採択が必要です。

追加指示により、RFC-0003の画像・動画経路とRFC-0004のモデル改善はmainnet後へ延期し、当面無効としました。ADR-0182/RFC-0022、PRINCIPLES §12、両RFC、対応Spec、索引、STATUS、readiness/統合/release資料に反映しています。mainnet開始で自動有効化せず、共有のテキスト・不変登録・検証・会計まで停止しません。

RFC-0017の既に撤回された旧資格審査は履歴へ分離しました。既存ADR/RFC主文書の番号・パスは保持し、実装補助資料は分離しました。既存番号衝突は完全パスで台帳管理しています。

## 変えていないこと

α=1.5、30日平均、階層係数、登録総枠の既存値/承認状態、Panel=0 7日相当・Panel=1 1日相当、Q/B/R/F、identity、claim認証の規則を今回再調整していません。未採択Draftを一括採用せず、旧方式も復活させません。

nodeコード、Params、wire、fence、fingerprint、有効化高さ、稼働設定を変更していません。**画像・動画・改善の実際のruntime無効化は未確認**で、full checkoutで実装照合・拒否/回帰試験が必要です。

## 原文・データの保存

[manifest](migration-manifest.json) に元/新パスと前後hash、[対応表](path-map.md) に移動先、[修正記録](link-rewrites.json) にURL書換えを記録しました。全元Markdownは [原文ZIP](../history/documentation/2026-10-17/original-markdown.zip) に元バイトで保存しています。ログ・JSON・バイナリ・元の補助プログラムは内容を保持しました。

OSの `__MACOSX` / AppleDouble 等を除外しました。古い実行例・ログ中のパスは当時の記録で、実行時には新しいパスを確認してください。旧GitHub URLがすべて自動転送される構成ではありません。

## 反映

元ZIPと一致するcheckoutを対象に、packageルートの同梱ツールを先にdry-runします。

```sh
python3 apply_docs_reorganization.py --repo /path/to/misakas --report /tmp/docs-reorg-review.json
```

元ファイルが変更/欠落している場合や、未知のファイルの衝突・symlinkがある場合は停止します。新しい作業内容を強制上書きしません。docs外に旧パスへの参照が残る場合は、別のレビュー済み変更で直してから反映します。ツールはコードを自動書換えしません。

```sh
python3 apply_docs_reorganization.py --repo /path/to/misakas --apply
```

既存docsの全体バックアップを `.docs-reorg-backup-<timestamp>` に保持し、未知の追加ファイルを残して置き換えます。commit/pushは行いません。単純な上書き展開では移動元が残るため避けてください。

反映後はgit diff、docs外の参照、統合先のADR-0181/0182・RFC-0021/0022の番号予約を確認し、文書checkerを実行します。ルートAGENTS.md/CLAUDE.mdやCI連携は元ZIP外のため別途照合します。

[検査結果と残存課題](validation-report.md) / [正式な延期範囲](../governance/post-mainnet-scope.md)。
