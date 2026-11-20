# Studio claim の隔離 T12 観測

**この Studio claim は落選した。** 3ピアとも同じ受付に対して Final と Gate C 認証に達し、旧規則の当選数は 0/21 だった。使用期限までの target 上限でも当選しない。この claim の受賞ブロックと支払は発生していない。処理時点ごとに3ピアのビルドを揃え、途中で実装を更新して同じ台帳から再開した。

2026-11-20、`codex/public-one-honest-vertical` の `3f117d246` から作った Linux ノードで、登録済み DAA 317 のチェックポイントを再開した。3 ピアは同一ホスト上の独立プロセスで、専用 genesis と履歴時計を使った。ノード実行ファイルの SHA-256 は `a954b1ae693a3ea4ed9a337d61c6d68d43c0679a9cf25ae475956f1c6bed621b`。追加統合した AN1・CAD1・CPN1・SWC1 は未採択案として保持し、この実行には既存の protected TFO3 受付を用いた。

## 受付

- Studio が登録済みモデルを検出し、prompt の実行で 85,938 work leaves を記録した。[Studio 応答](studio-run.json)
- Studio job と署名済み rail 取引を claim ID、output root、実行 root で対応づけた。`studio-evidence.json` は照合に使う項目の抜粋。[Studio と rail の対応](studio-evidence.json)
- ノードは carrier 取引 `3a989cfc…a54f08de` を、ブロック `62182f1b…66303ebd` から選択鎖ブロック `c643963c…bb93873` の ordinal 1 で受理した。DAA 318、blue score 638。受付時点の phase は `provisional`、Gate C は `pending`、21 量子の消費数は 0。完全な識別子、両ヘッダー、取引本体は RPC 観測に記録した。[受付観測](original.json)

## Panel 承認

DAA 323 で抽選された seat 7 が、実行者の seat 0 とは別のピアで 85,938 leaves を全再実行し、元の実行 root と回答の一致を確認した。Panel は署名済み取引 `50fbc448…e43498bc` を送信した。[Panel の再実行ログ](panel-worker.log)

この取引は carrier block `00d7bd33…8dc115` に入り、選択鎖ブロック `db95051c…ecccef` の ordinal 1、DAA 325 で受理された。元の受付と claim ID・受付ブロック・元実行 ID が一致し、ネイティブ状態の `fullReplayApprovalRecorded` は `true`。この観測時点で phase は `receipt_licensed`、Gate C は `pending`、消費済み量子は 0。通知期限は DAA 3438、異議申立て延長後の最終期限は DAA 10318。[Panel 承認の包含と状態](panel-approval.json)

## 適用した RFC

この claim は RFC-0054 の protected TFO3 として受理された。以後の Panel、異議、Final、Gate C と receipt quantum は受付時の識別子と期限で追跡する。RFC-0054 は同一 claim の権利消費と対応ブロック受理までを通し確認に含める。[RFC-0054](../../../rfc/0054-public-t12-original-admission-profile.md)

追加マージの CAD1 ゲートは既存 TFO3 に新しい受付 Ticket を要求できる構造だった。また SWC1 用の決済変換は既存の work identity の先頭 32 byte を OriginalId として使っていた。RFC-0058 §2.4 の OriginalId は専用の入力から導出する。両者の native 接続を追加マージ前の実装に戻した。AN1 と MOR1 の規範は新しい family／profile に適用する。分離した草案モジュールはこの実行の合意経路に入らない。[RFC-0058](../../../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)・[RFC-0059 §0](../../../rfc/0059-palw-an1-input-binding-and-panel-court.md)・[RFC-0060 §0](../../../rfc/0060-model-registration-and-attempt-output-binding.md)

## 回帰確認

追加マージ後の `kaspa-consensus-core --lib` は 5,946 件成功、126 件失敗、22 件 ignore。失敗した126件を追加マージ直前の `f721bc681` で個別に再実行し、全件で失敗を再現した。公開設定・DAA・認証を対象にした選択テスト205件は成功した。

受付済みの TFO3 から異議申立て側が PublicDA の入力を取得する修正（`875d1f6aa`）は、上記の実取引の公開 prompt ID 17 個を使って検査した。対象の Rust テスト8件と、同じ claim の再開・観測を含む Python テスト54件が成功した。関連する履歴読み取りテスト全15件では1件が DAA 100 の状態再読込比較で失敗し、この失敗も修正前のバイナリで再現した。

PublicDA 入力をメタデータだけで先読みした場合に、検証用の空き枠で受付済み carrier の本文を再取得する修正（`d16e45988`）を確認した。公開異議の Rust テスト135件が成功した。さらに、実際の受付済み TFO3 の prompt ID 17個と登録時の artifact root に一致するモデルを使い、ローカルで1トークンを再実行した。署名済み job と TIR ヘッダーの job pin が一致し、実行・trace・出力の3つの root がすべて一致した。Panel の署名と包含点は上の「Panel 承認」に記録している。[ローカル再実行出力](accepted-original-local-replay.log)

## 同一 claim の再開

