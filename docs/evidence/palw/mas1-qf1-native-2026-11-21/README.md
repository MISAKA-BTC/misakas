# MAS1・QF1 改訂の統合と隔離試験

2026-11-21 に受領した `misaka_docs_MOE2_MAS1_QF1_20261120.zip` の SHA-256 は `95f460df28fd5a8f7aad2ce50f12f0c20d7cbb15a3de3958523f1de58ac41303`。展開した3,094ファイルがすべてリポジトリの同じパスとバイト一致した。統合前の台帳にあった Studio 実行記録は対応する[隔離試験の記録](../studio-winning-receipt-2026-11-20/README.md)に保存した。

配布原本の改訂検査は入力3,012件、変更51件、履歴保存51件を照合した。G-COST改訂前、対話が任意であることと隔離試験の到達点を記述した時点で、ZIP 内の3,094ファイルを再照合した。差分は RFC-0054・0062・0066、統合版 RFC-v2-0017・0018 とその索引、裁定方針、QF1 移行文書の8件だった。さらに同梱の変更前ファイルを SHA-256 で検査して MAS1 版を復元し、その改訂検査も実施した。QF1 の Python 参照検査は57項目成功。過去の原本と照合する RFC・ADR の全体検査には配布版と同じ7件・157件の指摘が残る。文書全体の strict 検査にも配布原本から引き継いだ指摘が残っている。

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
# DOCS_BUNDLE_ROOT は受領 ZIP を展開したディレクトリ
python3 -B "${DOCS_BUNDLE_ROOT:?}/docs/maintenance/revisions/2026-11-20-qwen-fow1/check_revision.py"
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

もう一つでは3ピアが同じ署名付き EXEC の 111 入力から全36層を実行した。回答は `[14847, 108722, 60508]`、停止は Limit。Original は 1,967,418 個の TRACE leaf を束縛し、3ピアは Init、位置64、End の opening を1件ずつ開いた。後続の独立した PANEL で抽選された2者が全再実行して署名し、Stop に関する反証は公開 opening から `ChallengeNotEstablished` と裁定された。Python 検査は3者の一致、公開された値と root、抽選、Stop witness を照合する。[3ピアの生ログ](real-three-peer.txt)・[独立照合](real-lifecycle.json)

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

別の隔離 round では最初から監査期限を DAA 15,121 に固定した。Qwen3-8B を署名付きで登録し、監査準備ブロックの親状態から登録済み class を読み、候補 close を受理した。生成側は通常の heartbeat を積み、検証者成熟後に元の target で BASE-0 を134回実行した。受け付けた floor claim は DAA 5,182 に Final、10,222 に Gate C 認証に達した。検証した承認は抽選75位置分、必要数は41。認証済み frontier が受付ブロックと一致し、登録 → 監査準備 → 監査 close → floor 受付 → frontier → 選択先端の祖先関係を native DB で検査した。[登録・認証・native readiness の全記録](registered-certified-frontier.txt)

生成側と、genesis から raw block を再読した2プロセスは、それぞれの DB で同じ登録・監査 close・認証済み frontier・選択先端・状態 root を得た。受信側は監査と評価 PANEL の元の Ready 署名を各9件検査し、いずれかの round の先頭署名を1 byte 変えると資格を成立させられなかった。3者の native readiness の全項目は一致した。[3プロセスの独立照合](registered-certified-frontier.json)

今回の実行ファイルの SHA-256 は `53a8f6a955a051b82a417612a2d82383ea4bafe9433141f01879ab4fd19bee77`。同じビルドで短期の監査、固定入力の全層再実行、成熟仕事を試験入力にした別の3ピアでの EXEC・Panel・Stop Court、市場登録と再編成も再検査した。各試験にはそれぞれの台帳を用いた。

この試験では成熟済み仕事、frontier、pruning point を注入していない。DAA 10,222 の最初の未決済ブロック（blue score 1）には、必要な anchor 1件に対して0件、必要な仕事量1に対して0という二つの待ち理由が記録された。成熟待ちの fact も0で、BASE-0 は集計から1件除外された。safe と finalized はともに空で、監査 share の公開は保留となった。close を決済するには、計上対象の仕事の受付がその effect の DAA・blue score 以降にあり、深度に数える anchor も同じ条件を満たす必要がある。[仕事と anchor を数える条件](../../../../consensus/core/src/palw_native_settlement_v1.rs)

```sh
"$bin" private_registered_audit_certified_frontier --ignored --nocapture
```

再検査用の `misaka-qf1-certified-audit-20261121.jsonl.zst` は圧縮後 SHA-256 `000d19ed684297be78eb7c26eafe2df481831cea77d506eacb8ae4287e08da8d`、展開後 `f2287895bb054178bb0846f754f5f84f7688198814f8896f8e7ed7bcfe6268d0`。先頭行に genesis・登録・floor claim・先端・ブロック数と、監査用・評価 PANEL 用の dealer transcript root と元の Ready 署名各9件を収録した。続く20,448行は順番どおりの RPC raw block である。実モデルと隔離 genesis の初期資金は reader がローカルで用意する。

```sh
export CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=target/protected-core-tests RUST_MIN_STACK=67108864 RAYON_NUM_THREADS=3
cargo +1.93.0 --config 'profile.test.package.misaka-palw-tir-exec.opt-level=3' test \
  --locked --offline -p kaspa-consensus --features evm --lib private_registered_audit_certified_frontier --no-run
# bin に上で表示された executable、archive に保存済みの zst ファイルを指定する
zstd -dc "$archive" > "$OUT/audit.jsonl"
MISAKA_PRIVATE_CERTIFIED_AUDIT_READ="$OUT/audit.jsonl" \
  "$bin" private_registered_audit_certified_frontier --ignored --nocapture
# 保存済みの3プロセス実行ログを照合する
python3 -B "$p/check_real_lifecycle.py" --certified-history "$p/registered-certified-frontier.txt" "$p/class-check.json"
```

### 登録済み Qwen3 の EXEC close に固定する MOE2 見積条件

隔離版の EXEC 登録者は、モデルと inventory、資金提供 bond、会計 policy、quote anchor と有限期限、報酬・数量・価格の上限、worker 配分を含む文脈に ML-DSA-65 で署名する。資金提供者は、この登録キーと割当 slot に対して専用 domain で ML-DSA-87 署名を付ける。3ピアは準備ヘッダーの親状態から bond の公開鍵と class を検査する。EXEC close のブロック本文は、slot 順の登録キーと資金提供者の同意参照を含む集合を固定する。将来の Original、当選 ticket、claim ID を事前に仮定する必要はない。

試験は登録ブロックの市場を quote anchor に用いる。準備ヘッダーの状態 root と照合した各ピアの台帳から、上限160での見積値を再計算する。規則の配分は買付48、永久LP 32、残り80。今回の worker プランでは miner 64、Panel 16 になる。選択鎖の close を読み戻しても資金提供 bond の予算行と市場準備金は元の値を維持した。数量と配分はいずれも見積値で、この Qwen3 の採掘資格は引き続き REVIEW_HOLD である。[3プロセスのログ](registered-exec-quote.txt)・[集合 root と整数計算の独立照合](registered-exec-quote.json)

この実行のバイナリは SHA-256 `c9aeddbc4bb49669191c2d4d8ac7d3e07b97ce44458a12e2339774abdeec3c03`。見積の原期限は DAA 6（この値を含む）、観測は DAA 5。期限・policy・数量・slippage・配分プラン・model・anchor の差替え、slot 移動、署名欠落、別の実在 bond の署名は拒否された。元の見積を再計算すると2 Position 購入、1 Position 永久LP、1 Position burn。最新 tip の台帳を quote anchor の台帳として渡した場合と上限超過も拒否された。同じ実行ファイルで登録・予約決済後の短い再編成、holder の finality 撤回を検証した。[別台帳での回帰試験](exec-quote-regressions.txt)

受賞を原資とする決済には、将来の認証済み claim、元の EXEC 割当、実予算予約を同じ採掘履歴上で対応付ける必要がある。別の隔離台帳の予約決済・再編成試験は従来どおり試験原資を使う。

上記と同じ実モデル・tokenizer の環境変数を設定して検査する。

```sh
{ echo "test_binary_sha256=$(shasum -a 256 "$bin" | cut -d' ' -f1)"; "$bin" private_registered_tb1_pending_freeze --ignored --nocapture; } > "$OUT/registered-exec-quote.txt" 2>&1
python3 -B "$p/check_real_lifecycle.py" --exec-quote "$OUT/registered-exec-quote.txt" "$p/class-check.json"
```

### Studio への署名付き資本割当と新規 claim の受付

上記の長期 MODEL_AUDIT と同じ close から隔離分岐を作った。実 Studio TIR の登録時に旧 class の share を 1‰ にすると、native preflight は `NotEndToEndCertified` を返した。0‰ の登録は DAA 3 に受理された。この share は旧プロファイルの採掘枠であり、モデルの tier 係数とは別の値である。DAA 4 にカード0が署名した資本割当を通常の carrier で受理し、DAA 5,040 で元担保の半分 46,953,160,500,520 sompi が有効になった。7日平均資本は33,537,971,786,085 sompi、予算は720ブロック分だった。

同じ時点の Studio の登録状態は `Active`、旧 registry の新規 claim 判定は `Candidate`。QF1 も `Candidate`、genesis の非 BASE 2クラスは `Prefetching` だった。各クラスについて8枚の producer カードすべてで通常の readiness が拒否を返した。全 claim 行は0件、safe と finalized は空で、holder の公開判定は `WouldBlock`。これは DAA 5,040 のこの分岐の観測である。元の20,448ブロックの履歴の BASE-0 claim と、期限 DAA 8,330 の別の Studio TFO3 claim はそれぞれ元の履歴に属する。[3ピアの状態・枝のハッシュ](legacy-work.json)

同じバイナリ（SHA-256 `907cdaf06a13dd22b8465fd11a5b50721b45315609b134f2ce2b57ca801913e5`）で生成側1プロセスと raw block を独立に受理する2プロセスを実行した。旧監査の原署名を再検査した後、追加10,076ブロックをたどり、登録・割当・成熟の各状態と全32個の producer 判定を比較した。手元保存の `misaka-audit-studio-capital-20261121.json.zst` は4,570,219バイト、SHA-256 は `65a1c81246ee5066cd183161402af6b020dbf7c902d6638c1bf43d4f38289d3d`。展開後は20,459,485バイト、SHA-256 は `de6e24c40988136e26223d2ddd9183339fb9a279f3b0c653d80a9d4e6d48e3aa`。同じビルドで、元の20,448ブロックをもう一度読み、当初の認証済み frontier・Ready 署名18件・全 readiness 項目と一致した。別台帳の MOE2 市場登録・署名済み EXEC 見積もりも再検査し、実引落しは0だった。以下は前節のビルドとモデル環境変数を使用する。

```sh
zstd -dc "$capital_archive" > "$OUT/capital.json"
export MISAKA_PRIVATE_CERTIFIED_AUDIT_READ="$OUT/audit.jsonl"
MISAKA_PRIVATE_AUDIT_WORK_READ="$OUT/capital.json" "$bin" private_registered_audit_legacy_work --ignored --nocapture
# 同じ close から別途生成する場合は未使用の出力パスを指定する
MISAKA_PRIVATE_AUDIT_WORK_WRITE="$OUT/new-capital.json" "$bin" private_registered_audit_legacy_work --ignored --nocapture
python3 -B "$p/check_legacy_work.py" "$OUT/capital.json" "$p"/legacy-work-{a,b,c}.txt
```

### 実 QF1 の回答と残差値の観測

登録済み tokenizer で、署名付き Original の入力・回答の root、Panel、Court を照合した3ピアの実行と、別 challenge の全層再実行を復号した。両方の質問は `2 + 2?`、回答上限は3トークンである。署名付き Original の回答は `fas联合会_COORD`、別実行の回答は先頭空白に続く `whatever:inline profits` だった。どちらも上限で停止し、期待する答えの4を返さなかった。[入力ごとの出力 ID と復号結果](qf1-answer-check.json)

別実行の保存済み readout で、各層につき3予測位置 × 4,096成分を数えた。整数コードが上端または下端（±32,767）にある成分は、第1層の第二残差で70/12,288、第36層の第二残差で7,152/12,288、最終 norm で0/12,288だった。端値数だけから回答誤りの原因は確定できない。2件は同じ質問の観測で、正式な PracticalSuite、独立の MODEL_AUDIT 抽選、G-USE の資格判定は未実施である。

```sh
uv run --no-project --with tokenizers==0.23.3 python -B "$p/check_qf1_answers.py" "$MISAKA_PRIVATE_QF1_TOKENIZER"
```

#### 残差スケールを変えた比較計算

同じ整数化済み399テンソルを持つ登録 artifact の identity を検査し、各入力の先頭回答位置で全151,936 logits を計算した。基準の Q15 では署名付き Original の先頭 token と一致し、別入力では保存済み native logits の SHA-256 とも一致した。次にローカルの診断用に、残差の小数部を15から10 bit に変更し、embedding、O・down 投影と RMS epsilon を同じ単位へ変換した。実モデルの登録、署名した EXEC、採掘結果のいずれにもこの変更を適用していない。[予測 token・4の順位・層別の端値数](residual-scale-check.json)

| 入力 | 基準 Q15 の先頭 token | 診断用 Q10 の先頭 token | 数字4の順位（Q15 → Q10） | 第36層末端の端値数（全4,096成分） |
|---|---|---|---|---|
| 署名付き Original | `fas` | `#%%` | 10,977 → 17,657 | 2,406 → 578 |
| 別の保存済み入力 | 空白＋`whatever` | `++`＋改行 | 80,201 → 524 | 2,465 → 634 |

同じ署名付き入力の比較用に、同じ変換済み重みと行ごとのスケールを読み、活性値・attention・MLPを FP32 で評価した。先頭 token は数字2、数字4は2位だった。この計算は変換済み重みを使った診断であり、上流 BF16 の成績には数えない。残差を一律 Q10 にすると端値は減ったが、両入力の先頭 token は回答の数字に至らなかった。ここで比較したのは先頭回答位置だけで、G-USE の採点には全回答・独立の評価入力・採択済み suite が必要になる。

```sh
for mode in '' '--archived' '--weight-reference'; do
  uv run --no-project --with numpy --with requests --with tokenizers==0.23.3 \
    python -B "$p/check_residual_scale.py" ${mode:+"$mode"} "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
done
```

#### 同じ重みで回答の最後まで比較した結果

登録済み整数版の出力と、同じ変換済み重みを FP32 演算で評価した出力を、保存済みの2入力で比較した。どちらも質問は `2 + 2?` で、TB1 から導いた token 列は異なる。元の上限3 token では、FP32 は両入力とも `2 + ` で停止した。整数版はそれぞれ `fas联合会_COORD`、` whatever:inline profits` だった。

同じ入力から FP32 の診断用上限を12 token にすると、双方とも `2 + 2 = 4.` を生成し、9 token 目で EOS に達した。数字4は7 token 目に出た。元の Original の上限は3である。この比較から、元の上限では参照計算も式を書き終えられず、整数版の出力は先頭位置から参照計算と異なることが分かる。同じ質問の2入力についての観測であり、実用性 suite の成績は未測定である。[入力ハッシュ、整数版・参照出力、各位置の順位](qf1-decoded-comparison.json)

再現には上記の Python 環境と登録済み artifact を用いる。次の2コマンドを `--archived` 付きでも実行すると、もう一方の入力を検証できる。

```sh
python -B "$p/check_residual_scale.py" --decode-reference "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
python -B "$p/check_residual_scale.py" --decode-reference --reference-limit 12 "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
```

