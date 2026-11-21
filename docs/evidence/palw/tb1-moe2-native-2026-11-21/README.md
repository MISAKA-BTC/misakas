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

### EXEC・PANEL 両方の資格記録を署名付き登録に含めた試験

隔離3プロセスが、EXEC と PANEL それぞれについて3 dealer × 3 holder の署名付き Ready から別々の QualRoot を計算した。最初の heartbeat は、2個の RoundRef とそれぞれの QualRoot から計算した64バイトのハッシュを coinbase に記録した。登録者はそのブロックのハッシュと両ラウンドの記録に署名した。各プロセスはブロック本文と選択鎖上の順序を照合し、次の heartbeat で登録集合を凍結した。PANEL の資格記録への EXEC 記録の流用、準備ブロックより古い登録アンカー、別の PANEL 記録に対する登録者署名はいずれも拒否された。登録集合が空でも、準備ブロックの記録は必須になる。

初回は凍結記録を attempt ブロックに載せたところ、そのブロックが新しい claim を開き、holder は finality を確認できず share を公開する前に停止した。heartbeat を使った再試験では、3プロセスが同じ EXEC seed を復元し、38個の入力 token から通常の回答 token ID `198` を生成した。input・execution・trace・output の各 root と trace leaf 数 191,967 が一致した。分岐後には元の凍結ブロックが選択鎖に残ったまま finality が撤回され、holder は公開を停止した。[バイナリとモデルの SHA-256、各プロセスの観測値](joint-qualification.txt)

同じバイナリで別の隔離チェーンに Studio モデルを登録した。3プロセスで登録直後の市場と state root が一致し、再編成後には登録者の担保 100,000,000 sompi が戻った。

今回の新しい隔離仕事の期限は DAA 40、PANEL の公開予定期間は DAA 20 以上 30 未満。VSS 終了記録・資源予約・資金参照は試験入力である。両ラウンドの資格記録を登録時点で固定するところまでを検証した。独立した PANEL seed の公開と抽選、選出者の完全再実行、FOW1 出力 z、Court・Final・受賞・決済は引き続き未検証。

## MOE2

隔離した状態遷移は、追加モデルの登録ブロックの delta に class と初期市場を一緒に書き込む。分類には登録時点の native catalog、登録者、モデル ID、artifact root を使う。core の登録では市場行を作らない。決済帳簿は delta の旧値・新値とブロック後の市場を照合する。登録を巻き戻すと市場も同じ遷移で取り消される。

報酬原資を 100,000,000,000,000 sompi とした試験では、30%で 14,563 Position を購入した。20%と 9,425 Position を永久 LP に入れ、残る 5,138 Position を burn した。試験の配分プランに従い、残額のうち 40,000,000,000,000 sompi を miner、10,000,000,000,000 sompi を Panel に計上した。独立に3回実行した登録後の state root と金額を記録した。報酬原資と対象 claim は fixture で、検証範囲は状態遷移とローカル決済帳簿まで。新規 claim の原資拘束、3ノードでの worker UTXO 生成・決済は未検証。[市場と配分の観測値](market-observation.json)

### Studio と同じモデルの署名付き登録と再編成

Studio で使った `model.palwtir` を登録者が開き、実際の重みから inventory root を読み、class とともに新しい隔離チェーンへの登録に署名した。登録は mempool とブロックテンプレートを経由し、DAA 1 の遷移で class と市場の両方が記録された。市場の初期値は実準備金 0 sompi、仮想準備金 1,000,000,000,000,000 sompi、在庫 500,000 Position。登録者の bond から 100,000,000 sompi が差し引かれた。

同一バイナリの3プロセスは別々の DB を使い、登録ブロックで同じ state root を得た。受信側の2プロセスにはモデルファイルを渡さず、ファイルで受け渡した RPC raw block を通常のブロック検証に投入した。登録を含まない重い分岐に切り替えると、3者とも同じ state root に収束し、class と市場が消え、登録時の担保差引が戻った。[バイナリ・モデルの SHA-256、3プロセスの記録](studio-model-registration.txt)

同じバイナリとモデルで別の TB1 EXEC 試験も再実行した。3プロセスの回答 token はいずれも 76 で、入力・実行・trace・出力 root と 191,967 枚の trace leaf 数が一致した。登録試験と推論試験はそれぞれ専用の隔離チェーンを使用した。

市場の生成を選択するのは、この試験用の識別子を持つ隔離 genesis で、native 市場とモデル catalog が両方とも有効になった時点から。試験は fixture の鍵・残高と proof-of-work 省略設定を使った。通常 P2P 配信、新規 claim の受賞と、その原資からの買付・永久 LP・worker UTXO 決済は未検証である。

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

登録テスト群は同じバイナリで計25件を検証した。一括実行で24件が成功し、RocksDB 計測は保存先の指定漏れで停止した。未使用の `TKR_DB_MEASUREMENT_DIR` を指定した再実行で、この1件も成功した。

上記と同じ環境変数設定で、3プロセスでの署名付き登録と再編成を再現する。引数のパスは上記 SHA-256 のモデルを指定する。モデル指定を省略すると同じ手順を小さい TIR コーパスで実行する。

