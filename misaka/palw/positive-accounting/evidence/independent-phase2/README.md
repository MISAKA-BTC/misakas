# 並列実装 phase 2 の検証記録

raw logを編集せず保存した。コマンド、環境変数、実行開始時のbase commit、終了codeは各 `*-result.json` にある。
実行時には未commitのA5追加sourceも含まれるため、対象sourceの正本は `tested-source-manifest.json` のSHA-256である。
実装は後に `7cdce10d3` でcommitされた。

- node: 実A1 terminal 6件 + 既存escrow engine 9件 = 15件成功。
- A1/既存Court: 13 + 2 + 9 + 7 = 31件成功。
- 独立oracle/consumer: 5件成功。
- node全featureおよびleafのClippy: `--all-targets --no-deps -- -D warnings` 成功。

この段階ではA0 TIR annexまでを使った。後続のPanel frame認証helperやfunding共有契約の追加検証とは別の記録である。
native実資金受付・全native reader・永続化/実processの完成を示すログではない。