もう一つの診断では、署名済み入力の111位置すべてで attention 後と MLP 後の FP32 残差の絶対値を測り、各層で最大値を i16 に収める小数 bit 数を固定した。embedding は Q15 から始め、小数 bit 数は前層以下にした。層をまたぐ残差、O・down 投影、RMS の epsilon をその単位へ合わせてから、もう一方の保存済み入力で整数演算を評価した。較正入力では第6層までの最大値は83未満だったが、第7層で22,076を超え、以後の小数 bit 数は0になった。

評価入力の先頭予測は `The` だった。数字4の順位は元の整数版の80,201位から94位に変わり、第36層末端・最終入力位置の端値は2,465/4,096から0/4,096に減った。基準 Q15 では、KV 履歴を継いで計算した3つの予測位置の全 logits が、各位置で保存済み native 実行の SHA-256 と一致した。層別スケールで同じ評価入力から12 token まで生成したところ、冒頭は `The system is` となり、数字4も EOS も出なかった。両入力は異なる TB1 文脈で同一の質問を含む。この方式を登録モデルへ適用していない。[各層の観測値、固定スケール、評価入力の予測](qf1-layer-scale-comparison.json)

```sh
python -B "$p/check_residual_scale.py" --cross-input-scale --reference-limit 12 "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
```

### EXEC の締切ブロックで確認した MOE2 の予算枠

3つの native DB で同じ EXEC close を再読し、ヘッダーが認証する選択親状態から、署名済み slot 9 の報酬上限を賄うための担保を試算した。資金提供 bond の担保は93,906,321,001,040 sompi、予算の export 上限は担保の250‰。上限報酬16,000,000,000 sompi の全額を払い出すには64,000,000,000 sompi が必要で、1 sompi 減らすと足りない。epoch 0 のモデル配分は bootstrap 中だった。判定は、凍結集合の見積もり付き slot を順番に予算台帳の複製へ予約し、元の台帳に対する引き落としは0だった。今回の集合には見積もり付き slot が1件あった。[締切ブロックと3ピアの観測](registered-exec-budget.txt)・[署名参照と担保計算の独立照合](registered-exec-budget.json)

見積もりの期限は DAA 6 で、同じ round の PANEL 公開開始は DAA 27。元の条件のまま、この見積もりで PANEL 後の決済を行うことはできない。今回の観測は資金面だけの試算で、採掘資格・finality・当選・支払は成立していない。

決済時には、予約キーと対象 bond・model に加え、署名した policy が実行位置の予算台帳と一致することを確かめる。隔離試験では他の予約情報を保ったまま policy が異なる場合に、帳簿、予算行、journal が引き落とし前の値を保った。正規の試験予約は同じ実行ファイルの3ピアで決済・再編成・再生が通った。コアの関連9試験には無期限の MOE2 条件の拒否も含む。[決済と巻き戻しの観測・コア試験](budget-policy-regressions.txt)

再現には前節の `private_registered_tb1_pending_freeze` と `private_moe2_three_process_registration` を用いる。独立照合は次のコマンドで行う。

```sh
python3 -B "$p/check_real_lifecycle.py" --exec-budget "$p/registered-exec-budget.txt" "$p/class-check.json"
```

### PANEL 公開開始後と資本成熟後の MOE2 見積もり

新しい隔離 EXEC の締切前に、同じ Qwen3 class に対して二つの仕事を登録した。slot 9 は DAA 6 まで、slot 10 は DAA 15,171 までの見積もりを持つ。slot 10 の owner は別の nonce・仕事期限・経済条件に署名し、資金提供者はその登録キーと slot に署名した。slot 7 に見積もりはない。3プロセスは同じ締切ブロックと入力集合を検証した。DAA 3 には、資金提供者の署名と通常の手数料を伴うモデル資本割当 128,000,000,000 sompi を受理した。割当額は2件の報酬上限に必要な担保の合計である。

PANEL 公開開始は DAA 27。3つの独立した native DB で DAA 33 の先端ヘッダーと、そのヘッダーがコミットする親状態を照合した。slot 9 は期限切れを返し、slot 10 は元の署名条件で 160 BILI の配分を再計算できた。購入 48 BILI、永久 LP 32 BILI、購入した2 Position のうち1を LP に使い1を burn、Miner 64 BILI、Panel 16 BILI という見積もりである。入れ替えた親状態、先端の事後状態、署名上限を超える報酬は拒否された。

その先に生成した heartbeat 2,816 ブロックを2つの受信側 DB も順に検証し、資本割当が有効になる epoch 2 に入った。続くブロックのヘッダーが DAA 1,441 の状態を認証した。単位は sompi。

| 項目 | 3ピアの観測 |
|---|---:|
| 資金提供 bond の有効なモデル割当 | 128,000,000,000 |
| 最初の7期間平均、端数切り捨て | 18,285,714,285 |
| 当 epoch の累計 worker 原資 | 408,997,052,880 |
| モデルがこの位置で使える報酬枠 | 408,997,052,880 |
| 署名済み slot 10 を予算台帳の複製に予約した額 | 16,000,000,000 |
| 元の台帳でのモデル報酬予約・引落し | 0 |

この試験には T12 の[既存の tier 設定](../../../../consensus/core/src/config/params/full_activation_v1.rs)を使った。追加モデル係数は 300‰、追加 tier 全体の上限は 1,000‰。この履歴では当該モデルの利用枠がその epoch の累計額と一致した。署名したモデル資本割当には実際の状態遷移があり、受賞に伴う支出は0件だった。新規 claim の採掘資格、finality、当選、受賞 UTXO は引き続き未成立。[実行ログ](registered-live-offers.txt)・[3ピアと期限・資金の独立照合](registered-live-offers.json)

再現には上記の実モデル・tokenizer と同じテスト実行ファイルを使う。

```sh
MISAKA_PRIVATE_MOE2_SEASON=1 "$bin" private_registered_tb1_pending_freeze --ignored --nocapture
python3 -B "$p/check_real_lifecycle.py" --live-offers "$p/registered-live-offers.txt" "$p/class-check.json"
```

### QF1 の演算箇所別に固定した scale と出力

署名済み入力の111位置を較正に使い、各層の Q・K・V・残差・MLP の up 投影と積について、絶対値の99.99パーセンタイルから i16 の固定小数点位置を選んだ。別の記録済み TB1 入力で全36層を再実行した。二つの入力には同じ質問 `2 + 2?` が含まれる。先に登録済み方式で native の3予測位置・各151,936 logits の SHA-256 を再現した。較正した方式は12 token とも `**` を返し、数字4の先頭位置の順位は語彙151,936件中153位だった。[固定した scale、入力ハッシュと出力](qf1-stage-scales.json)

続いて同じ変換済み重みを使った FP32 参照計算で、入力を固定し、演算箇所を一つずつ整数の出力範囲に丸めた。単独で適用した6条件ではいずれも先頭 token は `2`。範囲を組み合わせた計算では `**` になり、MLP の up と積だけを参照精度に戻すと `2` に戻った。比較で「attention」を選んだときは、参照回転の後に Q/K と V を整数化し、履歴全体を使う整数 GQA を実行した。一部の出力表現は隣の段と重なる。これは先頭位置の比較である。[各条件の token・順位・クリップ数](qf1-stage-ablation.json)

同じ較正入力で、MLP の up と積だけは観測最大値を収める固定 scale に変えた。整数版の別入力での出力は `2 + 2?` の後に EOS、合計6 token だった。入力の式を繰り返し、数字4を出していない。登録済み class、署名済み仕事、正式な G-USE の結果には変更を加えていない。[最大値を使った場合の全出力](qf1-mlp-maximum.json)

```sh
python -B "$p/check_residual_scale.py" --stage-scales --reference-limit 12 "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
python -B "$p/check_residual_scale.py" --isolate-narrowing "$p/qf1-stage-scales.json" "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
python -B "$p/check_residual_scale.py" --stage-scales --preserve-mlp-range --reference-limit 12 "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
```

### 64成分単位の残差スケールによる Qwen3 の回答

元の署名付き入力だけを較正に使い、Q/K/V、MLP の up と積、残差の最大絶対値を層ごとに測った。第7層の残差には約22,076の値がある。層に一つの固定指数を与えると、ここから全4,096成分が Q0 になった。別の challenge を持つ `2 + 2?` に対する整数出力は48 token の上限で止まり、末尾で `2?` を反復した。[各層の残差に一つの指数を使った結果](qf1-stage-maxima.json)

同じ較正入力・重み・projection・greedy 規則で、残差だけを連続64成分ごとの固定指数にした。各層の指数64個を較正後に固定し、skip 加算は対応する指数へ丸め、RMSNorm に入る前に整数を共通指数へ正確に揃えた。別の challenge への回答は `2 + 2 = 4.` の後に EOS、9 token だった。元の Q15 実装についても同じ評価入力の3位置すべてで全 logit の SHA-256 が保存済み native 実行と一致した。[候補の指数・回答・logit hash](qf1-residual-tiles.json)

この指数と変換済み重みを固定し、別の2問を同じ整数実装で評価した。質問と診断用 challenge は較正に使っていない。32 token を上限とし、両方とも EOS で終了した。

| 質問 | 回答 | EOS を含む token 数 |
|---|---|---:|
| 3 + 5? | 3 + 5 = 8. | 9 |
| What is the capital of France? | The capital of France is Paris. | 8 |

各入力、候補の較正ファイル SHA-256、全出力 token と各位置の logit hash を[転用試験の記録](qf1-tile-transfer.json)に収録した。この数値候補には新しい実行グラフ・scale の登録が必要である。結果はオフライン評価であり、採択済み PracticalSuite の G-USE 判定、元の署名済み Original の更新、採掘適格性を意味しない。

```sh
python -B "$p/check_residual_scale.py" --stage-scales --preserve-stage-ranges --reference-limit 48 "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
python -B "$p/check_residual_scale.py" --stage-scales --preserve-stage-ranges --residual-tiles --reference-limit 48 "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
python -B "$p/check_qf1_tile_transfer.py" "$MISAKA_PRIVATE_QF1_FULL" "$MISAKA_PRIVATE_QF1_TOKENIZER"
```

### Genesis 8K の保有証明と非 floor 仕事の受付条件

署名済み MODEL_AUDIT を含む既存の [DAA 5,040 の履歴](legacy-work.json)を延長した。手元の Qwen2.5-1.5B A16 / graph-v7 @8192 は 1,799,359,436 バイト。通常の class manifest 検査で genesis の class と inventory root を再導出した。生成側はこのファイルの実データから、隔離 genesis の8個の異なる bond に当たった葉を開き、各 bond の鍵で V2 readiness に署名した。対象は既存の genesis class で、Qwen3 の監査判定とは別である。

通常の手数料を支払う8取引を16ブロックに収録した。DAA 5,048 に8席すべてが ready となり、class は `Probation { probes_passed: 0 }` に移った。さらに4個の heartbeat を検証した DAA 5,050 でも8席の証明は新鮮で、各席の producer preflight は通った。panel room は5、モデル別の利用可能枠は0 microblocks、claim は0件。3プロセスはそれぞれの native DB で同じ選択先端・状態 root・各席の span と開示数を得た。8鍵は隔離試験用である。[3ピア照合](genesis-seat-boundary.json)・[20個の raw block](genesis-seat-blocks.json.zst)

```sh
zstd -dc "$p/genesis-seat-blocks.json.zst" > "$OUT/genesis-seats.json"
python3 -B "$p/check_genesis_seats.py" "$OUT/genesis-seats.json" "$p"/genesis-seat-{a,b,c}.txt
# 前節の署名済み監査履歴と legacy fork を読み込んだうえで実行
MISAKA_PRIVATE_TB1_GENESIS8K="$held_qwen25" MISAKA_PRIVATE_GENESIS_SEATS_READ="$OUT/genesis-seats.json" \
  "$bin" private_registered_audit_legacy_work --ignored --nocapture
```

同じ選択先端から、別の genesis bond が自分の担保 46,953,160,500,520 sompi を 8K class に割り当てた。元の Studio 向け割当と監査期限 DAA 15,121 はこの分岐に残っている。8席の最後の手数料取引の釣銭で通常の署名付き carrier を支払い、DAA 5,051 の native 親状態遷移で pending となった。続く最初の利用可能額を3プロセスが確認した位置は DAA 6,481（epoch 9）。利用枠は152,530 microblocks、旧証明の有効席数は0、class は Held、claim 数は0だった。1ブロックは1,000,000 microblocks である。[署名・成熟・3ピアの照合](genesis-capital.json)・[割当から成熟までの2,862ブロック](genesis-capital-blocks.json.zst)

```sh
zstd -dc "$p/genesis-capital-blocks.json.zst" > "$OUT/genesis-capital.json"
python3 -B "$p/check_genesis_capital.py" "$OUT/genesis-capital.json" "$p"/genesis-capital-{a,b,c}.txt
```

成熟位置から同じ class と8個の bond に対し、新しい span の開示と署名を生成した。追加20ブロックを独立に検証した3ピアでは、DAA 6,491 に全8席が再び ready、class は Probation、panel room は5、利用可能な仕事枠は1,677,832 microblocks となった。この位置の claim は0件。監査期限内に既存クラスの通常の実仕事を試せる位置である。readout の `target` は registry の値で、実行時の draw は producer facts から target を読む。[更新後の3ピア照合](genesis-renew.json)・[更新の raw block](genesis-renew-blocks.json.zst)

```sh
zstd -dc "$p/genesis-renew-blocks.json.zst" > "$OUT/genesis-renew.json"
python3 -B "$p/check_genesis_seats.py" --renewed "$OUT/genesis-renew.json" "$p"/genesis-renew-{a,b,c}.txt
export MISAKA_PRIVATE_GENESIS_SEATS_READ="$OUT/genesis-seats.json"
export MISAKA_PRIVATE_GENESIS_CAPITAL_READ="$OUT/genesis-capital.json"
export MISAKA_PRIVATE_GENESIS_RENEW_READ="$OUT/genesis-renew.json"
# 上記の MODEL_AUDIT と legacy fork の READ、Qwen2.5 の実ファイルと QF1 の実行環境を引き継ぐ
"$bin" private_registered_audit_legacy_work --ignored --nocapture
```

隔離枝の実仕事について、受付済み raw block と推論資料を読み戻す試験に、Panel と Gate C の署名を進めるモードを追加した。生成側は Panel の bind 用と Gate C の抽選用の両方で実行した BASE-0 ブロックを使う。各署名の直前に、選出された席・検証者が受け付けた元の header と元の資料に対して、登録モデルの backend で独立再実行する。検証者候補から除外する bond は元の claim から読む。手数料原資は8席の更新を支払った取引の釣銭である。

`MISAKA_PRIVATE_GENESIS_CERTIFY_WRITE` で受け付けた claim 以後の raw block を記録し、`MISAKA_PRIVATE_GENESIS_CERTIFY_READ` を指定した各 reader は同じ branch を検証して、Final、元の claim の Gate C 投票数・transcript、当初の監査 close に対する publication 可否を読み直す。両モードには前節の素材と `MISAKA_PRIVATE_GENESIS_ATTEMPT_READ` の受付記録が必要である。実行ファイル SHA-256 `2cbce58f…6be290` の [BASE-0 再実行と RFC-0041 の回帰試験](genesis-certification-regressions.txt)は双方成功。この2試験は Qwen2.5 の仕事の認証結果を含まない。

## 64 lane 群の補正を使った QF1 の native 実行と登録

