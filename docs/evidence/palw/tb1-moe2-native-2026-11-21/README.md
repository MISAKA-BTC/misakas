# TB1 の凍結ブロックと MOE2 の登録時市場

基点は `misakas/pre` の `536ba9b1e`。元の Studio claim は DAA 7615 に Final、7685 に Gate C 認証に達した。使用期限 DAA 8330 までの21枚のうち当選は0枚で、この claim から消費できる権利はない。[元の抽選台帳](../studio-main-claim-2026-11-20/result.json)

## TB1

同じ試験バイナリの3プロセスが、独立した DB で同じ6ブロックを受理した。3者の凍結ブロック、選択鎖先端、QualRoot が一致し、3つの異なる holder 鍵の署名が検証を通った。別分岐への移行後、親ノードでは元の凍結ブロックが選択鎖に残っていても finality が撤回され、holder は公開を停止した。journal を開き直しても停止は続いた。[各プロセスの PID、root、finality 撤回と試験結果](native-three-process.txt)

holder は roster の鍵と有限体座標、期限内の VSS 終了記録、有効な Ready の dealer と holder の組、全 qualified dealer、準備期限と予約参照から SHA3-512 の QualRoot を計算する。Ready の配送順、再送、署名に用いた乱数を変えても同じ集合から同じ root を得る。

隔離試験の凍結ブロックは coinbase の miner data に64バイトの commitment を載せる。用途別 RoundRef、QualRoot、選択親、consumer 集合 root、policy root が入力になる。RoundRef は network と genesis、roster、epoch、用途、round、schedule、閉鎖 blue score、公開開始 DAA と原期限を固定する。holder は自分のノードの network と genesis を照合し、受理したブロック本文から通常の coinbase codec で commitment を読む。署名前にはそのブロックの native finality、公開期間、永続 journal を再確認する。

3者の署名前強制終了・再起動試験では、元の3署名が揃い、seed は `99df6ee58636a43cf174f09e4f60eac5c81ac8e5e7addcc21b7b00de72a06f4e` になった。この seed は custody 試験固有の入力に対応する。[holder の再起動と seed の検証ログ](holder-custody.txt)

VSS の終了記録、資源予約参照、consumer と policy の内容、成熟した仕事と pruning frontier には試験入力を使用する。ブロックの受け渡しにはファイルに保存した RPC raw block を使う。通常の P2P 通信、新規 FOW1 claim の推論・Panel・Court・受賞ブロックはこの試験の対象外。FOW1 採掘には、関数系列に適合する descriptor と G-COLL・G-COST の成立条件を満たす必要がある。[RFC-0062](../../../rfc/0062-palw-function-output-and-work-admissibility.md)

## MOE2

隔離した状態遷移は、追加モデルの登録ブロックの delta に class と初期市場を一緒に書き込む。分類には登録時点の native catalog、登録者、モデル ID、artifact root を使う。core の登録では市場行を作らない。決済帳簿は delta の旧値・新値とブロック後の市場を照合する。登録を巻き戻すと市場も同じ遷移で取り消される。

報酬原資を 100,000,000,000,000 sompi とした試験では、30%で 14,563 Position を購入した。20%と 9,425 Position を永久 LP に入れ、残る 5,138 Position を burn した。試験の配分プランに従い、残額のうち 40,000,000,000,000 sompi を miner、10,000,000,000,000 sompi を Panel に計上した。独立に3回実行した登録後の state root と金額を記録した。報酬原資と対象 claim は fixture で、検証範囲は状態遷移とローカル決済帳簿まで。新規 claim の原資拘束、3ノードでの worker UTXO 生成・決済は未検証。[市場と配分の観測値](market-observation.json)

## 再現コマンド

TB1 の core 9件、holder の custody 5件、native の3プロセス試験1件、MOE2 の8件が通った。次のコマンドをリポジトリのルートで実行する。[core と MOE2 の検証ログ](core-tests.txt)

```sh
export CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=target/protected-core-tests RUST_MIN_STACK=67108864
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_tb1 -- --nocapture
cargo +1.93.0 test --locked --offline -p misaka-palw-remote --lib tb1_exposure -- --nocapture
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib native_withdrawal_halts_an_exposed_holder -- --nocapture
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib moe2 -- --nocapture
```
