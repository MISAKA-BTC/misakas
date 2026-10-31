# Phase5: source due / 新Positive資金のDATA提案

2026-10-31。対象はnode-local calculatorと共通lock分類であり、actual nodeの資金lock・支払権限ではない。
旧FundingTerms/escrowのID、global key/domain、本番有効化は変更していない。

| 記録 | 対象と結果 |
| --- | --- |
| `source-dues-result.json` / `source-dues.log` | root通常Cargo、`--no-default-features --lib dues::`、7件成功。記載source SHAはready commit `c0d21e2f1` と一致 |
| `native-funds-clippy-lib-result.json` / `native-funds-clippy-lib.log` | 同node crateのproduction lib、no-default、`-D warnings`、exit 0。test codeはこのClippy対象に含めない |
| `node-data-proposals-initial-result.json` / `node-data-proposals-initial.log` | root通常Cargo、no-defaultの全lib **44件成功**（source due 7、新funds 15、既存native 15、rows 7）。試験中source変更なし |
| `node-all-targets-clippy-initial-result.json` / `node-all-targets-clippy-initial.log` | 同じ候補のno-default全target、`-D warnings`、exit 0・警告なし |

各JSONは実command、環境、base commit、raw log、対象source SHA-256を保持する。
初回dues試験後に新moduleを登録したため、最初のlib.rs hashと後続のhashは異なる。
別候補の数を合算してnative全体の合格数にしない。
原ログはCargoの末尾空行も含めて保存し、整形のために変更しない。
最終44件に先行dues 7件は含まれており、51件として合算しない。
default featureのTIR経路、actual node process、native writer/UTXO batch、mempool/blockのlive接続はこの試験で実行していない。
独立read-only sourceレビューと原ログ/hash照合は `independent-review.json` に保存する。

source dueはfirst unique receiptで一度発生し、実支払とは区別する。
ゼロ料金でもreceipt identityを保存する。新spend案はactual Transactionと提示UTXO entryに対し、
三勘定、出力、burn、snapshotの整合を計算する。
fixtureのorigin/row/UTXO entryはDATAで、native acceptanceや通常script検証を成立させたものではない。

採用案の具体API・actual VP/A4/A6の接続箇所は
[A5-positive-funds-proposal.md](../../../../../handoff/positive-court/A5-positive-funds-proposal.md) を参照。
accepted-prefix/import世代、canonical rate/recipient、全未精算due/返還可能額、live UTXO/lock、
Court全体の一回払いmarker、native single-batch/undoはactual ownerの接続が必要である。