36 層の残差を 64 群に分けて固定した係数を native 実行グラフに組み込んだ。旧 Q15 の全重みを保持し、追加した係数を別にコミットする。native と独立 Python 実装は算術と地理の2問で、生成した全17位置の全語彙 logits のハッシュ、各 token、EOS が一致した。[実行結果](qf1-native-tile-decodes.json)

同じ数値グラフのファイル形式ごとに class ID が異なる。親の重みを共有する形式は現行 T12 の Court サイズ計算で作業量の上限に達した。単一ファイル形式では全 8,835,229,568 bytes をストリームで独立に照合し、class ID は `b1da96d1…`。DAA 18,640 の条件で11種の終端の最大 close は 1,422,367 bytes、搬送上限は 3,200,000 bytes。[形式ごとの計測](qf1-native-tile-artifacts.json)

3つの独立したプロセス・DBで単一ファイル形式の class を導出し、署名付き登録、登録 burn、MOE2 市場の開設、MAS1 停止、reorg 後の担保復元を観測した。同じ class の別の登録試験では、未確定の EXEC close で公開と署名が3ピアとも WouldBlock となった。市場試験の予算は隔離用で、QF1 への採掘報酬は未検証。2問の結果は G-USE の判定に使用していない。[登録結果](qf1-native-tile-registration.json)・[登録生ログ](qf1-native-tile-registration.txt)・[EXEC close の照合](qf1-native-tile-registered-freeze.json)

以下の `parent` は先の節で作成した Q15 の全重み、`ex` は SDK の example バイナリ `qf1-8b-artifact`、`out` は書き込み可能な空のディレクトリを指す。

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
zstd -dc "$p/qf1-residual-tiles.section.zst" > "$out/scales.section"
"$ex" pack-tiles "$parent" "$out/generated.section"
cmp "$out/scales.section" "$out/generated.section"
"$ex" full-tiles "$parent" "$out/scales.section"
"$ex" export-tiles "$parent" "$out/scales.section" | python3 -B "$p/check_qf1_full_tiles.py" "$out/scales.section"
python3 -B "$p/check_real_lifecycle.py" "$p/qf1-native-tile-registered-freeze.txt" "$p/qf1-native-tile-artifacts.json" --pending-freeze
```

## 修正後の QF1 の署名済み Original と、旧規則の 8K の実抽選

単一ファイル class `b1da96d1…` の非報酬 EXEC で、生成上限16を登録時に署名し、隔離3プロセスで同じ入力を実行した。回答は `2 + 2 = 4.` と EOS の9 tokens、prefill を含む処理位置は119。独立な PANEL seed で選んだ2名が同じ Original `da14cf2d…` を完全再実行して署名した。3ピアとも最後の回答と Stop の局所証拠 1,622 bytes を検査し、異議不成立となった。この試験は private の成熟済み仕事 fixture で EXEC/PANEL の release を進める。報酬付きの Final・Gate C を測定した試験ではない。[3ピア生ログ](qf1-native-tile-three-peer.txt)・[独立照合](qf1-native-tile-lifecycle.json)

旧規則の Qwen2.5 A16 8K は上記と別の試行である。更新済みの担当席を置いた署名済み監査枝上で最初の実行を行い、claim `8748971c…` を native が受理した。生ヘッダから再計算した block は `3250abac…`（DAA 6,491）、class ticket は `cb1449dab7180d9863dae3a062f54dea`。このテンプレートの native target は `ffffffffffffffffffffffffffffffff`。下記の検査では、登録済み inventory、親の state、header challenge、execution anchor、claim ID、ticket を元 bytes に結び付ける。保管資料は 2,409,131 bytes。署名・算術の検査は native の担当である。[受理記録](genesis-draw-receipt.txt)・[生ヘッダを含むブロック](genesis-draw-block.json.zst)・[保管資料](genesis-draw-material.bin.zst)・[照合結果](genesis-draw.json)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
python3 -B "$p/check_real_lifecycle.py" "$p/qf1-native-tile-three-peer.txt" "$p/qf1-native-tile-artifacts.json" --decode-limit 16
zstd -dc "$p/genesis-draw-block.json.zst" > "$out/winning.json"
zstd -dc "$p/genesis-draw-material.bin.zst" > "$out/capture.bin"
python3 -B "$p/check_genesis_draw.py" "$out/winning.json" "$out/capture.bin" "$p/genesis-draw-receipt.txt"
```

## 提出時の Stop 資料を使った直接 Court

QF1 の trace を一回ストリーム処理する間に、指定された tile と Stop の両方の Merkle path を収集した。通常の署名済み Original と一緒に最後の回答・Stop の資料を渡し、各ピアが独立再実行した値と照合してディスクへ保存した。その後の Court は各ピアのローカルファイルから両方の opening を検証する。この順序では、Court のために Miner へ再照会したり、Miner がもう一度推論を実行したりする手順がない。

この実行では3ピアが同じバイナリで Original `87f8603b…` の資料 1,622 bytes を receipt **より前**に保存した。事後に抽選した2名が完全再実行して署名し、3ピアは全て正しい EOS に対する異議を退けた。照合スクリプトは保存時点、署名済み Original、回答・trace の2本の opening、局所裁定、ピアの集合を照合する。対象は非報酬 EXEC と Stop 関係。全演算の反証 coverage、任意資料の可用性、G-COST はこの実行の対象外。[生ログ](qf1-held-stop-three-peer.txt.zst)・[照合結果](qf1-held-stop-lifecycle.json)

```sh
zstd -dc "$p/qf1-held-stop-three-peer.txt.zst" > "$out/held-stop.txt"
python3 -B "$p/check_real_lifecycle.py" "$out/held-stop.txt" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --retained-stop
```

8K の[保管資料の native 照合](genesis-draw-material.json)では、署名済み envelope の ML-DSA-87 署名と manifest の class・inventory を検査した。保管資料の prompt は 1,023 tokens、decode は1 token（ID 220）。105,396,200 個の論理 step leaves を持つ sparse tree と 1,023 checkpoint leaves を含む資料から、commitment の execution・trace・output の各 root を照合した。末尾を1 byte切った資料は decode で、署名を1 bit変えた提出物は署名検査で拒否された。この検査で担当者による数値の完全再実行や Gate C 署名を代行することはできない。

```sh
cargo +1.93.0 run --locked --offline -p misaka-palw-base0 --example check-8k-capture -- "$out/winning.json" "$out/capture.bin"
```

旧 8K 枝の再生では、MODEL_AUDIT の preparation で認証済みの candidate・suite・予約を保持し、同じ close を後から参照する。以前の試験は DAA 5,040 で preparation の state を再取得し、履歴再構築上限 4,000 DAA に達した。修正後は同じ署名済み close、同じ tip/state に対して、その時点の枝と native publication gate を再検査できた。DAA 5,040 の判定は `WouldBlock`、原期限は 15,121 のまま。これは監査参照の検証結果である。[同じ枝での修正前後の記録](signed-audit-retained-preparation.json)

旧 8K の元ブロックを受け付けたプロセスで、Qwen2.5 の全入力から数値計算をやり直した。claim `8748971c…` の trace・output・execution と、元ヘッダに束縛した再実行 transcript `82bb8e54…` が一致した。DAA 6,491 の claim は `Provisional`。この検査は1プロセスで完走した。[再実行ログ抜粋](genesis-draw-reexecution.txt)・[元ブロックとの照合](genesis-draw-reexecution.json)

当初の受付だけを検証するバイナリでも、3プロセスで同じ元ブロックの完全再実行が完了した。3者は受付時の Provisional、state root、trace・output・execution、再実行 transcript で一致した。[受付時点の3ピア照合](genesis-intake-three-peer.json)

```sh
python3 -B "$p/check_genesis_draw.py" "$out/winning.json" "$out/capture.bin" "$p/genesis-draw-receipt.txt" --peer "$p/genesis-draw-reexecution.txt"
```

同じ受付済み claim について、各席が待機中に保有証明を更新しない場合も調べた。DAA 6,491 から 40 heartbeat 後、DAA 6,511 の抽選位置へ進み、採掘した BASE-0 アンカーを受理した。ノードは元の claim ID について「必要5席、除外・operator 重複排除後は2席」と記録し、Panel を割り当てなかった。試験はここで失敗し、この枝でこの claim の Gate C 署名や支払いは生じていない。[対照試験のログ抜粋](genesis-unrenewed-panel.txt)

同じ受付ブロック `3250abac…` から、保有証明を更新する別の隔離枝を検証した。両試験の元 claim・再実行 transcript・受付直後の state root は一致する。抽選 DAA 6,511 では既存の保有証明（有効期間 24 DAA）のうち 8席中6席が失効する。各席は登録済み Qwen2.5 の同じ artifact から新たな開示を取り、当該 span の challenge に署名し、通常の手数料を支払って提出した。抽選時の registry は ready 8、panel room 4。実行した BASE-0 アンカー `555089fe…` の受理後、元 claim は DAA 6,511 で `PanelBound` になった。この抜粋の終点は Panel の割当である。[保有証明の期限と PanelBound の記録](genesis-renewed-panel.txt)

上記で Panel が割り当てられた旧 8K claim は、担当席 3・4・7 がそれぞれ元の推論をやり直し、同じ transcript を確認した上で、有効票を署名した。quorum の受付後、元 claim の `Final` を DAA 6,633 の選択鎖で確認した。これは Gate C の認証前の観測である。[同じ claim の署名前の再実行と Final](genesis-panel-final.txt)

同じ試験で Final 後にも BASE-0 を実行し、異なる実行済み attempt を含む64ブロックを受理した。各回の8 heartbeat 後に元の 8K claim の認証行を確認したが、64回の上限でも抽選アンカーが空だった。試験は Gate C 承認の署名前で失敗した。[受理した64件と停止箇所](genesis-post-final-draw.txt)

試験終了前に保存した隔離 DB のコピーを読み取り専用で開き、元の受付ブロックから DAA 6,857 までの選択鎖789ブロックを取り出した。各ヘッダのハッシュを保存済みキーと照合し、ブロック本体を raw 形式に往復した。収録した Final 後の attempt ブロックのうち、後続ブロックも収録した56件では、すべて後続ヘッダの DAA が元と同じだった。保存した範囲は64回の試験の途中までである。[raw ブロック](genesis-exported-suffix.json.zst)・[範囲と SHA-256](genesis-exported-suffix.json)

新しい DB で署名済み監査枝と Qwen2.5 の元の仕事を再実行した1プロセスが、この789ブロックを通常どおり受理し、保存元と同じ tip・state root を得た。DAA 6,857 で元 claim は Final、Gate C の抽選0・承認0、認証期限は DAA 8,793。認証行に終局結果はまだなかった。[独立再生時の認証行と state root](genesis-recovered-peer.txt)

保存元の DB を同じ隔離ネットワークのパラメーターで開き、tip の state root を検証した。Final 後の56個の attempt ブロックごとに署名済みヘッダから claim ID を再計算し、対応ブロックの状態差分を読んだ。その差分に自分の claim の新規作成は0件、同じブロックを指す抽選 seed anchor の記録も0件だった。元の 8K claim の認証行は抽選前である。この範囲では追加の heartbeat や署名だけで Gate C を進めることはできない。[状態差分と認証行](genesis-anchor-journal.txt)

同じバイナリの独立した3プロセスが新規 DB で元の数値実行を行い、保存した789ブロックの受理を完了した。各ピアの transcript・Final・抽選0の認証行・tip・state root が一致した。別の照合器でも全789ヘッダを順番に再ハッシュして、直前の親と最後の tip を検査した。[3ピア照合](genesis-recovered-three-peer.json)・[ピア2](genesis-recovered-peer-b.txt)・[ピア3](genesis-recovered-peer-c.txt)

同じ枝をもう一度、候補の受付理由を記録して再生した。Final 後の56個の attempt ヘッダすべてで、ハッシュと producer bond の index をノードの記録と照合した。ブロックは選択鎖に入り、各 attempt の claim は BASE-0 の当該配分 epoch のモデル別ブロック機会 `P_m` 不足で拒否されていた。従って Gate C が使う seed anchor にもならなかった。[該当56ブロックとノードが返した拒否理由](genesis-model-budget-refusals.txt)

## 署名済み Original からの Stop 資料の受付

Stop 資料を保管する際は、owner 署名、対象の attempt、答えと trace の opening を検査する。局所関係に矛盾がある場合も認証済みの資料として受け取れる。開示が空、長さが不正、または opening が root に一致しない場合には拒否する。数値再実行の結果との一致を検査する前にファイルと親ディレクトリを同期する。[受付・裁定の実装](../../../../misaka/palw/remote/src/tb1_exposure/exec_input/stop_case.rs)・[関連6試験](qf1-stop-intake-test.txt)

Qwen3-8B の全重みを使った隔離3ピアの非報酬試験では、Original `c90c2128…` の1,622 bytesを3ピアが receipt 前に保持した。選出された2名が独立に完全再実行し、3ピアは保存済み資料から EOS への異議を退けた。答えは `2 + 2 = 4.` と EOS。対象はこの Original の Stop 関係である。[3ピアの生ログ](qf1-stop-intake-three-peer.txt.zst)・[独立照合](qf1-stop-intake-lifecycle.json)

```sh
zstd -dc "$p/qf1-stop-intake-three-peer.txt.zst" > "$out/stop-intake.txt"
python3 -B "$p/check_real_lifecycle.py" "$out/stop-intake.txt" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --retained-stop
```

## Readout 最終 chunk と End の直接反証

非報酬の QF1 試験では、元の trace 生成時に最終 chunk と End の開示を保持できる。Node は、両レコードが署名済み trace に含まれることを確認してから、最終 chunk の番号・消費バイト数・終端フラグ、採掘値 `z` と署名済み Readout root、End の各 root・件数を照合する。検証者は保管した資料から直接 Court に提出できる。この関係が検査する範囲は終端の結合で、各 chunk の圧縮・全層の算術は個別の検査対象である。

単体試験では、認証済みの誤った counter・終端フラグ・digest・End を反証として裁定し、開示の改変・欠落を裁定前に拒否した。[関連7試験の結果](qf1-readout-end-unit.txt)

Qwen3-8B の非報酬 Original `3c484ceb…` で、同じ実行ファイルの3ピアが receipt 前に End 資料を各3,069 bytes 保持した。選出された2名が固定入力から回答・四 root・Ticket を独立に再現した。回答は `2 + 2 = 4.` と EOS。3ピアは、同じ署名済み Original に対する Stop と End の異議をそれぞれの資料から直接裁定し、両方とも棄却した。独立した照合では、2,728,013 bytes の Readout の最終 chunk（77 bytes）・End の両方から trace root を再現し、末尾の digest と署名済み Readout root も照合した。この実行は正しい Original に対する異議を対象とする。[3ピアの生ログ](qf1-stop-end-three-peer.txt.zst)・[独立照合](qf1-stop-end-lifecycle.json)

```sh
zstd -dc "$p/qf1-stop-end-three-peer.txt.zst" > "$out/qf1-stop-end.log"
python3 -B "$p/check_real_lifecycle.py" "$out/qf1-stop-end.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --retained-stop --retained-end
```

### 署名済み Readout の矛盾を3ピアで直接裁定

隔離用の負例では、Qwen3-8B を実行して得た trace と最終 chunk と End を使い、試験用 owner がゼロの採掘値に対応する Readout root を含む Original に署名した。各ピアは署名・Ticket と開示の包含を受付時に検査し、同じ Original `5c929e66…` の資料を保存した。回答は `2 + 2 = 4.` と EOS。抽選された2名はいずれも完全再実行後に不一致を検出し、承認用メッセージの生成が拒否された。3ピアとも空の承認集合を拒否した。

