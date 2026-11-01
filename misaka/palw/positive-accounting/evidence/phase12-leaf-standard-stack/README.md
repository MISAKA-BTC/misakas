# A5 leaf library regression — standard test stack

2026-11-01。対象commit `2e63b142108e9fca6697aa43f7751dceb524e089`。

`cargo test -p misaka-palw-positive-accounting --lib --offline --locked` は42 passed / 0 failed / 0 ignored / 0 filtered out、exit 0。
既存A5専用target `/Users/wata/.cache/misaka-positive-accounting-node-target` を使用した。
`CARGO_BUILD_JOBS=1`、dev/test debug=0、incremental=0、`RUST_MIN_STACK`未設定。
候補・Cargo.lock・関係sourceのSHA256、実行前後一致、raw log SHA256は `leaf-lib-test-result.json` に固定した。

対象はleafの信用・private会計lib試験だけである。node crateにある同一block時計修正と新しい2試験はこの実行に含まれず、未compile・未実行のままである。
過去の55件・A7の667件、最新A0候補、native資金・P承認・全consumer・復旧の成功を示さない。

A0の追加指示に従い、未開始のClippyは実行しなかった。node/Core/C++のbuild、新target作成、他担当cacheの操作は行っていない。
初回の限定lib試験は成功し、失敗ログの差替え・再実行はない。