3 ピアを停止して各 DB を保存し、異議申立て側の公開入力読み取りを修正したノードで各自の DB を再開した。各ピアの RPC で元の受付の全識別子と、抽選された seat 7 の承認取引・包含点を照合した。DAA 374 で3ピアとも `receipt_licensed`、Gate C は `pending`、消費済みは 0/21。未応答の DA session は 0 件、存続している公開異議の予約は 7 件だった。[再開後のピア別観測](restart-peers.json)

## Panel 抽選前の未使用予約の返上

全再実行を承認した seat 7 は、抽選前に公開異議を予約していた。この予約で開いた session と参加した lane はともに0件だった。再開後、seat 7 の worker が自分の鍵で通常の `DisputeReleasedV1` を送信した。3ピアとも選択鎖上の同一ブロックで、取引 `75f3a31d…f59bbd1` の受理を DAA 421、ordinal 6 に確認した。seat 7 の予約は閉じ、預託は `dismissedHeld` に1件残った。資料要求を行ったほかの6件は継続している。この時点で同じ claim の phase は `receipt_licensed`、Gate C は `pending`、消費済みは 0/21。[3ピアでの返上取引と異議状態](seat-reservation-release.json)

## 公開回答を確認した6人の予約返上

同じ3台の台帳をローカル隔離環境に移した。停止中の台帳、Studio の元取引、実行者の保持資料を移行前後で照合し、3ピアで DAA 468 の受付点と Panel 承認を読み戻した。

公開異議を予約した seat 1〜6 は、それぞれ同じ claim を自分の TIR バックエンドで再実行した。回答済みの row 0・tile 0 から出力トークン `26226` を復元し、元の入力、実行・logits・出力の各 root、停止条件、事前登録された tokenizer の rendering と EOG を照合した。各 seat が署名した通常の `DisputeReleasedV1` 6件を3ピアが同じ包含点で受理した。5件は DAA 472、seat 4 の1件は DAA 473。DAA 474 で生存予約は0件、預託を保持した返上は先の seat 7 を含め7件。phase は `receipt_licensed`、Gate C は `pending`、消費は 0/21。各ピアは同一ホスト上で動作した。[6取引の所有者・取引ID・包含点・各ピアの読み戻し](answered-reservation-releases.json)

## 同じ受付の Final

DAA 3438 の通知期限を通過し、DAA 3439 で3ピアが claim `9d08847f…26ccb68c` の `final` を返した。元の DAA 318 の受付と DAA 325 の抽選 Panel 承認は同じ選択鎖上にある。Final の観測時点では Gate C は `pending`、消費済みは 0/21 だった。[Final と受付・承認の照合](final.json)

この区間では隔離時計がノードの提示した次の heartbeat 時刻を3ピアで確認した。ローカル GPU が探索した algo 8 の nonce はノードが通常の PoW 規則で検査した。DAA 747 における再開時は、履歴差分の再計算に使ったメモリ予約のうち、完了後の状態が占める分を残して一時分を解放した。生成ピアが既に保持していたブロックを通常の RPC で再提出し、3ピアの選択鎖が揃ってから観測を継続した。

## Gate C、旧規則での抽選と Court の回答

7つの抽選鍵の承認と公開異議期間の経過を受け、DAA 3509 で3ピアが同じ claim の `SecurityCertified` を返した。7鍵の担当位置は合計75、閾値は41だった。認証は抽選された stake の正直割合を前提とする。ピアと鍵はいずれも同一ホストの隔離試験に属する。[Gate C の状態](gate-c-certification.json)

認証後40 DAA の抽選位置は3549。旧規則が選ぶ最初の attempt ブロックは DAA 3553 の `06c279d2…aed2b3a0d3` だった。DAA 3559 のネイティブ状態で21枚の ticket を再計算すると、最小値は index 9 の `18566322049877804318042812783364979989`、有効な target は `2593558205736391250396941624356751` で、当選数は0。3ピアとも同じ固定ブロックから同一バイト列を返し、その後も DAA 3575・3577・3578 で消費は 0/21 だった。使用期限は DAA 4153。この間の再調整は DAA 4000 の1回で、最大4倍に緩和しても有効 target の上限は `10374232822945565001587766497427006`。最小 ticket より小さいので、この beacon を保持する選択鎖上では期限まで当選しない。[抽選値と各ピアの読み取り](draw-and-court.json)

生成を止め、3台それぞれの保存済みデータベースを同一バイナリで開き直した。3ピアとも同じ DAA 3616 の先端に復帰し、元の claim、Final、Gate C 認証、抽選ブロック、未消費 21/21 を読み戻した。[再起動後の3ピアの状態](draw-restart.json)

同じ claim の row 0・tile 0 に対する公開回答を3ピアの取引本体と照合した。登録済みの開示者の鍵で署名を検証し、登録された TIR program を補って元の trace root と execution root に対する局所開示を検証できた。各 root を一つずつゼロに置換すると同じ検査がそれぞれ拒否した。[署名済み回答](court-answer.bin)・[固定したチェーン状態](draw-state.borsh.zst)