3ピアが非対話で裁定した結果、正しい Stop への異議は棄却され、Readout と End の不一致は `ComputationRefuted` となった。照合器は固定した入力・回答、実行結果に対応する Readout root と署名された root の差、2本の trace opening、Panel の抽選順、個別の case と verdict のブロックを照合した。対象は非報酬の隔離経路で、実行費用の下界・決済を検査する試験ではない。[3ピアの生ログ](qf1-signed-readout-refuted.txt.zst)・[直接反証の照合](qf1-signed-readout-refuted.json)

```sh
zstd -dc "$p/qf1-signed-readout-refuted.txt.zst" > "$out/refuted.log"
python3 -B "$p/check_real_lifecycle.py" "$out/refuted.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --retained-stop --retained-end --inconsistent-readout
```

同じバイナリ・モデルで、正しい Readout root に署名する対照試験も実施した。各試験で EXEC・PANEL を新しく抽選した。対照試験の選出2名は完全再実行後に承認を署名し、3ピアすべてが承認の受付と End への異議棄却を確認した。[同一バイナリの対照試験](qf1-signed-readout-control.json)・[生ログ](qf1-signed-readout-control.txt.zst)

### 元の Final から BASE-0 の機会を確保して Gate C を抽選

上記の DAA 6,857 の枝から続けて、試験用 bond 5 が通常の資本割当を署名した。DAA 7,932 で BASE-0 の未使用枠が 1,021,082 microblocks に達した。登録済みの BASE-0 で実行した claim `614d0f9a…` が通常の価格でこの枠から受理され、ブロック `066986fa…` を抽選用アンカーとして状態に記録した。元の Qwen2.5 claim `8748971c…` は、原期限 DAA 8,793 より前の DAA 7,932 で Gate C の75枠を抽選した。75は重複を許す抽選枠数である。この記録の終点は抽選である。[割当と抽選](genesis-funded-draw.txt)・[ヘッダと元 claim の照合](genesis-funded-draw.json)・[追加の raw ブロック](genesis-funded-draw-blocks.json.zst)

同じバイナリの2つの別プロセスが、新しいDBで元の8Kジョブを読み込み、数値実行とその後の2,941ブロックを再生した。両ピアで元の Final DAA 6,633、抽選 DAA 7,932、75枠、抽選時点の承認0枠、選択鎖先端と状態 root が一致した。[再生結果](genesis-funded-draw-replicas.json)・[読取ピアA](genesis-funded-draw-reader-a.txt.zst)・[読取ピアB](genesis-funded-draw-reader-b.txt.zst)

```sh
zstd -dc "$p/genesis-exported-suffix.json.zst" > "$out/final.json"
zstd -dc "$p/genesis-funded-draw-blocks.json.zst" > "$out/draw.json"
for peer in a b; do zstd -dc "$p/genesis-funded-draw-reader-$peer.txt.zst" > "$out/reader-$peer.log"; done
python3 -B "$p/check_funded_draw_recovery.py" "$out/final.json" "$out/draw.json" "$out/reader-a.log" "$out/reader-b.log"
```

### QF1 の選出席が完全再実行の資料を取得できない場合

Qwen3-8B の非報酬 Original `6a0b74fd…` では、抽選後、各選出者が署名用ディレクトリから登録済み scale section を取得する。先頭の席にはこのファイルを配布せず、読み取りは `NotFound` となった。もう1席は親 artifact と section の組を確認し、完全再実行に一致して署名した。Original を準備したときのモデルはホストにある。この試験で欠落させたのは1席の署名用ストアの資料である。

3ピアとも1署名を検証し、同じ署名の二重計上と別の席の鍵への付け替えを拒否した。DAA 59 では残り1席の署名を待ち、元期限 DAA 60 に到達すると、新しい署名と承認集合の確定を拒否した。計算不正の裁定は発生していない。この試験は隔離 Panel の署名と期限を対象とする。資料供給契約の責任裁定、合意状態の未検証終了、Final・決済は別途検証を要する。[3ピアで照合した席と期限](qf1-missing-replay-material.json)・[生ログ](qf1-missing-replay-material.txt.zst)

```sh
MISAKA_PRIVATE_QF1_MISSING_REPLAY_MATERIAL=1 "$TEST_BINARY" private_tb1_stable_panel --ignored --nocapture
zstd -dc "$p/qf1-missing-replay-material.txt.zst" > "$out/missing.log"
python3 -B "$p/check_real_lifecycle.py" "$out/missing.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --missing-material
```

正常系の Original `2e5e95f3…` では、選出2席がそれぞれ全入力から再実行し、追加質問や Court を開かずに署名した。3ピアが DAA 27 に承認集合を収録し、元期限の DAA 60 でも当時の有効な記録を読み取れた。同じ承認集合を DAA 60 の別ブロックに載せた場合、3ピアとも期限後の新規受付を拒否した。比較にはブロックヘッダーの実際の DAA を使う。検証対象は稼働中のピアが保持した抽選情報に基づく承認記録である。[正常系の照合結果](qf1-undisputed-replay.json)・[生ログ](qf1-undisputed-replay.txt.zst)

```sh
MISAKA_PRIVATE_QF1_NO_DISPUTE=1 "$TEST_BINARY" private_tb1_stable_panel --ignored --nocapture
zstd -dc "$p/qf1-undisputed-replay.txt.zst" > "$out/undisputed.log"
python3 -B "$p/check_real_lifecycle.py" "$out/undisputed.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --undisputed
```

期限境界を検査した同一バイナリ（SHA-256 `1ff0718d227c8fe777e425225a13b8e8029360c59a1ef8ae84159020429e3c29`）で、署名済みの Readout が矛盾する Original `9a50bf6b…` も実行した。選出2席が完全再実行して承認を拒否し、3ピアは元の開示から、正しい Stop に対する異議を棄却し、End との不一致を `ComputationRefuted` と裁定した。正常・資料不足・不一致の3試験は、それぞれ新しい Original と3つの独立プロセスで実行した。[不一致の照合結果](qf1-replay-deadline-refuted.json)・[生ログ](qf1-replay-deadline-refuted.txt.zst)

### 8K の元の Final に対する Gate C 認証

Qwen2.5 の claim `8748971c…` では、producer を除く抽選済み7名が元の計算を個別に再実行した。署名付き承認は DAA 7,933〜7,939 に収録され、75の抽選枠が承認された。元の Final DAA 6,633 から訴追期間 5,040 DAA が経過した DAA 11,673 で、書き込みピアの native 状態が `Certified` を返した。照合器は抽選後の7,481ブロックの親・ヘッダーハッシュ、7件の取引の署名形式・bond・対象 claim・実行 transcript を照合する。署名自体と承認の合意判定は native 実行で検査される。これは隔離チェーン上の旧形式の計算である。[認証結果と7件の収録位置](genesis-funded-certificate.json)・[書き込みピアのログ](genesis-funded-certificate-writer.txt.zst)・[追加の raw ブロック](genesis-funded-certificate-blocks.json.zst)

```sh
zstd -dc "$p/genesis-funded-certificate-blocks.json.zst" > "$out/certification.json"
zstd -dc "$p/genesis-funded-certificate-writer.txt.zst" > "$out/cert-writer.log"
zstd -dc "$p/genesis-draw-block.json.zst" > "$out/original-attempt.json"
python3 -B "$p/check_funded_certificate.py" "$out/certification.json" "$out/cert-writer.log" "$out/original-attempt.json"
```

### MODEL_AUDIT の公開期限と holder の署名予約

登録済みモデルの非報酬監査で、close は DAA 6、公開窓は DAA 9 以上 21 未満だった。DAA 9 で両方の操作が公開を保留した。元期限後の DAA 33 でも finality は未成立で、3ピアすべてが未公開 holder の署名予約と監査の公開判定の両方で `TimedOut` を返した。同じビルドで、すでに公開した holder が finality 撤回後と再起動後も停止する回帰試験に合格した。[3ピアの期限境界](registered-audit-exclusive-deadline.txt.zst)

### 認証の独立再生と元の Qwen3 監査の期限

書き込みピアと同じバイナリの別の2プロセスが、それぞれ新規 DB で隔離チェーンを再生した。旧 Qwen2.5 claim `8748971c…` について、両者が同じ Final、75枠の Gate C 認証、対象ブロック、状態 root を読み出した。DAA 11,673 の両ピアで、別の Qwen3 監査の署名済み close `653ff4fa…` は公開保留だった。状態が示す未成熟の仕事量は 1,388,926,972,416、成熟済みの仕事量は 0。最も早い成熟 DAA 17,931 は、監査の元期限 15,121 を 2,810 DAA 過ぎる。[認証と監査状態](genesis-funded-certificate-replicas.json)・[読取ピア B](genesis-funded-certificate-reader-b.txt.zst)・[読取ピア C](genesis-funded-certificate-reader-c.txt.zst)

```sh
for k in b c; do zstd -dc "$p/genesis-funded-certificate-reader-$k.txt.zst" > "$out/cert-reader-$k.log"; done
python3 -B "$p/check_funded_certificate.py" "$out/certification.json" "$out/cert-writer.log" "$out/original-attempt.json" --readers "$out/cert-reader-b.log" "$out/cert-reader-c.log"
```

同じ署名済み close を保持した追加の native プロセスで、認証先端から通常の heartbeat だけを 6,896 ブロック追加した。ノードは DAA 15,120 の最後の2ブロックで公開を保留し、元期限 DAA 15,121 の最初のブロックで `TimedOut` を返した。全ヘッダーの親とハッシュ、pruning point を照合した。終了時の blue score は 30,302、検証済み pruning point は genesis で、safe・finalized ともに空だった。DAA 15,120 と 15,121 の停止位置では未成熟の仕事も 0 件だった。DAA 17,931 は認証直後の状態で表示された最早成熟時刻であり、そこで公開できることを確認した値ではない。監査 seed の公開・採点には至らなかった。[期限前後の照合](genesis-audit-release-window.json)・[native ログ](genesis-audit-release-window.txt.zst)・[追加ブロック](genesis-audit-release-window-blocks.jsonl.zst)

```sh
zstd -dc "$p/genesis-audit-release-window-blocks.jsonl.zst" > "$out/window.jsonl"
zstd -dc "$p/genesis-audit-release-window.txt.zst" > "$out/window.log"
python3 -B "$p/check_audit_release_window.py" "$out/window.jsonl" "$out/window.log"
```

### 8K の認証済み仕事が成熟したブロック

同じ隔離チェーン上で Qwen2.5 の claim `8748971c…` と Qwen3 の監査 close `653ff4fa…` を追跡した。DAA 15,121 の監査期限では、readiness が返す未成熟件数は0だったが、元の仕事IDの保存台帳（table 196）には記録が残っていた。以後、通常の heartbeat で DAA 17,931 まで進めると、その仕事IDが成熟済み台帳（table 195）に現れ、`safe` は元の8K claim受付ブロック `3250abac…` に達した。そこでの blue score は35,922、検証済み pruning point は genesis、`finalized` は空だった。genesis から pruning point が進むための最低深度74,920までは、なお38,998 blue score 必要だった。DAA と blue score は別の時計である。

認証ブロックとその次のブロックはいずれも DAA 11,673 だった。後者でも元のclaimと1件の未成熟仕事（1,388,926,972,416）が存在した。最初の DAA 11,674 のブロックで、元の仕事IDのclaim削除が native delta に記録された。bondと保存台帳の行は残った。readiness が示す0件は、その時点の先頭の未決済ブロックについての件数である。

元の監査は成熟時点でも `TimedOut` を返した。後続5,621ブロックには、成熟時の状態rootをヘッダーに固定した直後の1ブロックを含む。再現にはこの節の前で使った署名済み監査・資本・認証の履歴一式を用いる。[追加ブロック](genesis-work-maturity-blocks.jsonl.zst)・[生成側ログ](genesis-work-maturity-writer.txt.zst)

生成側は `7120c11d…`、削除直後の観測と保存ブロックの読み戻しを加えた読取側は `43b452fc…` を使用した。読み取り側は別の新規DBで、認証履歴、元期限までの6,896ブロック、成熟時点を含む追加ブロックを順に検証した。照合器はヘッダーの親とハッシュ、状態を観測した各ブロックの直後のヘッダーに入る状態root、両プロセスの成熟時の観測を比較する。[削除・成熟の照合結果](genesis-work-maturity.json)・[読取側ログ](genesis-work-maturity-reader.txt.zst)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
zstd -dc "$p/genesis-audit-release-window-blocks.jsonl.zst" > "$out/window.jsonl"
zstd -dc "$p/genesis-work-maturity-blocks.jsonl.zst" > "$out/maturity.jsonl"
zstd -dc "$p/genesis-work-maturity-writer.txt.zst" > "$out/maturity-writer.log"
zstd -dc "$p/genesis-work-maturity-reader.txt.zst" > "$out/maturity-reader.log"
python3 -B "$p/check_work_maturity.py" "$out/window.jsonl" "$out/maturity.jsonl" \
  "$out/maturity-reader.log" "$out/maturity-writer.log" > "$out/maturity-checked.json"
cmp "$out/maturity-checked.json" "$p/genesis-work-maturity.json"
```

nativeで再生するには、上の認証readerの設定に `MISAKA_PRIVATE_AUDIT_WORK_RETIREMENT_READ="$out/window.jsonl"` と `MISAKA_PRIVATE_AUDIT_WORK_MATURITY_READ="$out/maturity.jsonl"` を加え、同じ `private_registered_audit_legacy_work` を新規DBで実行する。成熟区間の生成時は `MISAKA_PRIVATE_AUDIT_WORK_MATURITY_WRITE` に新規ファイルを指定する。

### G-COST 改訂後の予備検査

同一の整数 Qwen3-8B descriptor で、Panel が承認した過去の6回の隔離実行を生ログから照合した。6試行では EXEC seed と入力列が各回で異なり、回答列は同一だった。試行の全15組で数値 Readout `z` が異なり、別試行の `z` を変更せずに再利用すると正規値に一致する例は0件だった。先頭の共通入力は組ごとに14〜16 token あった。この照合は採掘値全体をそのまま再利用する場合を対象とする。共通 prefix の費用は次節で実測した。QF1 の G-COST は UNKNOWN である。[検査結果](g-cost-readout-reuse.json)・[生ログからの照合器](check_g_cost_readout_reuse.py)

```sh
python3 -B "$p/check_g_cost_readout_reuse.py"
```

先頭の署名済み試行の回答と数値 Readout を固定し、残り5件の試行IDでそれぞれの Root を再計算した。9 token の回答列と回答 Root は5件すべてで元の値に一致し、数値 Readout の Root は5件とも異なった。同じ回答が出た過去の標本を使っており、未知の仕事で停止位置を予測できる頻度や、初回 Ticket までの費用は未測定である。[再計算した Root と照合結果](g-cost-cached-answer.json)

```sh
python3 -B "$p/check_g_cost_cached_answer.py" | cmp - "$p/g-cost-cached-answer.json"
```

### 共通 prefix の計算を共有する経路の費用

3組すべてで、共通 prefix の数値計算を共有しても6件の元の Ticket を再現できた。Apple M1 Max・3スレッドでの合計は独立実行475.835秒、共有実行447.899秒で、経過時間は5.87%短縮した。初期化は23.651秒だった。各組の実測を以下に示す。[照合結果](g-cost-shared-prefix.json)・[実行生ログ](g-cost-shared-prefix-run.jsonl)・[実行環境とハッシュ](g-cost-shared-prefix-environment.json)

| 共通入力（token） | 独立実行（秒） | 共有実行（秒） |
| ---: | ---: | ---: |
| 16 | 158.924 | 148.457 |
| 14 | 158.402 | 149.752 |
| 14 | 158.510 | 149.690 |

この入力集合では、共有によって同じ Ticket の検証を終えるまでの計算を実際に減らせた。公開採掘に使う事前閾値と誤差範囲は未採択であり、QF1 の G-COST 判定は `UNKNOWN` である。

上の6件をログ順に2件ずつ組にし、同じ Qwen3-8B 整数実装で2通りの計算を比較する。基準は各入力を先頭から独立して計算する。もう一方は共通する入力位置を1回計算し、各入力固有の位置からは KV 状態を分ける。どちらの経路でも、まず回答の停止位置を計算で決め、次に trace を作る。各 Original の入力、回答、数値 Readout、4つの root、Ticket が保存済みの署名資料と一致した場合に限り、その測定行を出力する。

2件分の経過時間には共有状態の生成・複製と、各試行に固有の trace の構築を含む。モデルの読み込みと照合は全組の前に1度だけ行い、その時間を別に記録する。各組で各経路を1回ずつ実行し、先に走る経路を組ごとに入れ替える。これは過去の Original を使うローカル評価で、費用比の採択閾値、同一入力での計測揺らぎ、新たな監査抽選は含まない。

上記の Q15 全重みを `parent`、作業用ディレクトリを `out` に指定して再現できる。

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
python3 -B "$p/check_g_cost_shared_prefix.py" > "$out/plan.json"
zstd -dc "$p/qf1-residual-tiles.section.zst" > "$out/scales.section"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 RAYON_NUM_THREADS=3
export CARGO_TARGET_DIR=target/protected-core-tests
cargo +1.93.0 --config 'profile.dev.package.misaka-palw-tir-exec.opt-level=3' build \
  --locked --offline -p misaka-palw-sdk --example qf1-paired-cost
"$CARGO_TARGET_DIR/debug/examples/qf1-paired-cost" "$parent" "$out/scales.section" "$out/plan.json" > "$out/run.jsonl"
python3 -B "$p/check_g_cost_shared_prefix.py" --log "$out/run.jsonl"
```

