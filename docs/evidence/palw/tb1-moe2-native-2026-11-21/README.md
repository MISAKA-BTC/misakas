# TB1 の凍結ブロックと MOE2 の登録時市場

基点は `misakas/pre` の `536ba9b1e`。元の Studio claim は DAA 7615 に Final、7685 に Gate C 認証に達した。使用期限 DAA 8330 までの21枚のうち当選は0枚で、この claim から消費できる権利はない。[元の抽選台帳](../studio-main-claim-2026-11-20/result.json)

## TB1

同じ試験バイナリの3プロセスが、独立した DB で同じ6ブロックを受理した。3者の凍結ブロック、選択鎖先端、QualRoot が一致し、3つの異なる holder 鍵の署名が検証を通った。別分岐への移行後、親ノードでは元の凍結ブロックが選択鎖に残っていても finality が撤回され、holder は公開を停止した。journal を開き直しても停止は続いた。[各プロセスの PID、root、finality 撤回と試験結果](native-three-process.txt)

holder は roster の鍵と有限体座標、期限内の VSS 終了記録、有効な Ready の dealer と holder の組、全 qualified dealer、準備期限と予約参照から SHA3-512 の QualRoot を計算する。Ready の配送順、再送、署名に用いた乱数を変えても同じ集合から同じ root を得る。

隔離試験の凍結ブロックは coinbase の miner data に64バイトの commitment を載せる。用途別 RoundRef、QualRoot、選択親、consumer 集合 root、policy root が入力になる。RoundRef は network と genesis、roster、epoch、用途、round、schedule、閉鎖 blue score、公開開始 DAA と原期限を固定する。holder は自分のノードの network と genesis を照合し、受理したブロック本文から通常の coinbase codec で commitment を読む。署名前にはそのブロックの native finality、公開期間、永続 journal を再確認する。

3者の署名前強制終了・再起動試験では、元の3署名が揃い、seed は `99df6ee58636a43cf174f09e4f60eac5c81ac8e5e7addcc21b7b00de72a06f4e` になった。この seed は custody 試験固有の入力に対応する。[holder の再起動と seed の検証ログ](holder-custody.txt)

VSS の終了記録、資源予約参照、consumer と policy の内容、成熟した仕事と pruning frontier には試験入力を使用する。ブロックの受け渡しにはファイルに保存した RPC raw block を使う。通常の P2P 通信、新規 FOW1 claim の推論・Panel・Court・受賞ブロックはこの試験の対象外。FOW1 採掘には、関数系列に適合する descriptor と G-COLL・G-COST の成立条件を満たす必要がある。[RFC-0062](../../../rfc/0062-palw-function-output-and-work-admissibility.md)

### 署名公開から EXEC seed まで

同じテストバイナリの3つのプロセスを同時に動かし、各 DB が凍結ブロックの finality を観測した後に holder が署名した。各署名は、round、QualRoot、FreezeId、holder の slot、全 qualified dealer の share 値を束縛する。holder 間の試験用 Unix ソケットで実データを運び、選択鎖の1ブロックに全署名と署名対象の順序付き digest を載せた。後続ブロックには、公開ブロックのハッシュ、全 dealer の係数、支持する署名の位置を束縛する certificate の digest を載せた。

3者はそれぞれ、固定済みの roster と公開データから署名を検査し、全 dealer の多項式を復元し、両ブロックの commitment と元の DAA 期限を照合した。得られた EXEC seed は3者とも `956c3261087432dbe72221d4c3c536c706a5bf8ce59ff9f3ace3bd0b2bd4c10b`。後続ブロックが伸びた後にも、親ノードは元の certificate を指定して同じ値を読めた。署名データの欠落・並べ替え、certificate 公開前の読み取り、別ブロックの指定、finality 撤回後の読み取りは拒否された。[各 PID のブロックハッシュ・seed・実行ログ](native-release-certificate.txt)

初回は追加の heartbeat が native の時刻上限を超え、公開ブロックの受理前に試験が止まった。記録した実行には、この隔離チェーンで既に有効な attempt ブロック2個を使った。新規 FOW1 の仕事からは独立している。consumer と policy、VSS と成熟済み仕事には前節と同じ試験入力を使用した。この reader は安定化を現地で観測したプロセス向けで、後から同期したノードの過去の finality、公開データのネットワーク配信、FOW1 の推論や合意 credit は未検証。隔離版の位置表現は DAA が32ビット、blue score が30ビットの範囲で動作し、範囲外の値を拒否する。

### 署名した新しい仕事の入力から実モデル推論まで

ローカルの新規仕事について owner が登録文脈に署名した。登録文脈は EXEC と PANEL の RoundRef、モデル・class・tokenizer、job token の root、nonce、受益者、target、会計・資料 policy の参照、実行量の上限と元期限を含む。試験ではこの1件に slot 7 を割り当てた。署名を検査した登録集合と資金予約参照を EXEC 凍結ブロックに結び付けてから holder が share を公開した。元の署名のまま job token を変更した入力と、同じ登録に2個目の slot を割り当てた集合は拒否された。

3つのプロセスはそれぞれ証明書を選択鎖から読み、登録文脈と EXEC seed から試行 ID を導出した。入力の先頭には BOS・POW、試行 ID の各バイトを固定表で変換した32個の token、SEP を置き、元の job token 3個を続けた。3者とも同じ38個の token を TIR エンジンに渡した。各プロセスが元の Studio と同じモデルファイルを開き、inventory root・class・tokenizer・語彙数・context 上限を照合した。モデルは1個の token ID `76` を生成した。3者の input・execution・trace・output root と、列挙して確認した191,967枚の trace leaf の件数が一致した。推論の開始時と結果を返す直前に、元の証明書の選択鎖所属、finality と期限を再読する。試験の後半で finality が撤回されると、既に導出した入力も使用可否の再確認で拒否された。[3プロセスの入力・生成値・root・finality 撤回ログ](registered-exec-inference.txt)

資金予約、難易度・会計 policy、VSS、成熟済み仕事は試験入力である。今回認証したのは凍結ブロックに含めた署名済み入力集合で、native の資金拘束と全登録の収集、独立した PANEL round の PREPARED・抽選、FOW1 の採掘出力 z と費用条件、Court・Final・受賞・決済は未検証。今回の生成値は TIR の通常の回答 y。元の Studio claim の署名・権利は新しい仕事に使用していない。

実モデルを伴う再現には、SHA-256 が `48c72278b0d9b45c7ab92b6e70f5fbcef47fa86cc8644cd4307798354639fe90` の `model.palwtir`（180,336,320バイト）を所持する環境で、下記の native 試験を実行する際に `MISAKA_PRIVATE_TB1_TIR_ARTIFACT` へそのパスを指定する。変数を省略した通常の試験では、登録・入力の導出・分岐後の停止までを検査する。

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
