# MISAKA-Studio main から生成した TFO3 claim

2026-11-20 に MISAKA-Studio の `main`（`c9257516b23b14527ef9cef06af143756a422e8f`）を未変更のソースからビルドした。チャット画面と共通の `POST /api/v1/network/mining-queue` に “Continue the count: three, four,” を投入した。隔離 testnet-12 の登録済み TIR クラスで17入力トークン、1出力トークン、85,938 leaves を実行した。Studio の queue、gateway の receipt、rail による Borsh の再読は同じ claim `05ba473f…e1c0cce9` を返した。[入力・実行 root・ファイルハッシュ](commitment.json)

| 段階 | 観測 |
| --- | --- |
| 出力と証拠の配置 | gateway が証拠 24,621,472 バイトを24チャンクに分け、隔離 provider から読み戻した。 |
| protected quote | 3ピアが DAA 3617 で同じ Borsh を返した。[3ピアのハッシュ](protected-quote.json) |
| 初回の提出 | rail が元の claim と実行 root に署名した。取引 `d964e427…721d4b6` は選択鎖で受理された。再起動後の3ピアはいずれも、この取引による claim の受付を `found=false` と報告した。[取引の受理と claim の照会結果](carrier-acceptance.json) |
| 初回の受付拒否 | 実行者 bond のクラスは21ブロック分の枠を必要とした。epoch 4 の最終 DAA 3679 でも配分比率から得られる上限は13.61ブロック分だった。[ノードの拒否理由](allocation-refusal.log) |
| 資本割当 | 実行者が隔離チェーン上で、既存 bond の 10,000,000,000 sompi をこのクラスに割り当てた。3ピアとも sequence 1 を epoch 4 の pending として記録した。[署名済み取引の ID と3ピアでの照会結果](capital-assignment.json) |
| 同じ claim の受付 | epoch 6 の DAA 4429 で必要な21,000,000単位に対して21,142,278単位が貯まった。再提出した取引 `dcb4a75f…22c7b175` をブロック `99ea4ea7…9a7718166` が受理し、3ピアとも同じ claim、execution root、受付 DAA を返した。[配分枠と3ピアの受付記録](claim-admission.json) |
| Panel 検証 | 選出 seat 1 の再実行承認を DAA 4444 の選択鎖で確認した。3ピアの `fullReplayApprovalRecorded` が true となり、同じ claim が `receipt_licensed` に移った。異議申立期間は DAA 7549 まで。[承認取引と3ピアの状態](panel-approval.json) |
| 独立再実行 | 受付取引の TFO3 本文と登録済み artifact を用いて、17 入力から生成した1トークンの execution・trace・output root を再計算し、3つとも一致した。Studio の照合器は、同じ記録に対する4種類の ID・root の差替えを拒否した。[再実行ログ](accepted-original-replay.log)・[入力ハッシュと拒否理由](replay-identity.json)・[受付取引の本文](accepted-original.bin) |

別入力 “Hi” も Studio main の queue で実行した。入力は10トークン、価格は同じ21 quanta だった。この比較用 claim の処理は outbox までで、上表はすべて元の claim を追跡している。[比較用の実行記録](short-prompt-price.json)

保存した commitment は、次のコマンドでオフラインで再読できる。引数はこのディレクトリからの相対パス。

```sh
misaka-palw-fp-rail --artifact ./fp-job-5853ff54e729b614 --print-claim
```

隔離 VM の再起動後、3ピアで同じ bond・金額・sequence の割当を再読した。3ピアに共通して残っていた直後の子ブロックを通常の `submitBlock` で各ピアに再提出すると、3ピアすべてで選択鎖に戻り、以降のブロック生成も再開した。[再起動後の子ブロックの照会結果](retained-child-after-restart.json)

見積もり RPC は、同じ epoch の bond 指定時に、受付側と共通の関数でクラスの残り配分枠を照合する。テストで空の初期配分、必要量ちょうど、枠不足、epoch 切替を確認した。Rust の bond budget 91件、TB1 3件、MOE2 5件、Python の protected drill 66件、Studio main の mining queue 5件が通った。ノード側は `cargo check --locked -p kaspa-consensus --lib` も通った。

再編成（reorg）を対象とする Rust テスト59件が通った。現在の形式は native 行を cold restore してから巻き戻し、旧形式は当時の writer が保存した liability frame を照合した。Court の検査には直前のチェーン位置を持つ fixture を使った。[対象と結果](targeted-tests.json)