### 新しい監査 close と holder の再起動

登録済みの整数 Qwen3-8B（class `b1da96d1…`）で、元期限 DAA 15,121 に達した監査とは別に、非報酬の MODEL_AUDIT と後続の評価 PANEL を準備した。公開窓は署名前に監査へ `[6, 90641)`、評価 PANEL へ `[90741, 256802)` を割り当てた。新しい監査 close は `67a7b2b0…`。この窓幅は隔離チェーンの観測に基づく試験設定で、公開 T12 の予定を示すものではない。

生成側は用途ごとに9件の署名済み READY を記録し、holder 3人それぞれに元の share と署名予約の journal を保存した。同じ実行ファイルの別々の3プロセスが新規 DB で登録と close を再生し、各自の2用途の資料を開き直した。各プロセスで元の READY 署名から同じ資格を再構成でき、署名を1件変えた対照は失敗した。DAA 6 では監査の finality が未成立で、監査の公開判定と署名予約はともに `WouldBlock` だった。試験用の dealer・署名鍵・予約を使っており、分散した dealer 間通信と実予算の保証はこの試験の対象外である。

同じ枝上で旧規則の Qwen2.5-1.5B 8K の仕事を1回実行し、通常の native テンプレートが新規 claim `50f0da2d…` を DAA 6,491 のブロック `5292996b…` で受理した。3プロセスが同じ2,409,131バイトの計算資料を読み、数値を再計算して trace・出力・実行 root と受付後の状態 root の一致を確認した。観測した claim の状態は `Provisional`。Qwen3 の監査 seed 公開、実用性・費用の判定、新規 FOW1 の受賞ブロックは引き続き未確認である。[照合結果](fresh-retained-audit.json)・[ブロック・計算資料・4プロセスの記録](fresh-retained-audit-public.tar.zst)

公開記録のみでヘッダー・Ticket・各プロセスの記録を照合できる。

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
zstd -dc "$p/fresh-retained-audit-public.tar.zst" | tar -xf - -C "$out"
python3 -B "$p/check_fresh_audit.py" "$out" > "$out/checked.json"
cmp "$out/checked.json" "$p/fresh-retained-audit.json"
```

native の再生には上記の全重み、登録済み residual scale section、Studio の TIR artifact、genesis の 8K artifact と、対象 holder が保持する元の非公開資料が必要である。次の変数を設定し、各 holder の独立プロセスで ignored test `private_fresh_audit_custody` を実行する。`slot` は 0・1・2 のいずれかである。

```sh
export MISAKA_PRIVATE_QF1_FULL="$parent" MISAKA_PRIVATE_QF1_TILES="$scales"
export MISAKA_PRIVATE_TB1_TIR_ARTIFACT="$studio" MISAKA_PRIVATE_TB1_GENESIS8K="$model8k"
export MISAKA_PRIVATE_FRESH_SLOT="$slot" MISAKA_PRIVATE_FRESH_HOLDER="$holder_dir"
export MISAKA_PRIVATE_CERTIFIED_AUDIT_READ="$out/public.jsonl" MISAKA_PRIVATE_AUDIT_WORK_READ="$out/capital.json"
export MISAKA_PRIVATE_GENESIS_SEATS_READ="$out/seats.json" MISAKA_PRIVATE_GENESIS_CAPITAL_READ="$out/genesis-capital.json"
export MISAKA_PRIVATE_GENESIS_RENEW_READ="$out/renew.json" MISAKA_PRIVATE_GENESIS_ATTEMPT_READ="$out/accepted"
RUST_MIN_STACK=67108864 RAYON_NUM_THREADS=3 "$test_binary" private_fresh_audit_custody --ignored --nocapture --test-threads=1
```

### 新しい監査 close と同じ枝での 8K claim の Final

11月22日の隔離試験で、上記の Qwen2.5-1.5B 8K claim `50f0da2d…` が DAA 6,633 のブロック `a615adb3…` で Final に到達した。選出に先立ち8席が readiness を更新し、DAA 6,511 の実行ブロック `a63633f0…` が Panel 抽選の起点になった。選出されたカード0・3・4は、元の計算資料から各自で数値を再実行し、それぞれ有効の receipt に署名した。受付ブロック `5292996b…` から Final までの285ブロックで元の claim ID、3件の署名付き receipt、選択親とハッシュを照合した。Final 時点の仕事は Gate C の承認待ちだった。[Final の照合結果](fresh-8k-final.json)・[署名済みブロックとログ](fresh-8k-final.tar.zst)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
zstd -dc "$p/fresh-8k-final.tar.zst" | tar -xf - -C "$out"
python3 -B "$p/check_fresh_final.py" "$out" > "$out/checked-final.json"
cmp "$out/checked-final.json" "$p/fresh-8k-final.json"
```

native の生成側は前節の再生設定に `MISAKA_PRIVATE_GENESIS_FINAL_WRITE` を追加して、まだ存在しない出力ファイルを指定する。署名済みの Final から再開するプロセスでは、代わりに `MISAKA_PRIVATE_GENESIS_SUFFIX_READ` にそのファイルを指定する。fixture の鍵を持つ1プロセスで選出席ごとに再実行しており、別々の運営者による署名を実証したものではない。

### 新しい監査 close と同じ枝での Gate C 認証

前節の Final を新規 DB で再生した時点では、Gate C の抽選・承認はともに0枠だった。カード5が floor 用に 46,953,160,500,520 sompi の資本を割り当てた。DAA 7,932 に予算が 1,021,082 microblocks となり、通常の実行ブロック `89e2e2f3…` から75枠が抽選された。元の抽選期限は DAA 8,793。採掘者を除く7枚のカードが、それぞれ元の8K仕事を再実行し、同じ transcript に署名した。DAA 11,673 のブロック `0b30c252…` が claim `50f0da2d…` を75枠の承認で `Certified` とした。

2つの別の新規 DB が受付から認証までの10,366ブロックを受理した。生成側を含む3プロセスが同じバイナリを使った。照合器は3席の Panel receipt、資本割当、7件の Gate C 取引の対象・署名者・transcript を確認し、2つの読み取り側の状態 root と監査の公開判定を比較する。各署名者の再実行には隔離試験鍵を用いた。[照合結果](fresh-8k-certificate.json)・[抽選・認証ブロックと各プロセスの記録](fresh-8k-certificate.tar.zst)

認証直後の2つの読み取り側で、当該 prefix の仕事 1,388,926,972,416 は1件の成熟待ちだった。台帳が示した最早成熟 DAA は 17,931（残り6,258 DAA）。今回の監査の署名済み期限 DAA 90,641 より前である。DAA 11,673 では `safe` と `finalized` はともに空、監査 seed の公開判定は保留だった。

同じバイナリで別途 Studio モデルを登録した3プロセスは、登録に伴う市場生成と MOE2 の配分を再生した。試験用の160単位のうち買付48、永久LP32、worker80（miner64・Panel16）。購入した2 Position を永久LPに1、burn に1配分し、巻き戻しと再適用後の市場・worker 権利・LP が3者で一致した。成熟と支払条件には fixture を用いた。8K claim の実支払いはこの観測に含まれない。

公開記録を次の手順で照合できる。

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
zstd -dc "$p/fresh-retained-audit-public.tar.zst" | tar -xf - -C "$out" accepted/winning.json
zstd -dc "$p/fresh-8k-final.tar.zst" | tar -xf - -C "$out"
zstd -dc "$p/fresh-8k-certificate.tar.zst" | tar -xf - -C "$out"
python3 -B "$p/check_fresh_certificate.py" "$out" "$out/accepted/winning.json" > "$out/checked-certificate.json"
cmp "$out/checked-certificate.json" "$p/fresh-8k-certificate.json"
```


native の生成は前節の `MISAKA_PRIVATE_GENESIS_SUFFIX_READ` に Final の履歴を指定し、`MISAKA_PRIVATE_GENESIS_FUNDED_ANCHOR` に未作成のディレクトリを指定する。生成された `draw.json` と `certification.json` は、それぞれ前区間の tip を base に持つ。読み取り側は次のファイルを `MISAKA_PRIVATE_GENESIS_CERTIFY_READ` に指定し、前節の元の8K受付・holder資料を使って同じ ignored test を新規 DB で実行する。読み取り側には Final の生成・再開用の環境変数を渡さない。

```sh
python3 - "$out" <<'PYTHON'
import json, sys
from pathlib import Path

root = Path(sys.argv[1])
names = ("final.json", "funded/draw.json", "funded/certification.json")
parts = [json.loads((root / name).read_bytes()) for name in names]
chain = parts[0]
for part in parts[1:]:
    assert (part["base"], part["claim"]) == (chain["tip"], chain["claim"])
    chain["blocks"].extend(part["blocks"])
    chain["tip"] = part["tip"]
with (root / "accepted-to-certified.json").open("x") as output:
    json.dump(chain, output, separators=(",", ":"))
PYTHON
```

### 監査 close を含む最初の pruning point

隔離チェーンで、8K claim の Gate C 認証ブロック `0b30c252…` から通常の heartbeat を52,170個受理した。最後のヘッダー `50f5791a…` は DAA 37,758、blue score 75,520。ヘッダーに記録された pruning point `98da02e7…` は blue score 600 で、同じ選択鎖上の監査 close `67a7b2b0…`（blue score 5）を含む。照合器は各ヘッダーのハッシュと親を再計算し、この包含関係を登録時の公開ブロックまで遡って検査する。[照合結果](fresh-pruning-transition.json)・[公開ブロックと生成ログ](fresh-pruning-transition.tar.zst)

次のテンプレートを要求した時点でノードが `LocalNativeStateContextUnavailable(HistoryUnavailable)` を返し、生成試験は失敗した。生成側が最後に観測した公開判定は認証直後の保留で、先端での holder の署名可否はこのログからは確定できない。署名済み期限は DAA 90,641。この記録は非報酬監査の履歴である。

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
tar -xf "$p/fresh-pruning-transition.tar.zst" -C "$out"
python3 -B "$p/check_fresh_window.py" "$out/after-cert.jsonl" "$out/writer.log" | cmp - "$p/fresh-pruning-transition.json"
```

同じ公開履歴を空のデータベースから読み直した holder 0 は、認証から12,516ブロック後の DAA 17,931 で、8K claim `50f0da2d…` の仕事 `70daa3ca…` を native 台帳の table 195 に確認した。行の claim と成熟 DAA を照合し、その状態 root が次の公開ヘッダーに含まれることも検査した。この時点の safe は元の受付ブロック `5292996b…`、finalized は空で、監査 share の公開は保留だった。下記のログは成熟時点までの抜粋である。[読み直しの記録](fresh-work-maturity.log)・[照合結果](fresh-work-maturity.json)

```sh
python3 -B "$p/check_fresh_window.py" "$out/after-cert.jsonl" "$out/writer.log" \
  --maturity-log "$p/fresh-work-maturity.log" | cmp - "$p/fresh-work-maturity.json"
```

同じ holder 0 は52,170個すべてを再生した。DAA 37,758 で readiness は pruning point `98da02e7…` を finalized と表示したが、holder が参照する native settlement snapshot では finalized head を取得できず、公開ゲートは `WouldBlock` を返した。署名処理はそこで終了し、この実行で share は公開されなかった。これは期限前の一時点の観測である。[公開判定と終了ログ](fresh-holder-window-end.log)・[到達点の照合](fresh-holder-window-end.json)

```sh
python3 -B "$p/check_fresh_window.py" "$out/after-cert.jsonl" "$out/writer.log" \
  --held-endpoint-log "$p/fresh-holder-window-end.log" | cmp - "$p/fresh-holder-window-end.json"
```

履歴を保持する別プロセス（PID 10387）で同じ52,170ブロックを再生し、台帳の pruning snapshot が `98da02e7…` に保存されたことを確認してから、同じ先端で holder 0 に署名を要求した。成熟した work、先端の状態 root、期限、公開判定は先のプロセスと一致した。保存完了後も署名時の settlement snapshot から finalized head は取得できず、holder は署名を拒否した。[保存から署名拒否までの連続ログ](fresh-captured-window-end.log)・[独立照合結果](fresh-captured-window-end.json)

```sh
python3 -B "$p/check_fresh_window.py" "$out/after-cert.jsonl" "$out/writer.log" \
  --captured-endpoint-log "$p/fresh-captured-window-end.log" | cmp - "$p/fresh-captured-window-end.json"
```

非報酬の MODEL_AUDIT と後続の評価 PANEL では、各 round の3 dealer に対する3 holder の READY 計18署名を、公開ブロックに固定された元記録から OpenSSL 3.6.3 で検証した。隔離 fixture の[4つの公開鍵](fresh-holder-public-keys.bin)は slot 順の ML-DSA-65 生鍵（各1,952 bytes）で、元の固定試験鍵に対応する。検証器は公開鍵・座標から roster root を求め、各 round の用途・期限、dealer の transcript と各署名から二つの qualification root を再計算し、準備ブロックと close の commitment に照合する。READY は準備時の署名で、share の公開記録は別に検査する。[照合結果](fresh-qualification.json)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
# macOS / Homebrew。PATH 上の openssl が ML-DSA-65 対応なら --openssl openssl でも実行できる。
python3 -B "$p/check_fresh_qualification.py" --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/fresh-qualification.json"
```

### 元の監査で最初の holder が署名したブロック

同じ holder 0 を新しい DB から再起動し、元の52,170ブロックと pruning checkpoint を再検証した。先端 `50f5791a…` では、署名時の settlement snapshot の finalized は空だった。通常の後続ブロック `0f1fb6bf…` を追加すると、DAA は同じ37,758、blue score は75,520から75,521となり、次の snapshot に finalized `98da02e7…` が記録された。元期限 DAA 90,641 の内側で holder 0 は元の3 dealer に対する share に署名した。

署名メッセージの round・qualification・freeze ID・slot・3つの share を再構成し、元の holder 公開鍵と release 専用の署名 domain で ML-DSA-65 を独立検証した。各 share を含む登録時の custody seal も一致した。この記録の公開者は1人。seed の採用には残り2人の公開とネイティブの batch・certificate 検証が必要になる。[後続ブロック・公開された署名行・連続ログ](fresh-holder-0-release.tar.zst)・[検証結果](fresh-holder-0-release.json)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
tar -xf "$p/fresh-holder-0-release.tar.zst" -C "$out"
python3 -B "$p/check_fresh_holder_release.py" "$out" --holder 0 --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/fresh-holder-0-release.json"
```