リポジトリのルートから次のコマンドで読み取りを再検査できる。

```sh
evidence=docs/evidence/palw/studio-winning-receipt-2026-11-20
state=$(mktemp)
zstd -dc "$evidence/draw-state.borsh.zst" > "$state"
cargo run --locked -p kaspa-consensus-core --example protected_studio_draw -- \
  "$evidence/draw-and-court.json" "$state" "$evidence/court-answer.bin"
```

## 資料遮断と再起動の回帰試験

ネイティブ Court のテストで、署名者が要求された chunk を最後まで提供しなかった場合は claim が無効になり、異議を出した seat の預託が回復した。旧経路で再起動すると実行 cohort の内部インデックスの初期状態が通常のブロック適用時と食い違っていた。非稼働期間の復元を直し、資料遮断ケースを再実行して成功した。別途、実行記録からの TIR 回答、二重消費の拒否、異議中の受賞権利の停止、reorg 時の Court 復元を検査した。これらはローカルのテストで、上記の Studio 3ピアでは実際に取得した署名済みの公開回答と保存済み DB を検証した。[実行したケースと結果](native-regressions.log)

## 新版の構成要素を単体で確認した範囲

MOE2 ではネイティブの登録ブロックに含まれる2クラスのうち、追加登録モデルだけに市場が開いた。1件の登録 root が一致しないと同じバッチをすべて棄却する。報酬 1,000 の RFC 例では、購入に 300、永久 LP に 200 を充て、残り 500 を報酬として記帳した。購入した 2,307 Position のうち 1,183 を LP に戻し、1,124 を burn した。支払・公開有効化は行っていない。

TB1 の有限体検査では、固定した全 qualified dealer に対して同じ凍結 roster の相異なる支持点を検査した。dealer の欠落・重複・差替えを拒否し、到着順を変えても定数を凍結順に取り出せた。MOE2 と TB1 のネイティブテスト計8件が成功した。新しい FOW1 claim の本番 checker、TB1 の VSS 通信からの統合、および3ピアでの新版決済は未検証。[テスト名と結果](native-regressions.log)

## 別の当選 claim による受賞ブロック生成

同じ日に T‑N1 の既存ローカル試験で、producer service が別の TFP1 claim `e6770642…f8934791` を投入した。DAA 1733 で署名が定足数に達し、Final 後の DAA 2725 に実行済み attempt が抽選 beacon を供給した。index 0 は当選し、使用期限は DAA 3325 だった。通常のテンプレートに署名済み PFS3 を載せて受賞ブロック `7cb8b072…56e5a147` を生成した。直接の子 `64525d97…457388fd` が生成者へ 320,084,650,080 sompi を割り当てた。生成側と、新しい DB に genesis から全ブロックを入れた2つの reader が、同じ index の消費と子の coinbase UTXO を検証した。初回のこの3台帳は1つのテストプロセス内にあった。[当選と3台帳の実行ログ](independent-tn1-receipt.log)

この試験は T‑N1 の固定 genesis、試験用の期間・価格、PoW を省略するネイティブテスト設定を使う。Studio が追加した冒頭の TFO3 claim とは識別子も規則も異なる。

同じ当選履歴の5,473ブロックを RPC の raw block 形式で保存し、同じバイナリを使う3つの独立プロセスで読み込んだ。各プロセスは新しい DB と試験用 genesis の UTXO から開始し、全ブロックを通常のネイティブ検査に通した。受付 DAA 1726、Final DAA 2325、受賞 DAA 2725、先端、消費 index、子の coinbase の outpoint と金額が全て一致した。この履歴は Gate C 適用前の条件に属する。先端 DAA 2726 では子の支払 UTXO が存在し、子に対する別の決済確定照会は3台とも `false` だった。ブロック配送には保存したストリームを用いた。[3プロセスの照合結果](independent-tn1-readers.json)

通常の生成試験は、新しい履歴を作って2つの子プロセスで検査する。保存済みの同じ当選履歴を再検査する場合は、ローカルの `misaka-tn1-winning-receipt-20261120.jsonl.zst` を使う。圧縮ファイルの SHA-256 は `68f482b493efa458b56de0eb965bd9aabcdea14564d877976f4457bff52af983`。次の例では `test_bin` に `cargo test --locked -p kaspad --lib --no-run` が表示する実行ファイルを、`archive` に圧縮ファイルのパスを指定する。

```sh
case_name=palw_panel::palw_service_public_e2e::tir_frame::tir_frame_node_tn1_default_semantics_spends_an_actual_honest_quantum
result_dir=$(mktemp -d)
for peer in 1 2 3; do
    zstd -dc "$archive" | KASPA_TEST_CLOCK_ISOLATED=1 MISAKA_TN1_REPLAY_RESULT="$result_dir/$peer.json" \
        "$test_bin" "$case_name" --exact --test-threads=1 || exit 1
done
cmp "$result_dir/1.json" "$result_dir/2.json" && cmp "$result_dir/1.json" "$result_dir/3.json"
```