DAA 4689 で同じ親の子ブロック2件が全ピアに到着し、各ピアの選択先が分かれた。両方を通常のブロック検証に再提出した後、3ピアは同じ先端に収束し、元の claim の受付・Panel 承認を読み戻せた。[分岐前後の3ピアと6件の再検証応答](sibling-resolution.json)

異議申立期間中の DAA 4859 で隔離VMを正常停止し、検証用ディスクの上限を42 GiBから54 GiBに拡張した。同じノードバイナリと3つのデータベースで再開し、DAA 4864 に全ピアが元の受付ブロック、Panel 承認と未消費の権利を返した。[再起動後の3ピアの照会](notice-window-restart.json)

DAA 5213 の状態を同じブロックに固定して再読した。3ピアとも7,147,447バイトが一致し、Panel 承認済みの claim は `receipt_licensed`、Gate C は `pending` だった。抽選の照合器はこの状態を `claim is not Final` で終了した。当落はまだ確定していない。[3ピアの固定ブロックと照会結果](before-final-capture.json)・[固定した状態（Borsh、zstd）](before-final-state.borsh.zst)

この検査はリポジトリのルートから次のコマンドで再現できる。照合器は当該 claim の Final、Gate C の行、異議申立中の時計停止を照合し、成熟後に各 ticket と target を計算する。Court への申立てがない claim にも使える。 保存済みの旧抽選では、従来の署名付き Court 応答の照合と全21枚の ticket 再現も通った。同じ資料の network domain、価格基準クラス、成熟期間、親ブロックの DAA、単一ピアの attempt 列をそれぞれ差し替えると、照合器は全5件を拒否した。

```sh
d=docs/evidence/palw/studio-main-claim-2026-11-20
state=$(mktemp)
zstd -dc "$d/before-final-state.borsh.zst" > "$state"
cargo run --locked -p kaspa-consensus-core --example protected_studio_draw -- --draw "$d/before-final-capture.json" "$state"
```

### Studio main の提出済みジョブから再開する

Studio のキューで追加したジョブは、起動中の Studio main の queue ID を指定して直接追跡できる。`--config` の `studio.port` をその Studio のループバック HTTP ポートに合わせ、gateway と署名済み rail のファイルがある outbox と、提出直前の選択先端を渡す。

```sh
python3 scripts/drills/protected_private_registration_run.py \
  --config "$CONFIG" --output "$FRESH_OUTPUT" --resume-studio "$REGISTERED_RUN" \
  --studio-main-job "$QUEUE_ID" "$OUTBOX" "$SCAN_FROM_BLOCK"
```

drill は指定 ID が `committed` になるまで待ち、該当する gateway ファイルと `.rail.json` を読み、署名済み取引から先を追跡する。Studio main の実バイナリで保存済みの queue ID `1a1f8c8c241c3fc5588` を再読し、この経路が DAA 4429 に受け付けた同じ取引 `dcb4a75f…` に結び付くことを確認した。[queue ID・取引・受付ブロックの照合](queue-id-readback.json)

隔離3ピアを起動する `scripts/drills/protected_private_registration_run.py` に `--studio-queue-submission /absolute/path/to/submission.json` を渡すと、外部で起動した Studio main、gateway、rail の結果を受け取れる。通常の `--config` と `--output` に対応し、登録済みの隔離チェーンなら `--resume-studio` も使える。ジョブを送信した側で、次の4フィールドを持つ JSON を rail の提出後に rename で配置する。

| フィールド | 入力 |
| --- | --- |
| `studioRun` | Studio の `GET /api/v1/network/mining-queue` の `jobs` 配列にある対象行（`status=committed`） |
| `outbox` | その gateway の `fp-job-*.json` と rail の `retention/` を含む絶対ディレクトリ |
| `rail` | 同じ job で実行した `misaka-palw-fp-rail --submit` の JSON 出力 |
| `scanFromBlock` | rail が取引を提出する直前の、同じ隔離チェーンの sink hash |

drill は queue の job ID から gateway のファイルを開き、claim、executor bond、output root、execution root、network domain と rail の署名済み取引 ID を照合する。受付ブロック以降は従来と同じ Panel・Final・Gate C・受賞ブロックの観測を使用する。既に受付記録のある実行には `--resume-claim` を使用する。