### Studio 登録市場の永久 LP と会計記録の再計算

監査 holder 0 の署名試験と同じバイナリ `8b08e725…` で、別の隔離3プロセスが Studio モデルを登録し、同じ精算帳簿（1,789 bytes）と巻き戻し記録（1,219 bytes）を保存した。照合器は3者の checkpoint root と帳簿の Borsh bytes を結び、登録直後の仮想 reserve・Position 在庫と元の予約条件から両方の市場取引を再計算する。対象は隔離用に成熟条件を与えた160単位の予約である。

買付48で2 Position を取得し、永久 LP へ32と1 Position を入れ、余剰1 Position を burn した。残り80は miner 64、Panel 16。登録市場の実 reserve は80増加し、元の seed・owner 手数料は同額だった。同じ帳簿の claim に、LP receipt、モデル ID と artifact root の永久 LP 履歴、元の巻き戻し記録が結び付いた。新規 FOW1 の受賞に起因する支払はこの試験の対象外である。[3ピアの帳簿・巻き戻し記録・ログ](studio-moe2-book.tar.zst)・[再計算結果](studio-moe2-book.json)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
tar -xf "$p/studio-moe2-book.tar.zst" -C "$out"
python3 -B "$p/check_studio_moe2_book.py" "$out" | cmp - "$p/studio-moe2-book.json"
```

### 同じ確定ブロックでの2人目の監査 share 公開

holder 1 は新しい native DB で元の認証と成熟を再生し、52,171 個の後続ブロックを受理した。DAA 37,758 の先端 `0f1fb6bf…` で確定点 `98da02e7…` を読み、監査 close `67a7b2b0…` に対して署名した。holder 0 と holder 1 は別プロセス・別鍵で、同じバイナリ、先端、確定点、状態 Root に到達した。2件とも元の custody seal と ML-DSA-65 署名を照合できた。最後の heartbeat は coinbase 全体からヘッダーの取引 Merkle root も再計算した。この時点で公開済みは2人。seed の公開条件は3人である。[holder 1 の公開行と連続ログ](fresh-holder-1-release.tar.zst)・[照合結果](fresh-holder-1-release.json)

上の holder 0 のファイルを展開した同じディレクトリで検証できる。

```sh
tar -xf "$p/fresh-holder-1-release.tar.zst" -C "$out"
python3 -B "$p/check_fresh_holder_release.py" "$out" --holder 1 --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/fresh-holder-1-release.json"
```

### 3件の公開行からの MODEL_AUDIT seed と評価 challenge

holder 2 も独立した DB で先端 `0f1fb6bf…` と確定点 `98da02e7…` を再現し、元の監査 close に署名した。3人の公開行をまとめた batch は blue score 75,522、その直後の certificate は 75,523 で受理された。いずれも隔離チェーンの DAA 37,759 で、監査の公開期限 DAA 90,641 の内側にある。

公開行の署名と元の custody seal を照合し、3 dealer の多項式を復元して、各 dealer が登録時に固定した値と比較した。coinbase の marker と両ヘッダー、batch から certificate への親子関係を再計算した。独立に導出した seed は `295579de916fffa6989aa64670953af0006f33add45becb3084916439d0267a5`。1候補に割り当てた2件の challenge も、certificate を受理した peer 2 の native 結果と一致した。公開行を2件に減らす、署名を変える、certificate の coinbase を変える各対照は拒否された。[署名行・両ブロック・連続ログ](fresh-audit-publication.tar.zst)・[seed・各 challenge・照合したプロセス](fresh-audit-publication.json)

この記録で certificate を読んだ native プロセスは1件。残る2ピアでの再生は別に記録する。対象は非報酬の監査抽選で、監査 suite の採点や新規 claim の採掘資格は未確定である。

```sh
for file in fresh-holder-0-release fresh-holder-1-release fresh-audit-publication; do
    tar -xf "$p/$file.tar.zst" -C "$out"
done
python3 -B "$p/check_fresh_audit_seed.py" "$out" --publisher-only --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/fresh-audit-publication.json"
```

### 停止位置を予測して全 trace を生成した費用

6件の署名済み Original の先頭を通常どおり検証し、観測した回答長9を残り5件すべてに使った。予測経路でも greedy 選択と Stop は毎回実行し、回答列・数値 Readout・四つの Root・Ticket を元の値と照合した。基準経路は停止位置を実行で確定してから改めて trace を作る。両経路の実行順を組ごとに逆転した。5件の基準経路合計は396.068秒、予測経路合計は205.998秒で、5件すべてで同じ正規 Ticket を再現した。モデルの準備と最初の1件を含む合計はそれぞれ506.340秒、316.270秒。対照として最初の対象で予測を8に変えると trace の葉数が合わず、失敗分と元の検証を合わせて120.020秒かかった。これは非報酬の再生で、各経路・各対象を1回計時した。元の回答長が異なる仕事での成功頻度は未測定である。[計測結果](g-cost-predicted-length.json)・[生ログ](g-cost-predicted-length-run.jsonl)・[事前の比較手順・環境](g-cost-predicted-length-environment.json)

共通 prefix 試験と同じ `parent`、scale section、署名資料から作った `plan.json` を使う。上記のビルド設定と3スレッドで次を実行する。

```sh
"$CARGO_TARGET_DIR/debug/examples/qf1-paired-cost" --predict-length "$parent" "$out/scales.section" "$out/plan.json" > "$out/length-run.jsonl"
python3 -B "$p/check_g_cost_predicted_length.py" "$out/length-run.jsonl"
python3 -B "$p/check_g_cost_predicted_length.py" "$p/g-cost-predicted-length-run.jsonl" | cmp - "$p/g-cost-predicted-length.json"
```

### 異なる仕事の費用検査で使うバイナリの MOE2 再検証

新しい3問の EXEC 抽選前に、仕事の本文・生成上限16・比較方法・固定予測長9を記録した。隔離試験では各本文を元の tokenizer で符号化し、EXEC 受付の署名対象へ含める。[事前に固定した3問と測定手順](g-cost-varied-jobs-schedule.json)

この試験用の native バイナリ（SHA-256 `ad5f222a…`）で Studio モデルの登録・市場生成・報酬配分と巻き戻しを3プロセスから再検証した。3者が保存した帳簿から、対象160に対する買付48・永久LP向け32・worker残額80、Position 2単位のうち1単位を永久LPに、1単位をburn、残額のうちminer64・Panel16を再計算できた。原資は隔離試験の worker 予算である。[3者の結果](varied-jobs-moe2-book.json)・[生ログと両方の帳簿](varied-jobs-moe2-book.tar.zst)

```sh
tar -xf "$p/varied-jobs-moe2-book.tar.zst" -C "$out"
python3 -B "$p/check_studio_moe2_book.py" "$out" | cmp - "$p/varied-jobs-moe2-book.json"
```

### MODEL_AUDIT の certificate を隔離3ピアで再生

公開した peer 2 に続き、peer 0 と peer 1 が別々の新しい DB で元の認証・成熟・後続ブロックを再生し、同じ batch と certificate を受理した。3者は同一バイナリ（SHA-256 `8b08e725…`）で、公開済みの3署名から、同じ seed `295579de…`、2ケースの challenge、受理後の状態 Root `0ca846c3…` を得た。再生した2者は新たな share 公開に署名していない。再生側ログに署名行を加える、seed または状態 Root を変える対照は、いずれも照合器が拒否した。この段階で確定したのは非報酬評価の検査入力である。[3ピアの照合結果](fresh-audit-readback.json)・[追加2ピアの連続ログ](fresh-audit-readback.tar.zst)

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
out=$(mktemp -d)
for file in fresh-holder-0-release fresh-holder-1-release fresh-audit-publication fresh-audit-readback; do
    tar -xf "$p/$file.tar.zst" -C "$out"
done
python3 -B "$p/check_fresh_audit_seed.py" "$out" --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/fresh-audit-readback.json"
```

### 異なる3問での固定長予測

抽選前に固定した順に、各問を異なるEXECとPANELで実行した。照合器は各EXECの試行IDと仕事の本文からtoken列を組み立て、署名対象の入力・回答・数値Readout・四Root・Ticketおよび選出Panelの期限内承認に照合する。どの問も、StudioのMOE2試験と同じバイナリ `ad5f222a…` を使った独立した3プロセスで一致した。[事前に固定した仕事](g-cost-varied-jobs-schedule.json)

| 仕事 | 生成token数（EOSを含む場合あり） | 停止理由 | 署名済みOriginalとPanelのログ |
| --- | ---: | --- | --- |
| 日本の首都を都市名だけで答える | 2 | EOS | [第1問](varied-job-0.txt.zst) |
| 3から2を引き、数字一つで答える | 2 | EOS | [第2問](varied-job-1.txt.zst) |
| 葉が緑色に見える理由を英文1文で説明する | 16 | 生成上限 | [第3問](varied-job-2.txt.zst) |

過去の算術の仕事で固定した長さ9は、この3問では一度も一致しなかった。各問は1回の独立抽選で、回答の実用性の採点は行っていない。完全な元の入力とTicketを含む計時用planを再構成するには、元のQF1 tokenizerと以下を使う。計時用planのSHA-256は `ebf752802d705f92fe4657a5911f15c7d311e0adfb95bd625597fed45fe2e33f`。

```sh
p=docs/evidence/palw/mas1-qf1-native-2026-11-21
uv run --offline --no-project --with tokenizers==0.23.3 python -B "$p/check_g_cost_varied_jobs.py" "$MISAKA_PRIVATE_QF1_TOKENIZER" "$p" > "$out/varied-plan.json"
```

3問合計では、基準経路244.377秒、予測経路368.101秒（50.63%増）だった。モデルの準備23.112秒と予測値を決めた元の算術1件79.611秒を両方に加えた合計は347.100秒と470.824秒。別枠で第1問に長さ8を指定した対照は、再試行を含め117.769秒だった。計時は各問・各経路1回で、実行順は問ごとに交替した。採択済みの閾値はなく、同じ算術問題の5試行で観測した短縮も回帰対象に残る。[検証済みの問別費用](g-cost-varied-jobs.json)・[生の計時記録](g-cost-varied-jobs-run.jsonl)・[環境とバイナリのSHA-256](g-cost-varied-jobs-environment.json)

```sh
uv run --offline --no-project --with tokenizers==0.23.3 python -B "$p/check_g_cost_varied_jobs.py" "$MISAKA_PRIVATE_QF1_TOKENIZER" "$p" --log "$p/g-cost-varied-jobs-run.jsonl" | cmp - "$p/g-cost-varied-jobs.json"
RAYON_NUM_THREADS=3 "$CARGO_TARGET_DIR/debug/examples/qf1-paired-cost" --predict-length "$parent" "$out/scales.section" "$out/varied-plan.json" > "$out/varied-run.jsonl"
```

同じバイナリの別の隔離3ピアで、正しい算術のOriginalを選出Panelが承認した後にStopとEndの両方を直接Courtへ提出した。受付前に保持した証拠はそれぞれ1,622 bytes、3,069 bytesで、3者の裁定はいずれも `ChallengeNotEstablished` だった。これは上の3問の費用集計から独立した非報酬試験である。[3ピアと裁定結果](varied-build-direct-court.json)・[元のログ](varied-build-direct-court.txt.zst)

```sh
zstd -dc "$p/varied-build-direct-court.txt.zst" > "$out/varied-build-direct-court.log"
python3 -B "$p/check_real_lifecycle.py" "$out/varied-build-direct-court.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --retained-stop --retained-end | cmp - "$p/varied-build-direct-court.json"
```

同じバイナリで、日本の首都を問う仕事を新たに登録し、選出2席のうち1席で再実行資料を取得できない場合も検査した。3ピアが同じEXECのbatch・certificate・seedと同じ期限ブロックを読み取った。署名は1件で、DAA 59では承認集合が不足し、元の排他期限DAA 60で新規受付が終了した。これはローカルな承認受付の締切までの観測で、未検証終了の合意状態への記録は残る。[3ピアと期限の照合](varied-build-missing-material.json)・[元ログ](varied-build-missing-material.txt.zst)

### 期限後にPANELの承認集合を再構成する

隔離3ピアで、元の排他期限DAA 60に達した後、保持したcut・公開時のgate・batch・certificate・公開行から抽選を組み直し、期限内に収録した承認を読み取った。署名を1件に減らした集合、同じcommitmentを期限に再収録したブロック、期限後の新規抽選受付はいずれも拒否された。履歴の読み取りでも選択鎖・確定点・各署名を検査する。この試験では公開資料とgateをメモリに保持しており、プロセス再起動からの再構成は未検証である。[3ピアの結果](historical-replies.json)・[生ログ](historical-replies.txt.zst)

同じ実行ファイルでStudioの登録市場・160単位の配分・巻き戻しも隔離3プロセスで再照合した。[精算結果](historical-replies-market.json)・[帳簿・巻き戻し・ログ](historical-replies-market.tar.zst)

```sh
zstd -dc "$p/historical-replies.txt.zst" > "$out/historical-replies.log"
python3 -B "$p/check_real_lifecycle.py" "$out/historical-replies.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --recorded-replies | cmp - "$p/historical-replies.json"
```

### ディスクから読み戻したPanel承認署名の独立検証

Qwen3-8Bの隔離3ピアは、DAA 27で受理した2席の公開鍵と署名（計10,522 bytes）を各自のファイルに保存し、DAA 60で開き直して履歴に照合した。別プロセスの検証器は試行・受付・PANEL certificate・seedから署名対象を再計算し、2件のML-DSA-65署名、抽選順、nativeで収録を確認したcommitmentを照合した。切り詰め、末尾への追加、席の入れ替え、署名の1ビット変更を個別に拒否した。抽選時の確定状態はnativeの観測を使用しており、ノード自体を終了してからの履歴再構成は未検証である。[照合結果](public-reply-evidence.json)・[公開署名を含む3ピアログ](public-reply-evidence.txt.zst)・[同じバイナリでのMOE2帳簿と巻き戻し](public-reply-evidence-market.tar.zst)

```sh
zstd -dc "$p/public-reply-evidence.txt.zst" > "$out/public-reply-evidence.log"
python3 -B "$p/check_recorded_reply_signatures.py" "$out/public-reply-evidence.log" --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/public-reply-evidence.json"
```

### 公開時と原期限の native データベースからの復元

隔離3ピアはそれぞれ、PANEL share を公開する前と、Original の排他期限DAA 60で、RocksDB の一貫したチェックポイントを作成した。期限到達後、別の consensus インスタンスで公開前のDBを開き、保存された選択鎖・決済状態から close の確定を再確認した。期限側のDBも新たに開き、前者から再構成した公開条件と保存済みの2署名を使って、DAA 27で収録した承認を検証できた。期限側のDBから新しい公開条件を取得する試みと、期限ブロックで同じ承認を収録する試みは拒否された。[両世代と署名の照合結果](checkpoint-panel-replies.json)・[3ピアの元ログ](checkpoint-panel-replies.txt.zst)

