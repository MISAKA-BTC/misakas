# A7 runner失敗診断の改善

source `c3a5084fa7798602c67182b98c8a8e8c77d97948`。driverのmanifestにrunnerが報告したerror・phase・status・CPU開始フラグ・quote上界・要求capを保存する。既存の厳密なschema/input/caps/roundtrip検査を維持し、errorが成功statusに隠れても拒否する。自動のcap拡大・再実行は行わない。

Rootの既存4GiB失敗JSONをread-onlyで再解析した。原本は不変、3件とも `RESOURCE_LIMIT_REPORTED`、phase=quote、CPU開始=falseの記録を維持した。[exact error/hash/診断](root-failure-replay.json)。下表は論理quoteでありRSS実測・料金・native予算ではない。外部宣言2GiBを含む。

| モデル | 論理operation上界 B | 要求cap B | 超過 B |
|---|---:|---:|---:|
| Smol360 | 6721991467 | 4294967296 | 2427024171 |
| Smol135 | 4972195946 | 4294967296 | 677228650 |
| Pythia70 | 4587572532 | 4294967296 | 292605236 |

pure/mock guardは既存12＋診断6の18件が成功した。合成値の境界検査であり、モデルprocess/Rust buildは0回。[source hash・観測したcommand結果・独立レビュー](review.json)。この追加変更ではRust/Cargo・新quote・推論を実行していない。Rootの後続8GiB quoteやactual CPU runは別の原ログ・候補として扱う。
