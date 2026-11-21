# MAS1・QF1 改訂の統合と隔離試験

2026-11-21 に受領した `misaka_docs_MOE2_MAS1_QF1_20261120.zip` の SHA-256 は `95f460df28fd5a8f7aad2ce50f12f0c20d7cbb15a3de3958523f1de58ac41303`。展開した3,094ファイルがすべてリポジトリの同じパスとバイト一致した。統合前の台帳にあった Studio 実行記録は対応する[隔離試験の記録](../studio-winning-receipt-2026-11-20/README.md)に保存した。

最新版の改訂検査は入力3,012件、変更51件、履歴保存51件を照合した。さらに同梱の変更前ファイルを SHA-256 で検査して MAS1 版を復元し、その改訂検査も実施した。QF1 の Python 参照検査は57項目成功。過去の原本と照合する RFC・ADR の全体検査には配布版と同じ7件・157件の指摘が残る。文書全体の strict 検査にも配布原本から引き継いだ指摘が残っている。

## QF1

RFC-0066 の入力試験は Qwen3-8B の tokenizer をリビジョン `b968826d9c46dd6066d109eabc6255188de91218` で固定した。ファイルの SHA-256 は `aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4`。固定した3区間と通常の入力文 `2 + 2?` を個別に符号化した。元の Studio モデルの byte-token 入力とは署名対象の domain と instance が異なる。隔離 QF1 登録の model/program・資金参照は試験値で、実際の QF1 インスタンスの登録を意味しない。[固定した tokenizer](https://huggingface.co/Qwen/Qwen3-8B/blob/b968826d9c46dd6066d109eabc6255188de91218/tokenizer.json)

Readout の試験は配布版の合成テンソルを用いる。1回答位置について36層の第二残差と最終 norm に相当する37本の各4,096値を順に符号化する。303,141 バイトを75個の chunk に分け、前後の BLAKE2b 状態、counter、最終フラグを照合する。四つの root の順は X、Y、Z、TRACE。合成 TRACE を用いた Ticket は符号化の比較値である。入力試験と合成テンソル試験は別の実行である。MAS1 の G-USE 評価には専用の TB1 MODEL_AUDIT と後続の PANEL が必要になる。

## MAS1

新規採掘の 6項目の判定 について、FAIL は REJECTED、UNKNOWN は REVIEW_HOLD とする。未解決 case が原期限を迎えると REJECTED_UNRESOLVED に移る。隔離版の MODEL_STOP reader は元の roster、case、停止証明の順序と対象を選択鎖のブロック本文で照合する。登録者が選んだ候補を権限の根拠にせず、この試験では専用署名鍵と許可された roster の digest をあらかじめテスト設定で固定する。

3署名は隔離 fixture の異なる鍵で生成する。3ピアは同じ署名をそれぞれ検査する。対象者4人のうち故障・共謀が最大1人という BFT 前提は試験値。実ネットワークの roster 選出、evidence 資料の可用性、停止と解除の永続 reducer、実インスタンスの G-USE・G-COLL・G-COST、公開有効化とは別の検証である。

## 実行結果

同じ native バイナリで二つの隔離チェーンを試験した。TB1 チェーンでは3プロセスが同じ EXEC 証明書から QF1 入力111 token を導出した。独立した Hugging Face tokenizers 0.23.3 と各ピアの全111 ID が一致した。同じ登録集合の従来形式を選んだ Studio モデルは回答 token 95 を返した。QF1 の新規採掘登録は REVIEW_HOLD で拒否された。

MOE2 チェーンでは3プロセスが市場の生成、異なる鍵による3署名の MODEL_STOP、停止後も市場が存在することを確認した。2署名、重複 slot、別 descriptor は拒否された。より重い分岐に切り替えた後は、各プロセスが元の停止証明の選択鎖所属を再検査し、適用を拒否した。[バイナリの SHA-256・各ピアの観測値・回帰試験](observations.json)

## 再現

リポジトリのルートから実行する。3プロセスで実 tokenizer を照合する際は、上記のファイルのパスを `MISAKA_PRIVATE_QF1_TOKENIZER` に指定する。併走する従来の Studio モデル推論には [TB1 の実行記録](../tb1-moe2-native-2026-11-21/README.md)に記載した `MISAKA_PRIVATE_TB1_TIR_ARTIFACT` を指定する。

```sh
export CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=target/protected-core-tests RUST_MIN_STACK=67108864
python3 -B docs/maintenance/revisions/2026-11-20-qwen-fow1/check_revision.py
python3 -B docs/design/palw/models/qf1/check_qf1_reference.py
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_qf1
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_mas1
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib native_withdrawal_halts_an_exposed_holder -- --nocapture
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --nocapture
```

## 署名済み EXEC 入力からの QF1 数値再実行

隔離試験では、小型の状態付き TIR を PALWTIR1 ファイルとして各ピアで開く。登録者の署名は、このファイルの inventory、class、tokenizer、変換資料の digest、各層の出力 node と scale 参照を固定する。QF1 の読み取り位置は36個の layer carry と、最後の carry から logits に至る post block の committed node である。ピアは登録時の上限を再確認し、空の状態から全入力を処理する。予測位置では全151,936 logits 上の greedy 選択を、同じ TIR が出力した TopK(1) と照合する。

試験用 TIR は token に応じた累積和を持つ。各 lane に周期7の値を加え、層ごとに1、最後の読み取り位置で5を加える。4つの token に対応する logit の符号を固定して、最初の予測でも EOS の予測でも同点を発生させる。これにより、入力の取り落とし、層・lane の誤順序、同点の選択、EOS 後の余分な forward を独立に検出できる。TIR の演算は fixture 用で、Qwen の attention・MLP・RMSNorm の適合性や G-USE は未検証。

[3ピアの実行記録](prediction-run.txt)には native 証明書、全入力 ID、各回答、Readout、既存 Studio モデルの並行試験を収録した。[独立照合の結果](prediction-check.json)にはバイナリの SHA-256 と処理位置を記録した。別 scale の instance、従来形式の登録、元の finality を失った登録での再実行は拒否された。1 token での上限停止、最初の EOS、上限到達と同時の EOS、leaf 予算不足を別の回帰試験で確認する。

再現には前節と同じビルド環境と tokenizer を使う。次の最後のコマンドは、観測した各入力から全 lane を Python で導出して配布版の Readout と照合する。第2引数には Cargo が `Running unittests` 行に表示したテストバイナリを指定する。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib qf1_stateful_prediction_boundaries
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib native_withdrawal_halts_an_exposed_holder -- --nocapture > /tmp/qf1-prediction.log 2>&1
python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_prediction.py /tmp/qf1-prediction.log "$TEST_BINARY"
```

## 署名済み Original の受領と PANEL 対象の固定

この隔離試験では、前節の状態付き試験用 TIR が実行中に出力する全 commit を、RFC-0066 の順序で記録する。Init に続いて各位置の pre・36層・予測位置の post の値を64 lane ごとに符号化し、greedy 選択、停止、Readout の各 chunk、End を記録する。署名者は回答長を最初の実行で確定する。各ピアは登録時の上限を検査し、同じ入力から再実行して宣言された回答長と実際の停止を照合する。四つの root と Ticket はパディングを含めて逐次計算する。

登録者の ML-DSA-65 署名は、チェーン、登録キー、スロット、試行、四つの leaf 数と root、Ticket を束縛する。受領を確認するノードは EXEC 証明書より後の選択鎖ブロックにその Original ID が含まれることを検査する。後続の PANEL 凍結ブロックは、時刻表で決まる位置に、対象 Original と受領ブロック、候補と重み、方針、予約参照を固定する。登録資金・資料予約・候補者の適格性はこの隔離試験の入力である。試験後に親ピアを別の分岐へ切り替えると、元の受領ブロックと PANEL 凍結ブロックの読み取りはいずれも拒否された。

[3ピアの記録](original-run.txt)と[Python による照合結果](original-check.json)を収録した。照合には受領した仕様に付属する Merkle・Readout・Ticket の参照関数を使う。Init の登録コンテキスト digest は記録から取り込み、残りの Init フィールドと後続全レコードは入力 token と試験用 TIR の数式から再構成する。次のコマンドは保存済みの記録にも適用できる。

```sh
python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_original.py \
  docs/evidence/palw/mas1-qf1-native-2026-11-21/original-run.txt
```

この記録の再実行は3つの試験プロセスによるもの。PANEL 凍結の native finality と公開開始 DAA が揃わず、PANEL share の公開は保留された。このローカル推論では登録前に固定した閾値 2^255 と Ticket の 256 ビット値を比較して記録する。比較結果による報酬・権利の発行は行わない。抽選された担当者による承認、Court、採掘 claim の Final・受賞・決済は未検証である。

## 独立した PANEL seed から選出者の完全再実行まで

この追加試験は、前節の保留・再編成試験とは独立した隔離チェーンで実行する。新しいローカル推論の登録時に難度を D=1、閾値を 2^256、仕事の期限を DAA 60 に固定した。登録後の閾値変更や再抽選はない。3ピアはそれぞれ専用 DB で同じ heartbeat 列を受理し、EXEC は DAA 4、PANEL は DAA 26 から share を公開できる。最初の試験では EXEC の finality 観測時の DAA が3で、holder が署名を拒否した。記録した試験では、各ノードが native DAA を確認し、原スケジュールの公開開始まで heartbeat で進めた。

PANEL 用には、準備ブロックの前に EXEC と独立した dealer 多項式を生成する。隔離試験の provisioner が holder ごとの share を配り、多項式の digest が Ready と QualRoot に入る。完全な VSS 通信、専用の取引手数料・担保はこの実行の対象外である。成熟済み仕事と pruning frontier は既存の RFC-0012 隔離 fixture の入力を用いた。選択鎖、DAA、凍結ブロック、受領ブロック、finality と release 証明書は各ピアが native API から検査した。

3つの検証者プロセスは各自の ML-DSA-65 鍵を生成し、公開鍵だけを交換する。PANEL の対象を固定するブロックは、同じ署名済み Original、候補3名の ID、試験用重み 1・2・4、既存の整数重み付き抽選、2席すべての完全再実行義務を束縛する。選出された2者は公開済みの入力から試験用 TIR を再実行し、四つの root、回答長、Ticket と登録者署名を照合してから署名した。残り1者の署名要求、他の席の署名、欠落・重複した返答は拒否された。全3者が後続の選択鎖ブロックから2署名の受領を確認した。

[3プロセスの記録](panel-run.txt)と[独立照合の結果](panel-check.json)には、両 seed、凍結・安定化・証明書・返答ブロック、選出者を収録した。独立照合は Python で全 trace を再構成し、元の Original、凍結時の候補重み、PANEL seed から全候補の抽選順位を計算する。Court のうち最後の回答と Stop の関係は次節で検証した。実 Qwen3-8B の採掘資格、正規の資金予約、claim 全体の終端、Final、受賞、決済は未確認である。

```sh
# 前節と同じ tokenizer・ビルド環境を指定して起動する
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_stable_panel -- --ignored --nocapture
python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_panel.py \
  docs/evidence/palw/mas1-qf1-native-2026-11-21/panel-run.txt
```

### 最後の回答 token と Stop を結ぶ局所 Court

選出者の返答が受理された後、同じ署名済み Original に「最後の回答と停止記録が一致しない」という異議を提出する試験を追加した。資料提供側が最後の回答と Stop の Merkle path を配る。各ピアの裁定処理はモデルの再実行を必要としない。登録者署名と選択鎖の Original 受領を確認し、回答の index を T−1、Stop の index を trace leaf 数−Readout chunk 数−2 と計算する。根は元の Y と TRACE を使う。長さ、処理位置数、最終 token、EOS 優先の停止理由を照合し、独立した後続の case・verdict ブロックの順序と本文を読む。

この Original は正しく EOS で停止したため、異議の結果は `ChallengeNotEstablished`。署名済みの根と整合しない資料は裁定前に拒否する。別の境界試験では、認証された値自体が誤っている場合を `ComputationRefuted` と判定する。対象は最後の回答と Stop の関係であり、途中の EOS・全 logits の選択・全層の算術、資料欠落時の制裁、すべての case の期限・会計・Final にはそれぞれの検証が必要になる。[開示サイズ・元の index・裁定記録](stop-check.json)

```sh
cargo +1.93.0 test --locked --offline -p misaka-palw-remote --features private-tb1-tir --lib authenticated_stop_distinguishes_invalid_witness_from_committed_contradiction
python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_stop.py \
  docs/evidence/palw/mas1-qf1-native-2026-11-21/panel-run.txt
```

## 上流 Qwen3-8B の形状照合

固定リビジョンの config、分割 index、5個の safetensors header を取得した。config と index の内容を当該 Git オブジェクト ID と照合し、RFC-0066 §1 の36層、Q/K gain の各128値、Q/K/V/O の向き、MLP、独立した embedding と LM head、最終 gain まで全399テンソルの名前・shape・BF16型を検査した。各 shard のデータ領域の offset は重複・欠落がなく、合計8,190,735,360パラメータ、16,381,470,720バイトで index と一致した。

[実行結果](upstream-geometry.json)には上流が公表した各 shard の LFS SHA-256 を収録した。今回の読み取りは header までで、重み全体の再ハッシュ、整数化された QF1 artifact、推論結果には各々の検証が残る。再現は `huggingface_hub==0.36.2` を利用できる Python で `python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_qwen_source.py` を実行する。[固定した上流リビジョン](https://huggingface.co/Qwen/Qwen3-8B/tree/b968826d9c46dd6066d109eabc6255188de91218)

## 実際の出力ヘッドの整数化と照合

上流の第5 shard は独立した `lm_head.weight` だけを収録する。全1,244,659,840バイトの SHA-256 が固定リビジョンの LFS digest と一致した。既存の TIR lowerer で151,936行×4,096列の BF16 重みを出力チャネルごとの absmax/127 で i8 に変換し、各行の倍率を TIR の整数 multiplier・shift へ落とした。

この試験の hidden ベクトルは `((73*i+19) % 2047)-1023`、入力 scale は 1/1024、logit scale は 1/4096。全行について整数内積の native 版と scalar 版が一致した。Python は元 shard を別途読み、全 i8 重み、全倍率、全整数 logit の digest と境界4行の値を独立に照合した。最大 logit は18,754、該当する行は67,155の1件だった。入力と出力 scale はこの演算の試験値で、Qwen 全36層が出した hidden、実回答 token、採掘資格の測定はこの試験の対象外である。[全体 digest と境界行](output-head.json)

`SHARD` に固定リビジョンの `model-00005-of-00005.safetensors` を指定して実行する。Python 側は NumPy を使用する。

```sh
cargo +1.93.0 run --locked -p misaka-palw-tir-lower --example check-qf1-output-head -- "$SHARD" > /tmp/qf1-head.json
python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_output_head.py "$SHARD" /tmp/qf1-head.json
```

## 元の全重みの照合

上流の固定リビジョンに対して、今度は5 shard の全16,381,516,776バイトを読み、各ファイル全体の SHA-256 が上流の LFS digest と一致することを確認した。399テンソルそれぞれについてデータの型・形状・長さ・offset と実データの SHA-256 を記録した。145本の norm gain は別途取り出し、保存した全616,448バイトをこの記録と照合した。[上流全399テンソルの実データの SHA-256](upstream-payloads.json)

`check_qwen_weights.py` は5 shard 全てをストリームで読み、検査が揃ってから結果を出力する。`--local` で指定したディレクトリにある shard はそこから読み、残りは固定リビジョンから取得する。`--vectors` には gain の保存先として新しいディレクトリを指定できる。サンプル実行は `python3 -B docs/evidence/palw/mas1-qf1-native-2026-11-21/check_qwen_weights.py`。ここまでで照合した全体は上流 BF16 の入力である。QF1 の整数化済み36層・scale・table・正規 program とその評価は個別に追跡する。

## EXEC 入力の実 Qwen 埋め込み

固定した上流 revision の入力 embedding 全体（151,936 行、1,244,659,712 バイト）を読み、SHA-256 を [399 テンソルの台帳](upstream-payloads.json)と照合した。隔離ジョブのテンプレート、質問、全16種類の challenge 桁で使う48行を切り出し、その集合のハッシュを試験に固定した。各行を既存の TIR lowerer で量子化し、全行共通の隔離試験用 residual scale `1/32768` で i16 に丸めた。

同じバイナリの3ピアが、その回の証明書から得た EXEC 入力111位置について、各位置の BF16 の4096要素と整数化後の4096要素を token 順に照合した。3ピアで双方のハッシュが一致し、独立の Python 有理数計算が両方を再現した。行の改変と未収録 token の読取りは拒否された。同じ実行で従来の試験重みに対する Panel と Stop Court の再検算も通った。実重みを使ったこの照合の終点は入力 embedding で、36層の実推論、署名した Original の実重みへの切替、採掘資格は未完である。[バイナリ、各ピアの入力と結果](embedding-run.txt)

再現時はこのディレクトリの `check_embedding.py /tmp/qf1-embedding.rows` で行集合を作成する。既存の `private_tb1_stable_panel` の環境に `MISAKA_PRIVATE_QF1_EMBEDDING=/tmp/qf1-embedding.rows` を追加し、出力を保存する。`check_embedding.py /tmp/qf1-embedding.rows --check <ログ>` で独立照合できる。

## 第0層の入力正規化・Q/K/V 投影

`misaka-palw-tir-lower::qf1::attention_input` は第0層の入力 RMS と gain、Q/K/V の3投影、32個の Q head と8個の K head の RMS と gain を通常の整数 TIR に展開する。各 head の幅は128。Q/K の gain は head 間で共通の128要素を使う。隔離試験では上流6テンソルを個別にハッシュ照合し、入力行と変換済みの重みを PALWTIR1 に書き出した。3プロセスはそれぞれそのファイルを開き直して EXEC 入力111位置を実行した。

入力 residual の scale は 2⁻¹⁵、正規化後と Q/K/V は 2⁻¹⁰、gain は整数 Q20 とした。epsilon は元の 1/1,000,000 から各入力の二乗単位の Q24 に変換した。いずれも今回の部分実行に固定した数値である。入力行は検証済み48行の集合を使い、元の token ID との対応を artifact に記録した。3ピアの同一 artifact と六つの段階の出力を、独立した整数計算で照合できる。[3ピアの入力とハッシュ](attention-run.txt)・[段階ごとの照合結果](attention-check.json)

再現時は本ディレクトリで `python fetch_qf1_attention.py <重みディレクトリ>` を実行し、前節の試験環境に `MISAKA_PRIVATE_QF1_ATTENTION=<重みディレクトリ>` を追加する。`python check_attention.py <重みディレクトリ> <埋め込み行ファイル> <試験ログ>` で再計算する。ここで実重みを使った範囲は RoPE の直前までで、KV 履歴、因果 attention、MLP、各層の読出し、採掘用 Original は引き続き未検証である。

## 全399テンソルの整数変換

上流の5 shard を読み直し、各 shard と各 BF16 テンソルの SHA-256 を検証した。変換結果の[整数パラメータ台帳](integer-weights.json)に、shape、dtype、row-major のコード列の SHA-256 を記録した。253行列は出力行ごとの i8、入力 embedding は行ごとの i16、145本の norm gain は little-endian の i32（Q20）である。行スケールは BF16 絶対値最大の little-endian 列でも照合できる。各行の絶対値最大を127または32,767で割ってスケールとし、全ゼロ行のスケールには1を用いた。丸めは HalfAwayFromZero。コードは全体で8,813,989,888バイト、行ごとの最大値は3,409,408バイト相当になった。

出力 head の全 i8 が既存の Rust 変換結果と一致した。保存していた上流 gain 全145本を独立に有理数で Q20 へ変換してコードハッシュを照合した。さらに上流 index・元の重み・整数台帳を使い、第0層の全11テンソルと7本の行スケール列を新しいディレクトリに復元した。18ファイルを再読した合計は193,049,600バイトで、全件が台帳に一致した。先頭 gain の期待ハッシュを改変した試験では、出力ディレクトリを公開する前に復元が失敗した。

再現には本ディレクトリで `python check_integer_weights.py --local <保存済みshardの場所>` を使う。整数台帳から任意の1層を復元するには `python materialize_integer_layer.py <層番号0〜35> integer-weights.json <固定revisionのmodel.safetensors.index.json> <新しい出力ディレクトリ>` を実行する。これらのファイルは入力パラメータである。全36層を実行する登録用 TIR、活性値の全 scale と table、StateMap、推論品質、MAS1 採掘適格性は別途検証する。

## 実重みの第0層と全36層の推論

同一バイナリの隔離3ピアで、TB1 EXEC から得た同じ111入力位置を第0層の末端まで実行した。12か所の中間値を独立した整数計算と照合した。最後の位置の Q/K/V を固定して過去の KV を除くと、attention 出力の4,096成分中3,667成分が変化した。[3ピアの入力と段階別ハッシュ](decoder-run.txt)・[第0層の照合結果](decoder-check.json)

次に単独の Rust evaluator で固定 revision の整数化済み399テンソルを読み、同じ111入力位置から全36層、最終 norm、151,936語の出力 head を実行した。回答上限を3とし、各位置の最大 logit から順に生成した ID は 8820、67942、21856、停止理由は Limit。各予測位置から36層の第二残差と最終 norm を取得した。3位置分の readout 入力は計909,312バイトだった。この全層実行は隔離計測で、署名済みの合成モデルの Original・Panel・Stop Court とは対象が異なる。[全層の入力](full-replay-input.json)・[native の結果](full-replay.json)・[生の readout 入力、i16 little-endian、zstd 圧縮](full-readout.i16le.zst)

リポジトリのルートから次のコマンドで全層の生成を再現できる。Python は numpy、requests、huggingface-hub を使用する。SHARDS は保存済み上流 shard のディレクトリで、不足する shard は固定 revision から取得する。OUT は生成先の空ディレクトリを指定する。

```sh
set -o pipefail
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
cargo +1.93.0 build --locked --offline --release -p misaka-palw-tir-lower --example replay-qf1-8b
python "$p/check_integer_weights.py" --local "$SHARDS" --stream |
  "${CARGO_TARGET_DIR:-target}/release/examples/replay-qf1-8b" \
    "$p/integer-weights.json" "$p/full-replay-input.json" 3 "$OUT/readout.bin" > "$OUT/result.json"
```

独立した Python 実装では入力と既生成の2トークンの計113位置を層順に処理した。上流の値と整数台帳を照合し、全36層を経た3予測位置の全111ベクトルのバイト列、各位置の全151,936 logits のハッシュ、最大値の ID と停止条件が Rust と一致した。参照実装から得た readout Z と層ごとのハッシュは[独立再計算結果](full-replay-check.json)に記録した。再検査には固定 revision の safetensors index、最終 norm の BF16 バイト列、出力 head の元 shard を指定する。

```sh
python "$p/check_full_decoder.py" "$OUT/result.json" "$p/decoder-run.txt" "$OUT/readout.bin" \
  "$SHARDS/source-model.safetensors.index.json" "$SHARDS/qwen-source-gains/model.norm.weight.bf16" \
  "$SHARDS/model-00005-of-00005.safetensors"
```

## 通常の PALWTIR1 読み込みと登録検査

全層の単独実行を記録したコミットは `c6465c9a4`。そのグラフを登録検査に通すと、位置入力は TIR の上限 262,143 まで取り得るため、8,192 行の RoPE 表の Gather が拒否された。登録用グラフでは位置を 0〜8,191 に絞ってから表を引く。QF1 の受理可能な入力位置は同じである。次の登録試験では Court が一度に扱う値が4,160個に達し、上限4,096個を超えた。Q 投影、因果 attention 出力、attention 後の残差、MLP の乗算・narrowing 後を追加で commit し、各部分の裁定が既存の上限に収まるようにした。回答の読出し位置は各層の第二残差と最終 norm の37本である。

SDK の隔離用 reader は graph、layout、tokenizer、変換 manifest、実際のパラメータから計算する inventory root を照合する。重み・gain・整数スケール・RoPE 表は909個のテンソルとして通常の PALWTIR1 に格納される。各 shard の元データと整数変換を照合し、全パラメータがそろって通常の node reader を通過した後にファイルを公開する。格納されたソース由来の399テンソル、全1,721,967 inventory leaves、class ID を独立した Python 実装でも検査する。

次のコマンドはリポジトリのルートで実行する。OUT は新しい生成先、SHARDS は固定 revision の元 shard を保存したディレクトリである。変換器は不足する shard を固定 revision から取得できる。

```sh
set -o pipefail
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
cargo +1.93.0 build --locked --offline -p misaka-palw-sdk --example qf1-8b-artifact
python "$p/check_integer_weights.py" --local "$SHARDS" --stream |
  "${CARGO_TARGET_DIR:-target}/debug/examples/qf1-8b-artifact" pack \
    "$p/integer-weights.json" "$OUT/qwen3-8b.palwtir"
python "$p/check_class.py" "$OUT/qwen3-8b.palwtir"
```

隔離用 class は `64b927d057d7d088c605e739823a4ba38c631c764737a3fd82d7558fd668910381d0d674a9640396b13d5698ca03428487e4b19f8ae0268ef4b231b3979b9260`、PALWTIR1 ファイルの SHA-256 は `bdeb1aea5ba21c6b120874d1d2ab8b55f1c6acb24293829fca41df8bd4e96de4`。保存済みの111入力 token をノードで再生し、113位置・回答 `[8820, 67942, 21856]`・Limit・readout Z が独立参照に一致した。[全 inventory と class ID](class-check.json)・[ノードの再実行](registered-archived-replay.txt)

二つの隔離チェーンで同じ実モデルを検査した。一つでは署名付きの追加モデル登録を3ピアが受理し、同じ登録ブロックの状態に市場が生成された。隔離チェーンの最初の heartbeat の事前検査にも現行 t12 の range evaluator を適用した。最初に DAA 1 から適用した実行では事前検査が旧規則を選び、terminal close の計算量が上限を超えた。[登録・市場・再編成の3ピア記録](real-market.txt)

もう一つでは3ピアが同じ署名付き EXEC の 111 入力から全36層を実行した。回答は `[74, 130998, 488]`、停止は Limit。Original は 1,967,418 個の TRACE leaf を束縛し、3ピアは Init、位置64、End の opening を1件ずつ開いた。後続の独立した PANEL で抽選された2者が全再実行して署名し、Stop に関する反証は公開 opening から `ChallengeNotEstablished` と裁定された。Python 検査は3者の一致、公開された値と root、抽選、Stop witness を照合する。[3ピアの生ログ](real-three-peer.txt)・[独立照合](real-lifecycle.json)

仕事は RFC-0066 の private local inference で、事前固定した隔離用 D=1 を使う。報酬原資、新規採掘適格性、Final、Gate C、受賞ブロックをこの実行で認定していない。MODEL_AUDIT・G-COLL・G-COST・G-USE は別途必要である。

```sh
export MISAKA_PRIVATE_QF1_FULL="$OUT/qwen3-8b.palwtir" MISAKA_PRIVATE_QF1_TOKENIZER="$OUT/tokenizer.json"
export RUST_MIN_STACK=67108864 RAYON_NUM_THREADS=3
# 第1引数は同じビルドの kaspa_consensus テスト実行ファイルのパス
bin="$1"
"$bin" private_moe2_three_process_registration --nocapture
"$bin" private_qf1_8b_archived_readout --ignored --nocapture
{ echo "test_binary_sha256=$(shasum -a 256 "$bin" | cut -d' ' -f1)"; "$bin" private_tb1_stable_panel --ignored --nocapture; } > "$OUT/three-peers.txt" 2>&1
python3 -B "$p/check_real_lifecycle.py" "$OUT/three-peers.txt" "$p/class-check.json"
```

## 同じ選択鎖の登録、市場、EXEC 準備と freeze

3ピアは、署名付き class 登録を畳み込んだブロック `5b2dfee9b6cc2243…`（DAA 1、blue score 3）を受理した。各 DB の市場と class に対して、ノードの TIR record の graph・layout・tokenizer と保持する実重みを照合した。EXEC 準備ブロック `38e2fd9a968c0b38…` のハッシュを固定し、そのヘッダーが親についてコミットする classes テーブルから、SDK が登録済み class と inventory root を読み直した。登録ブロック自身のヘッダーはさらに一つ前の状態をコミットするため、この読み取りを正しく拒否した。

同じ3ピアが EXEC close `1ab19aa282d91bbc…` を受理した。登録後は通常の heartbeat で進め、frontier・成熟済み仕事・pruning の試験値を注入していない。公開期間 DAA 5〜33 の初日（DAA 5）に、native settlement は close を覆う認証済み frontier を確認できず、finalized head を返さなかった。全 holder で公開と署名前の journal 予約がともに `WouldBlock` になった。この一連の試験は実モデルの登録と EXEC の接続、finality 待ちでの停止を示す。[3プロセスの記録](registered-freeze.txt)・[登録、準備、close の独立照合](registered-freeze.json)

```sh
"$bin" private_registered_tb1_pending_freeze --ignored --nocapture
python3 -B "$p/check_real_lifecycle.py" --pending-freeze "$p/registered-freeze.txt" "$p/class-check.json"
```

### 別の隔離チェーンでの認証済み frontier

既存の t12 規則が認証済み frontier を進める条件を、BASE-0 の実行から別の native DB で確認した。検証者の成熟期間を DAA 5,040 まで通常の heartbeat で満たした。設定された class target のまま122回実行し、受理された floor claim `b5f3c6e16a26…` の Panel 署名者と Gate C 承認者がそれぞれ、受理ヘッダーとローカルに保持する実行資料から計算を再実行した。受理は DAA 5,040、Final は 5,182、Gate C の判定は 10,222。75抽選位置に対し必要な承認は41で、検査した鍵の署名は75位置を満たした。複数位置が同じ鍵を抽選し得る。認証後の frontier は実際の受付ブロックに一致した。[条件・再実行・判定のログ](certified-floor-history.txt)

これは1プロセスの旧規則の履歴試験である。native settlement の mature useful work 集計は BASE-0 を明示的に除外している。実モデルの TB1 freeze には、その close を覆う認証済み frontier に加え、元の期限内に native settlement が計上できる非 floor の成熟仕事を同じ履歴で確立する必要がある。[native settlement の計上規則](../../../../consensus/core/src/palw_native_settlement_v1.rs)

再実行には同じ consensus テスト実行ファイルを使う。

```sh
"$bin" private_tb1_certified_floor_history --ignored --nocapture
```

### MODEL_AUDIT の候補固定と原期限

同じ隔離チェーンの登録済み Qwen3-8B について、モデル評価用の `MODEL_AUDIT` と後続の評価用 `PANEL` に別々の dealer 多項式を生成した。3プロセスは各自の DB から準備ブロックを読み、ヘッダーの親状態で class と実重みの inventory を照合した。close は候補集合、descriptor と世代、suite、試験件数、容量予約参照、case の原期限、両 round の資格記録を固定する。保持者は元の EXEC 資格や別 round の資格を流用できず、close 後に suite、世代、descriptor、試験件数、予約、期限を変えた読み取りも拒否された。

この試験の suite・予約は隔離入力である。候補は1件、試験枠は2件。監査の close は DAA 6、公開期間は DAA 9 以上21未満。開始時に3者とも署名予約と公開を保留した。通常の heartbeat で DAA 33 まで進め、48個すべての追加ブロックで finalized head がないことを確認した。close 以降に受理される通常の attempt が frontier を進めるには、Final の後に Gate C の異議申立期間 5,040 DAA が必要である。DAA 6 にすでに Final だったと仮定した下限でも 5,046 となり、この監査の期限を過ぎる。独立照合では、3者の記録から候補集合と close のハッシュを再計算した。[監査のブロックと holder の記録](registered-model-audit.txt)・[候補と期限の独立照合](registered-model-audit.json)

この履歴では監査 seed の公開、suite の採点、評価用 PANEL の抽選には達していない。QF1 の G-USE・G-COLL・G-COST は UNKNOWN のままである。再実行には前節と同じ実モデル環境変数を使う。

```sh
{ echo "test_binary_sha256=$(shasum -a 256 "$bin" | cut -d' ' -f1)"; "$bin" private_registered_tb1_pending_freeze --ignored --nocapture; } > "$OUT/model-audit.txt" 2>&1
python3 -B "$p/check_real_lifecycle.py" --model-audit "$OUT/model-audit.txt" "$p/class-check.json"
```

### 登録済みモデルと同じ鎖で frontier が close を覆った試験

別の隔離 round では最初から監査期限を DAA 15,121 に固定した。Qwen3-8B を署名付きで登録し、監査準備ブロックの親状態から登録済み class を読み、候補 close を受理した。同じノードで通常の heartbeat を積み、検証者成熟後に元の target で BASE-0 を374回実行した。受け付けた floor claim は DAA 5,182 に Final、10,222 に Gate C 認証に達した。検証した承認は抽選75位置分、必要数は41。認証済み frontier が受付ブロックと一致し、登録 → 監査準備 → 監査 close → floor 受付 → frontier → 選択先端の祖先関係を native DB で検査した。[登録・認証・native readiness の全記録](registered-certified-frontier.txt)

このケースは1プロセスで、試験用の成熟済み仕事、frontier、pruning point は注入していない。DAA 10,222 の最初の未決済ブロック（blue score 1）には、必要な anchor 1件に対して0件、必要な仕事量1に対して0という二つの待ち理由が記録された。成熟待ちの fact も0で、BASE-0 は集計から1件除外された。safe と finalized はともに空で、監査 share の公開は保留となった。close を決済するには、計上対象の仕事の受付がその effect の DAA・blue score 以降にあり、深度に数える anchor も同じ条件を満たす必要がある。[仕事と anchor を数える条件](../../../../consensus/core/src/palw_native_settlement_v1.rs)

```sh
"$bin" private_registered_audit_certified_frontier --ignored --nocapture
```