この試験は各ピアのプロセス内でDBを開き直した。元のcut・qualification・公開行は試験プロセスが保持しており、DB単独から全資料を復元したという結果ではない。隔離fixtureは成熟仕事を供給する。同じバイナリでStudioの市場生成・配分・巻き戻しも再検証した。[MOE2の帳簿とログ](checkpoint-panel-market.tar.zst)

```sh
zstd -dc "$p/checkpoint-panel-replies.txt.zst" > "$out/checkpoint-panel-replies.log"
python3 -B "$p/check_recorded_reply_signatures.py" "$out/checkpoint-panel-replies.log" --openssl "$(brew --prefix openssl@3)/bin/openssl" --checkpoints | cmp - "$p/checkpoint-panel-replies.json"
```

### 元の3プロセス終了後のPANEL承認ブロックの照合

各peerはPANEL公開前と期限時のRocksDBチェックポイント、公開鍵・署名、参照するブロックIDを保存した。元の3プロセスが終了した後、同一バイナリの別の3プロセスが各peerのファイルを開いた。各readerは公開前と期限時の両DBでcloseが確定点の祖先であること、選択鎖上に公開前世代・batch・certificate・承認・期限ブロックが順に存在することを確認した。DAA 27の承認とDAA 60の期限ブロックに同じ承認集合のcommitmentが入っていた。元の排他期限を満たしたのは前者である。独立照合器はディスクから読み出した10,522 bytesを元の公開記録と照合し、選出2席の署名を検証した。markerの変更、期限ブロックの承認への差し替え、二つのDBの交換はnative readerが拒否した。[プロセスと照合結果](cold-panel-receipts.json)・[元のログ、否定例、同一バイナリのMOE2帳簿](cold-panel-run.tar.zst)

[再現用の6つのDB](cold-panel-databases.tar.zst)を新しいディレクトリに展開し、さらに別の3プロセスでも同じ承認と期限を再現した。新しいreaderが直接確かめた範囲はnativeの確定状態、選択鎖、commitment、期限と保存された承認である。抽選前の全資料からqualificationとseedを構築し直す部分は元の実行記録を参照する。

```sh
out=$(mktemp -d)
tar -xf "$p/cold-panel-run.tar.zst" -C "$out"
tar -xf "$p/cold-panel-databases.tar.zst" -C "$out"
for slot in 0 1 2; do
    MISAKA_PRIVATE_COLD_PANEL_DIR="$out/peers/$slot" RUST_MIN_STACK=67108864 "$TEST_BINARY" private_read_cold_panel_receipt --ignored --test-threads=1 --nocapture > "$out/fresh-$slot.log" 2>&1 || break
done
python3 -B "$p/check_recorded_reply_signatures.py" "$out/native.log" --openssl "$(brew --prefix openssl@3)/bin/openssl" --cold-readers "$out/fresh-0.log" "$out/fresh-1.log" "$out/fresh-2.log"
```

### 新しいプロセスでのPANEL資格集合とseedの復元

非報酬のQwen3-8B試験で、署名時の9件のREADY、holder公開鍵、dealerのtranscript root、公開済みの3件の署名行を保存した。元の3プロセスの終了後、各peerの新しいreaderは元のREADYからqualificationを構築し、公開前DBの確定状態とcloseのcommitmentを検査した。期限側DBのbatch・certificateと署名行から導出したseedは、元の3ピアと一致した。各dealerの復元多項式も保存したtranscriptと一致した。署名2席の承認はDAA 27、排他期限はDAA 60だった。[元プロセスとreaderの照合](recovered-panel-seed.json)

READY署名の変更、公開行の署名変更、公開行を2件へ削減、consumer集合のroot変更、dealer transcriptの変更を個別に拒否した。consumer集合のrootはcloseのpreimageとして検査しており、受付した全Originalからこの集合を再集計する部分は残る。あわせて同じバイナリでMOE2の登録市場と巻き戻しを再照合した。初回の市場試験は並行出力が混在して照合器が完了行を認識できなかったため、独立に取り直した実行を収録した。[実行ログ・否定例・市場帳簿](recovered-panel-seed-run.tar.zst)

[再現用チェックポイントと公開資料](recovered-panel-seed-databases.tar.zst)を展開し、別の3プロセスからも再検証できた。

```sh
out=$(mktemp -d)
tar -xf "$p/recovered-panel-seed-run.tar.zst" -C "$out"
tar -xf "$p/recovered-panel-seed-databases.tar.zst" -C "$out"
for slot in 0 1 2; do
    MISAKA_PRIVATE_COLD_PANEL_SEED=1 MISAKA_PRIVATE_COLD_PANEL_DIR="$out/peers/$slot" RUST_MIN_STACK=67108864 "$TEST_BINARY" private_read_cold_panel_receipt --ignored --test-threads=1 --nocapture > "$out/fresh-$slot.log" 2>&1 || break
done
python3 -B "$p/check_recorded_reply_signatures.py" "$out/native.log" --openssl "$(brew --prefix openssl@3)/bin/openssl" --require-seed --cold-readers "$out/fresh-0.log" "$out/fresh-1.log" "$out/fresh-2.log"
```

### PANEL受付集合と抽選の再現

隔離3ピアの各writerが終了してから、各DBを開いたreaderが、提出済みOriginal 1件の受付行と3候補の重みからconsumer集合のrootを再計算した。元のcloseに一致することを確認し、復元したseedで2席を引き直して、それぞれの完全再実行承認署名と受付ブロックのcommitmentを照合した。承認はDAA 27、排他期限はDAA 60。公開資料のアーカイブを展開した別の3プロセスでも同じ結果を得た。この試験は受付済みの1件を対象とし、readerがQwen推論をもう一度実行する試験や、新しいclaimの報酬決済は含まない。[照合結果](recovered-panel-intake.json)

受付行の除去、資料予約の変更、候補の重み変更、2席の承認の入れ替えを個別に拒否した。[実行ログ](recovered-panel-intake-run.tar.zst)・[DBと公開資料](recovered-panel-intake-databases.tar.zst)

```sh
out=$(mktemp -d)
tar -xf "$p/recovered-panel-intake-run.tar.zst" -C "$out"
tar -xf "$p/recovered-panel-intake-databases.tar.zst" -C "$out"
for slot in 0 1 2; do
    MISAKA_PRIVATE_COLD_PANEL_INTAKE=1 MISAKA_PRIVATE_COLD_PANEL_SEED=1 MISAKA_PRIVATE_COLD_PANEL_DIR="$out/peers/$slot" RUST_MIN_STACK=67108864 "$TEST_BINARY" private_read_cold_panel_receipt --ignored --test-threads=1 --nocapture > "$out/fresh-$slot.log" 2>&1 || break
done
python3 -B "$p/check_recorded_reply_signatures.py" "$out/native.log" --openssl "$(brew --prefix openssl@3)/bin/openssl" --require-intake --cold-readers "$out/fresh-0.log" "$out/fresh-1.log" "$out/fresh-2.log"
```

### 停止位置を予測しない1回推論の費用

署名済みの算術1件と上記3問を、登録済みQwen3-8Bの同じ整数profileで再実行した。1回推論する経路では、入力・commit対象の各tile・各tokenの選択記録を実行順にバッファへ格納する。EOS又は生成上限で実際の回答長が決まった後、正しいleaf数を使ってTRACEを構築する。記録バッファには1 GiBの上限を設けた。モデルと実行器を含むプロセス全体の最大常駐メモリは別に記録した。各経路で元の回答、数値Readout、四Root、Ticketがすべて一致してから計時行を出力する。

同一プロセスで各問の1回推論と従来の2回実行を交互の順に1回ずつ測定した。モデル準備は各問の計時から分けた。仕事別の再現には、前節の計時用planと同じ登録済みartifact・scaleを使う。[問別の実測値](g-cost-online-stop.json)・[計時ログ](g-cost-online-stop-run.jsonl)・[機材・バイナリ・最大常駐メモリ](g-cost-online-stop-environment.json)

```sh
RAYON_NUM_THREADS=3 "$CARGO_TARGET_DIR/debug/examples/qf1-paired-cost" --online-stop "$parent" "$out/scales.section" "$out/varied-plan.json" > "$out/online-run.jsonl"
uv run --offline --no-project --with tokenizers==0.23.3 python -B "$p/check_g_cost_varied_jobs.py" "$MISAKA_PRIVATE_QF1_TOKENIZER" "$p" --online-log "$p/g-cost-online-stop-run.jsonl" | cmp - "$p/g-cost-online-stop.json"
```

### 新規Originalを1回推論で生成する隔離3ピア試験

登録者が署名したclass・descriptor・tokenizerとEXECから導いた入力を取得し、producerは回答長の事前申告なしにQwen3-8Bを実行した。実際の停止位置から四Root・Ticket、Init・Stop・最後のReadout chunk・Endのopeningを構成してOriginalに署名した。Panelは登録済みモデルで個別に完全再実行した。producerのTRACE記録バッファ上限は1 GiBで、ホストのメモリ予約器を使用する場合は実行前の予約額に含める。

正常系では承認がDAA 27で受理され、元の排他期限DAA 60でも参照できた。別の試行で、保持していたStop・EndをCourtが直接検査し、両caseで反証不成立となった。負の対照では、producerがReadoutだけを差し替えて署名すると、Panelが完全承認を出せず、Endの裁定が計算を反証した。各試行のOriginal・PANEL・caseは独立であり、それぞれの結果を[正常系](native-one-pass-accepted.json)・[正規Originalの直接裁定](native-one-pass-court.json)・[矛盾したOriginalの直接裁定](native-one-pass-refuted.json)に収録した。

[実行ログとMOE2の帳簿](native-one-pass-run.tar.zst)には同一バイナリの隔離市場試験も含む。市場生成・永久LP・余剰burn・MinerとPanelへの配分はこの市場試験の決済で確認した。[帳簿の検算](native-one-pass-book.json)。新規Qwen claimのFinal・採掘資格・受賞・決済は未検証である。

```sh
out=$(mktemp -d)
tar -xf "$p/native-one-pass-run.tar.zst" -C "$out"
for name in accepted court refuted; do
    case "$name" in
        accepted) set -- --undisputed ;;
        court) set -- --retained-stop --retained-end ;;
        refuted) set -- --retained-stop --retained-end --inconsistent-readout ;;
    esac
    python3 -B "$p/check_real_lifecycle.py" "$out/$name.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --one-pass-original "$@" | cmp - "$p/native-one-pass-$name.json" || exit 1
done
python3 -B "$p/check_studio_moe2_book.py" "$out/moe2-original-book" | cmp - "$p/native-one-pass-book.json"
```

### 資料不足で元期限に到達したPANELの終端

新しいQwenのOriginalに対し、選出2席のうち1席は登録scaleを取得できず、もう1席は完全再実行に署名した。各ピアは元の排他期限DAA 60に達すると、最初に期限へ到達したブロックのselected parentからPANEL certificateまでの区間を走査した。各ブロックのcoinbaseをnative codecで読み、期限内の完全承認を格納し得る64バイトのcommitmentがないことを確認した。その後のブロックに、元のduty・期限・境界ブロックに束縛した未検証終了markerを格納した。各ピアは公開前と終端側のDBを開き直し、PANELのdrawを復元して同じ終端を照合した。[未検証終了の記録](native-unverified-expiry.json)・[DBと公開資料](native-unverified-expiry-databases.tar.zst)

同じバイナリで両席の承認が期限内に入った別のOriginalでは、承認署名を渡さずに終了を提案しても走査が承認の入ったブロックを特定し、未検証終了を保留した。そのブロックの署名を渡すと期限後も元の承認を検証できた。[期限内承認の対照](native-unverified-expiry-positive.json)

この非報酬経路が対象とするのはPANEL certificate後の選択鎖と、その後の終端markerである。別用途か判別できない64バイトの値が一つでもあれば保留する。履歴を取得できない、走査予算が尽きる、確認中に枝が動く場合も終了を認定しない。資料提供者の責任判定、公開合意の状態遷移、Final・決済は別途必要である。

```sh
out=$(mktemp -d)
tar -xf "$p/native-unverified-expiry-run.tar.zst" -C "$out"
python3 -B "$p/check_real_lifecycle.py" "$out/native.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --missing-material --unverified-expiry --one-pass-original | cmp - "$p/native-unverified-expiry.json"
python3 -B "$p/check_real_lifecycle.py" "$out/positive.log" "$p/qf1-native-tile-artifacts.json" --decode-limit 16 --undisputed --unverified-expiry --one-pass-original | cmp - "$p/native-unverified-expiry-positive.json"
```

初回の試験では待機中にも64バイトの値を格納し、走査が終了を保留した。修正版では次ブロックのDAAを確認し、待機ブロックを空にして再測定した。3ピアのMOE2市場決済も同じバイナリで再照合した。[市場帳簿](native-unverified-expiry-book.json)

### G-USEの隔離算術試験を抽選前に固定する

非報酬の算術2問に専用のMODEL_AUDIT（round 15）と評価PANEL（round 16）を準備した。challengeの先頭2バイトからそれぞれ0〜4の値を得て、1問目は加算、2問目は和から第2項を引く減算とする。HEX64を含む登録済みQF1入力を使う。正解は登録tokenizerの数字1 tokenにQF1 EOS 1 tokenが続く場合で、2問中2問を試験用の基準とした。問題文・演算・採点・最大16出力token・最大32,000,000 leafを固定したsuiteのdigestは `0ae87bea…`。この閾値と用途は隔離試験のもので、公開の採掘基準は未採択である。

登録済みQwen3-8Bのclass・inventory・descriptorとこのsuiteを束縛したclose `8f9e2d10…` を生成し、3つの新規DBで元のREADY署名とブロックを再検証した。DAA 6で全holderが署名・公開を保留した。この記録は抽選前の条件を検証する。旧監査の既知seedからこのsuiteの採点結果を作ることはできない。新しい監査のseed公開、実出力の固定と採点、後続Panelによる独立再実行が残る。[登録・close・各holderの結果](qf1-arithmetic-audit-close.json)・[公開ブロックと4プロセスのログ](qf1-arithmetic-audit-close-public.tar.zst)

```sh
out=$(mktemp -d)
tar -xf "$p/qf1-arithmetic-audit-close-public.tar.zst" -C "$out"
python3 -B "$p/check_fresh_audit.py" "$out" --arithmetic-close | cmp - "$p/qf1-arithmetic-audit-close.json"
```

### 算術試験用の枝で旧規則の仕事を受理

上記のcloseから進めた隔離枝で検証者8席を準備し、DAA 6491で旧規則のQwen2.5-1.5B 8Kのclaim `de2c19ea…` がProvisionalになった。1回の実行で得たticketは `715025e5aa3875b700f4853b2124d35a`。ノードが提示した元の閾値で受理し、同じnativeプロセスが数値記録を再実行した。Qwen3の算術採点は後続のMODEL_AUDIT抽選を必要とする。[受理・ticket・再実行記録](qf1-arithmetic-audit-work.json)・[追加のブロック・採掘記録](qf1-arithmetic-audit-work-public.tar.zst)

上記closeの展開先に追加記録を展開して照合する。対照試験では、展開した `accepted/capture.bin` の先頭1ビットを変更すると、nativeが記録したSHA-256と一致せず検査に失敗した。

```sh
tar -xf "$p/qf1-arithmetic-audit-work-public.tar.zst" -C "$out"
python3 -B "$p/check_fresh_audit.py" "$out" --arithmetic-work | cmp - "$p/qf1-arithmetic-audit-work.json"
```