```sh
MISAKA_PRIVATE_TB1_TIR_ARTIFACT=/path/to/model.palwtir \
  cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --nocapture
```

## MOE2 の報酬予約と引き落とし

追加モデルの登録を受理した3プロセスが、それぞれの選択鎖から市場、資金提供 bond の公開鍵・担保、予算 policy を読み取った。登録者とは別の試験 bond が claim、model/root、資金提供 bond、policy、quote anchor、期限、数量・価格制限、worker 配分を束縛する予約キーに ML-DSA-87 で署名した。このキーを登録後の heartbeat に収録し、別の後続ブロックを会計処理位置に使った。各ピアが登録時の鍵で署名を検査した。期限を変更すると署名は一致しなかった。

この試験では登録時点の native 予算台帳を複製し、通常の予約 API で試験用の worker 報酬 160 をモデルと bond に予約する。MOE2 の決済処理はこの予約から 160 を一度だけ引き落とした。買付 48 で 2 Position を取得し、永久 LP に 32 と 1 Position を入れ、余剰の 1 Position を burn した。残り 80 の支払権利は採択済み試験プランに従って miner 64、Panel 16 となった。部分的な引き落とし、従来の claim ID の予約流用、決済の再送は拒否された。予算 journal の単一書き込みを逆適用すると、複製した台帳は引き落とし前と完全一致した。[8件の core テスト、3ピアのブロック・予約キー・会計値](reserved-reward.txt)

経済条件の収録ブロックと MAS1 停止証明は、同じ登録ブロックまでを再生した別々の DB で短い再編成を検査した。どちらも3者が同じ代替 tip と登録の巻き戻しを確認した。二つの履歴を直列につないだ先行試験では、対抗分岐に heartbeat を追加した場合も、試験用 native attempt を追加した場合も、元の停止証明が選択鎖に残った。深い分岐で停止が解除されたという結果は得ていない。

選択鎖に収録したものは資金提供者が署名した条件の digest である。報酬額と成熟・支払期限には隔離 fixture を使用した。複製上の市場・支払権利・予約消費から、受賞ブロックによる発行、選択鎖の残高更新、worker UTXO 支払いを認定しない。再現コマンドは上記の `private_moe2_three_process_registration` と `--lib moe2` を使う。

## 選択鎖から外れた MOE2 決済の復元

`private_moe2_three_process_registration` では、3ピアがそれぞれ登録ブロックの後に、資金提供者が署名した条件の収録ブロックと、決済位置に使う後続ブロックを受理した。各ピアは両ブロックの native delta を元の状態に適用し、ノードが保存した二つの root と照合した。その後、隔離用の経済状態に予約・引落・市場更新を加え、状態、delta、決済帳簿、1,219 バイトの巻き戻し記録をディスクに保存した。

別の分岐が選ばれて両ブロックが選択鎖から外れたことを確認した後、各ピアがディスクから経済状態を読み直した。巻き戻すと、モデル残高、永久 LP、worker 支払権利、元の受付・予約状態が決済前と一致した。二つの delta を逆順に戻すと元の登録ブロックの native root に戻り、保存した分岐の再生で決済後の root と帳簿が一致した。別の親の root を指定した復元、サイズ制限を超えた入力、同じ決済の二度目の巻き戻しは拒否された。[3ピアの root・決済額・実行記録](settlement-reorg.txt)

ブロックに収録された経済データは元の署名条件の digest まで。経済状態の root は隔離試験が別途保持した値である。この試験で再読したのは保存済みの状態と差分で、ノード全体の再起動・IBD、新規採掘 claim の受賞による原資発行、worker UTXO の生成は検証範囲外である。

## 実行ブロックの native 状態を使った決済

隔離用の `settle_private_moe2` は、決済位置にある class・市場と元の登録を照合し、その位置の予約行・消費済み額・担保・他債務・引出制限・export 上限から会計条件を得る。既存の combined liability 判定を通過すると native 状態の予約と市場を更新し、同じブロックの delta に両方の書き込みを追加する。試験では delta の再適用で同じ状態 root を再現した。予約を欠く元の状態、配分を超える Panel 支払い、公開 T12 設定、二重実行をそれぞれ拒否した。

3ピアは各自の予約・市場・帳簿から計算した checkpoint を、後続の選択鎖ブロック本文に収録したハッシュと照合した。ハッシュは network、署名条件の予約キー、登録条件の収録ブロック、実行位置、経済状態 root、帳簿 root を束縛する。別の親 root、改変した帳簿、実行ブロック自体を後続ブロックとした読取りは失敗した。別分岐の選択後、元の本文にハッシュが残っていても reader は決済を現行状態として返さなかった。[3プロセスの観測値と同一バイナリでの回帰試験](native-accounting.txt)

原資予約と成熟済み支払条件は隔離用の入力である。後続ブロックが収録するのは経済状態のハッシュで、ブロックヘッダーがこの経済状態を合意 root とした実測や、新規当選 claim の coinbase・worker UTXO の発行は含まない。再現には前節の `private_moe2_three_process_registration` を使う。