算術試験と同じバイナリ（SHA-256 `deb86d2b…`）でStudioの登録・市場生成・MOE2配分を3プロセスで再現した。試験用160 MSKから買付48、永久LP32、Miner64、Panel16となり、余剰の1 Positionをburnした。3プロセスとも巻き戻しと再適用で同じ市場・LP・worker権利を復元した。別試験ではholderがnative finalityの撤回を受け、再起動後も公開停止を維持した。[同じビルドの実行ログ](qf1-arithmetic-build-regressions.tar.zst)

FinalとGate Cの公開記録の照合器は、各ブロックの署名を含む全取引からヘッダーの取引Merkle rootを再計算する。既存の8K claim `50f0da2d…` の受付後10,366ブロックで照合し、Panel receiptとGate C attestationの署名を各1ビット変えた対照では、いずれも元ヘッダーとの不一致を検出した。[取引のシリアライズと照合](check_transaction_root.py)

### 算術評価の資格と提出物

算術試験のclose `8f9e2d10…` で、監査round 15と後続Panel round 16のREADY署名を公開鍵から検証した。各roundで3 dealer × 3 holderの9署名が元のround・dealer資料・sessionに一致し、資格rootと算術suiteを含むcloseも再現できた。[両roundの結果](qf1-arithmetic-qualification.json)

```sh
python3 -B "$p/check_fresh_qualification.py" --arithmetic --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/qf1-arithmetic-qualification.json"
```

隔離用の提出処理では、監査seed公開後に各問を1回推論し、出力token列、停止理由、処理位置数、数値Readout、四Rootの件数・digest、Ticketを記録する。2件を出題順に並べてsuiteと元の監査証明書に結び付け、証明書の直後のブロックに格納したdigestを照合する。読み取り側は同じ登録モデルから全件再実行し、保存した全フィールドとブロックの一致を確認する。署名済み監査seedによる実測と後続の選出Panelによる判定は別途記録する。

提出形式の回帰試験では、問題順の逆転を拒否し、証明書・回答・Rootの変更で別のdigestになることを確認した。remoteライブラリの試験は213件成功・1件ignored。提出処理を含むnativeビルド `456ec675…` でも、独立したStudio市場試験の3プロセスとholderのfinality撤回試験を再実行した。[検査ログ](qf1-arithmetic-submission-tests.tar.zst)

### 算術監査の枝で8K claimがFinalに到達

DAA 6491で受理した旧規則のQwen2.5-1.5B 8K claim `de2c19ea…` について、DAA 6511の抽選で選ばれたカード4・6・2がそれぞれ元の数値資料を再実行し、同じtranscriptで承認した。別のnativeプロセスが受付ブロックから285ブロック進め、DAA 6633のブロック `dd51ac1f…` でFinalを記録した。3席の再実行はこのプロセス内で行った。公開アーカイブを展開して全取引とヘッダーを照合し、Panel receipt末尾を1ビット変えた対照では元ヘッダーとの不一致を検出した。[Finalの照合結果](qf1-arithmetic-8k-final.json)・[285ブロックとnativeログ](qf1-arithmetic-8k-final.tar.zst)

```sh
out=$(mktemp -d)
for name in qf1-arithmetic-audit-close-public qf1-arithmetic-audit-work-public qf1-arithmetic-8k-final; do
    tar -xf "$p/$name.tar.zst" -C "$out"
done
python3 -B "$p/check_fresh_final.py" "$out" --arithmetic-work | cmp - "$p/qf1-arithmetic-8k-final.json"
```

### 算術監査の公開資料からの独立照合

前回の監査で実際に署名された3件のreleaseを使い、元のcloseでは3ピアのseedと2件のchallengeを再現した。同じreleaseを今回の算術監査の入力にすると、holderが当初登録したshareのcommitmentとの照合で拒否された。[旧roundの対照結果](qf1-arithmetic-cross-round.json)

算術監査の公開後は `check_arithmetic_submission.py` にslot 0〜2の署名ログ、公開済みrelease、publication、後続の提出ブロック、slot 0・1の再読込ログを置いたディレクトリと登録tokenizerを渡す。入力文・正解digit・HEX64入力列をseedから導き、停止位置、入力・回答・Readoutの各Rootと4件のRootからTicketを再計算する。traceの数値再実行は3者それぞれのnative観測に照合する。提出物のBorsh表現から計算したdigestはnativeテストの固定値とも照合した。この照合の対象は提出時点までである。

### G-COSTの1回推論の再測定予定

既存の4件を逆順に並べ、同じバイナリの同一プロセスで各仕事の1回推論と2回実行を1回ずつ測る。4件が偶数なので、各仕事で先に測る経路も前回と入れ替わる。機材・3スレッド・モデル・元Ticketをそろえ、準備時間と最大常駐メモリを別に記録する。実測前に固定した計画に前回のログをそのまま渡すと、仕事とTicketの順序が異なるため照合に失敗する。G-COSTの公開閾値は未採択である。[再測定の順序とバイナリ](g-cost-online-repeat-schedule.json)

```sh
uv run --offline --no-project --with tokenizers==0.23.3 python -B "$p/check_g_cost_varied_jobs.py" "$MISAKA_PRIVATE_QF1_TOKENIZER" "$p" --reverse-online > "$out/repeat-plan.json"
RAYON_NUM_THREADS=3 "$CARGO_TARGET_DIR/debug/examples/qf1-paired-cost" --online-stop "$parent" "$out/scales.section" "$out/repeat-plan.json" > "$out/repeat-run.jsonl"
uv run --offline --no-project --with tokenizers==0.23.3 python -B "$p/check_g_cost_varied_jobs.py" "$MISAKA_PRIVATE_QF1_TOKENIZER" "$p" --reverse-online --online-log "$out/repeat-run.jsonl"
```

算術監査枝のFinalを新しいDBで再生した時点ではGate Cの抽選・承認は0件だった。カード5の割当46,953,160,500,520 sompiが反映され、DAA 7932で1,021,082 microblocksを使用可能になった。同じDAAで実行ブロック `81a50221…` を起点に、元claim `de2c19ea…` のGate Cが75枠を抽選した。期限はDAA 8793。Finalからの2,600ブロックの取引とヘッダー、割当のbond、実行ブロックから導いたanchor claimを照合した。抽選記録のanchor claimだけを書き換えた対照は拒否した。[Gate C抽選の照合結果](qf1-arithmetic-8k-draw.json)・[抽選までのブロックと実行ログ](qf1-arithmetic-8k-draw.tar.zst)

上記のFinalまでのアーカイブを展開したディレクトリで抽選を照合できる。

```sh
tar -xf "$p/qf1-arithmetic-8k-draw.tar.zst" -C "$out"
python3 -B "$p/check_fresh_certificate.py" "$out" "$out/accepted/winning.json" --arithmetic-work --draw-only | cmp - "$p/qf1-arithmetic-8k-draw.json"
```

上のコマンドに `--openssl "$(brew --prefix openssl@3)/bin/openssl"` を加えると、元ブロックの各Panel署名について、network・claim・Valid判定・署名DAAからメッセージを再構成してML-DSA-87を検証する。算術監査枝のカード4・6・2で成功した。カード4の署名について署名の1ビット、カード、メッセージの1ビット、署名contextを個別に置換すると、4件すべてで検証に失敗した。Gate Cではネットワーク・bond・元のattestationバイト列を使用する。以前の認証済み8K枝では、署名付き7取引も同じOpenSSLで検証できた。[隔離用カード公開鍵8件](isolated-card-public-keys.bin)は `TestConsensus::palw_v2_registry_keypair` のカード番号順で、1鍵2,592バイト。実際の算術監査枝のGate C認証は `--arithmetic-work --writer-only --openssl` とOpenSSLのパスで生成ノードから、続いて `--writer-only` を外して2台の再読込ノードと照合する。

算術提出物の読込では、監査証明書を選択親とする提出ブロックを先に受理し、元の期限、後続Panelの締切blue score、選択鎖上の位置、提出digestを検査する。生成側は停止位置を推論しながら記録を保持する。読込側は提出された回答長でMerkle木を準備し、登録モデルと抽選された入力から全演算を再実行して記録を順次処理する。回答・停止理由・位置数・Readout・四Root・Ticketの全フィールドを元の提出物と照合し、各問の実行後も監査challengeと提出ブロックの有効性を確認する。

この読込処理を含むビルド `5aea69b7…` で、remote試験213件成功・1件ignored、別途Studioの3プロセス市場試験とholder撤回試験が成功した。実際の算術roundの読み取り結果はseed公開後に記録する。[ビルドの回帰試験](qf1-arithmetic-stream-replay-tests.tar.zst)

同じ算術提出物読込ビルドで、隔離した3プロセスから決済台帳（1,789バイト）と切断時の復元記録（1,219バイト）を取得し、それぞれのバイト列の一致を確認した。追加モデル登録で開いた市場の元の価格曲線から、予約額16,000,000,000 sompiのうち購入4,800,000,000、永久LP 3,200,000,000、miner 6,400,000,000、Panel 1,600,000,000を再計算した。購入2単位のうち1単位が永久LPに入り、余剰1単位がburnされた。決済と復元記録の先頭1ビットを個別に変えた場合、照合は両方失敗した。これはStudio市場の隔離試験用claimによる決済で、Qwen3の算術監査は別の非報酬実行である。[照合結果](qf1-arithmetic-stream-moe2-book.json)・[元の台帳・復元記録・実行ログ](qf1-arithmetic-stream-moe2-book.tar.zst)

算術監査枝の8K claim `de2c19ea…` は生成ノードでDAA 11673にGate C認証を得た。DAA 7932の抽選から認証先端 `0225310e…` までの7,481ブロックについて親・取引Rootを再計算し、カード0・2・3・4・5・6・7の8K再実行結果、各bondに結び付くML-DSA-87署名、75枠の賛成を照合した。これは算術監査枝を進める8K仕事の認証である。Qwen3-8Bの算術評価と採掘適格性には別の検査が残る。[生成ノードの認証結果](qf1-arithmetic-8k-certificate-writer.json)・[認証までのブロック・実行ログ](qf1-arithmetic-8k-certificate-writer.tar.zst)

抽選までの記録を展開したディレクトリで生成ノードの認証記録を再検証する。

```sh
tar -xf "$p/qf1-arithmetic-8k-certificate-writer.tar.zst" -C "$out"
python3 -B "$p/check_fresh_certificate.py" "$out" "$out/accepted/winning.json" --arithmetic-work --writer-only --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/qf1-arithmetic-8k-certificate-writer.json"
```

Qwen3-8Bで同じ4問の実行順と各問の経路順を反転して再測定した。正規Ticketを再現する1回推論は初回169.752秒、再測定170.055秒。2回推論はそれぞれ323.475秒、323.631秒だった。モデル準備は別途26.755秒と26.002秒。実行順を変えても4問すべてで同じTicketを得た。後続の費用比較で2回推論だけを基準にすると、この機材では1回推論で実際に生成できる費用を過大評価する。今回の比較は同一機材・同じ4入力・各経路2測定で、採択済み費用閾値の判定はなく、G-COSTはUNKNOWN。順序指定の変更、欠測、Ticketの1ビット変更、経路の入れ替えを個別に拒否した。[2回の問別比較](g-cost-online-repeat.json)・[再測定ログ](g-cost-online-repeat-run.jsonl)・[資源使用記録](g-cost-online-repeat-resources.log)

```sh
uv run --offline --no-project --with tokenizers==0.23.3 python -B "$p/check_g_cost_varied_jobs.py" "$MISAKA_PRIVATE_QF1_TOKENIZER" "$p" --online-repeat | cmp - "$p/g-cost-online-repeat.json"
```

8K認証の生成ノードを終了した後、同じ実行ファイルを使う別プロセス（PID 38620）が元のブロック列を再読込した。認証DAA 11673、75枠の賛成、再実行結果、先端 `0225310e…` が生成側と一致し、frontier は元の受付ブロック `3f2aca47…`（blue score 12984）を指した。認証時の監査seed公開は保留で、readinessには未成熟の仕事1件と最早成熟DAA 17931が記録された。frontierの高さ、readinessが返す先端のblue scoreを個別に変更した記録は拒否された。[1台目の再読込結果](qf1-arithmetic-8k-certification-reader-0.json)・[完走ログ](qf1-arithmetic-8k-certification-reader-0.tar.zst)

生成側の認証まで展開したディレクトリで再現できる。

```sh
tar -xf "$p/qf1-arithmetic-8k-certification-reader-0.tar.zst" -C "$out"
python3 -B "$p/check_fresh_certificate.py" "$out" "$out/accepted/winning.json" --arithmetic-work --first-reader --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/qf1-arithmetic-8k-certification-reader-0.json"
```

3台目（PID 39253）も元のブロック列から同じ8K認証を復元した。2台の再読込結果はclaim、再実行結果、frontier、認証DAA 11673、75枠の賛成、認証時の監査状態で一致した。両者が計算した認証後の状態root `15417af4…` は、3台目が生成した直後のブロック `b18b5444…`（blue score 23351）の親状態commitmentとも一致した。子ブロックの親、親状態root、取引Rootを個別に置換した対照はすべて拒否された。認証時には3台目でも監査公開は保留だった。保存したログはその時点までの抜粋である。[3プロセスの照合結果](qf1-arithmetic-8k-certified-parent.json)・[子ブロックと3台目のログ](qf1-arithmetic-8k-certified-parent.tar.zst)

1台目の再読込記録まで展開したディレクトリで照合する。

```sh
tar -xf "$p/qf1-arithmetic-8k-certified-parent.tar.zst" -C "$out"
python3 -B "$p/check_fresh_certificate.py" "$out" "$out/accepted/winning.json" --arithmetic-work --openssl "$(brew --prefix openssl@3)/bin/openssl" --certified-successor "$out/first-child.json" | cmp - "$p/qf1-arithmetic-8k-certified-parent.json"
```

算術監査のclose `8f9e2d10…`（blue score 5）は、認証先端から52,171ブロック後のDAA 37,758で、holderが署名できる確定範囲に入った。52,170ブロック目（blue score 75,520）ではpruning pointがcloseを覆っていても、確定状態は空で公開は保留だった。次のブロック（blue score 75,521）で決済snapshotのfinalizedが `bf4d6fc1…` となり、ノードがreadyを返した。元期限はDAA 90,641。genesisから認証先端までの23,350ブロックと後続52,171ブロックの親・取引Rootを再計算し、finalizedが元closeの子孫であることを確認した。この記録は公開条件の成立までを扱う。実際のseed・算術結果はholderの署名と再実行から別に確認する。[公開条件の遷移](qf1-arithmetic-release-window.json)・[認証後の全ブロック](qf1-arithmetic-release-window-blocks.jsonl.zst)・[独立2台の認証照合](qf1-arithmetic-8k-certification-readers.json)・[完走ログ](qf1-arithmetic-8k-certification-readers.tar.zst)

生成ノードの認証まで展開したディレクトリで、独立2台の結果と公開条件を照合する。

```sh
tar -xf "$p/qf1-arithmetic-8k-certification-readers.tar.zst" -C "$out"
python3 -B "$p/check_fresh_certificate.py" "$out" "$out/accepted/winning.json" --arithmetic-work --openssl "$(brew --prefix openssl@3)/bin/openssl" | cmp - "$p/qf1-arithmetic-8k-certification-readers.json"
zstd -dc "$p/qf1-arithmetic-release-window-blocks.jsonl.zst" > "$out/after-cert.jsonl"
python3 -B "$p/check_arithmetic_release_window.py" "$out" "$out/after-cert.jsonl" "$out/read-cert-2.log" | cmp - "$p/qf1-arithmetic-release-window.json"
```
