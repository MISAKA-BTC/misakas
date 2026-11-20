# MISAKA-Studio main から生成した TFO3 claim

**元の Studio claim は落選した。** Final、Gate C 認証を経て、DAA 7730 の beacon で21枚の ticket が確定した。使用期限 DAA 8330 までの最大 target も全 ticket が上回った。3ピアとも消費数は0で、この claim の受賞ブロックと決済は発生しなかった。[最終結果](result.json)

抽選後に隔離3ピアを停止し、同じノードバイナリで各ピアの保存済みDBを開いた。3ピアとも DAA 7745 の同じ先端で元の claim に Final、Gate C 認証済み、21枚中の消費数0を返した。抽選時に固定したブロックの状態7,631,437バイトを各ピアから再取得し、停止前と同じ SHA-256 を確認した。[再起動後の3ピアの記録](post-draw-restart.json)

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
| 予約の返上 | 資料要求を開いていなかった6件について、各所有者が元の署名済み取引・prompt・実行・出力・停止条件・tokenizer を照合して返上した。DAA 7607、7613、7614 の返上取引と包含順が隔離3ピアで一致した。3件の再送も選択鎖に入ったが、返上済み件数は増えていない。[所有者・受付取引・前後の状態](unused-reservation-releases.json) |
| Final | DAA 7615 で3ピアが同じ claim の Final を返した。未処理の予約と Court はともに0、認証は pending、21 quanta の消費数は0だった。[Final のノード応答](final.json) |
| Gate C | DAA 7685 で3ピアが同じ元取引について `certified` を返した。抽選された7つの bond が合計75の抽選位置に対して再実行結果を提出し、DAA 7623 に承認が記録された。認証時点の消費数は0。[認証](gate-c-certification.json)・[3ピアの照会](gate-c-peers.json)・[検証者の記録](gate-c-worker.log) |
| 当落 | 認証後40 DAA の slot 7725 以降で最初の attempt は DAA 7730 の `6e09a6dc…fd01974`。3ピアが同じブロックで同一の状態 7,631,437 バイトを返した。全21枚が通常 target と期限内の上限を上回った。[全 ticket・検証者別の判定](draw.json)・[3ピアの照会](draw-capture.json)・[再検算用の状態](draw-state.borsh.zst) |

別入力 “Hi” も Studio main の queue で実行した。入力は10トークン、価格は同じ21 quanta だった。この比較用 claim の処理は outbox までで、上表はすべて元の claim を追跡している。[比較用の実行記録](short-prompt-price.json)

保存した commitment は、次のコマンドでオフラインで再読できる。引数はこのディレクトリからの相対パス。

```sh
misaka-palw-fp-rail --artifact ./fp-job-5853ff54e729b614 --print-claim
```

隔離 VM の再起動後、3ピアで同じ bond・金額・sequence の割当を再読した。3ピアに共通して残っていた直後の子ブロックを通常の `submitBlock` で各ピアに再提出すると、3ピアすべてで選択鎖に戻り、以降のブロック生成も再開した。[再起動後の子ブロックの照会結果](retained-child-after-restart.json)

見積もり RPC は、同じ epoch の bond 指定時に、受付側と共通の関数でクラスの残り配分枠を照合する。テストで空の初期配分、必要量ちょうど、枠不足、epoch 切替を確認した。Rust の bond budget 91件、TB1 5件、MOE2 5件、Python の protected drill 68件、Studio main の mining queue 5件が通った。ノード側は `cargo check --locked -p kaspa-consensus --lib` も通った。

再編成（reorg）を対象とする Rust テスト59件が通った。現在の形式は native 行を cold restore してから巻き戻し、旧形式は当時の writer が保存した liability frame を照合した。Court の検査には直前のチェーン位置を持つ fixture を使った。[対象と結果](targeted-tests.json)

RFC-0061 の TB1 については、公開済みの share から dealer 多項式を再構成するライブラリを個別に検証した。7名中2名までの欠落・誤値の全99通りで、係数と整合する slot が一致した。7・16・64名の独立した Python 入力でも同じ結果を得た。この Studio claim の抽選には受付時の TFO3 規則を適用する。[試験件数と入力条件](targeted-tests.json)

TB1 の全 dealer をまとめる検査では、各 holder の公開行を RoundRef・QualRoot・FreezeId と照合し、証明書が参照した行から該当 dealer の share を読み戻した。7枠・16枠・64枠で誤値と欠落の組み合わせを変えた計9ケースすべてで、全 dealer の定数が独立した Python 計算と一致した。この試験は公開行の解決後の処理を対象とし、署名の受理と release 時刻の判定は native 側の接続項目として残る。[再計算条件と結果](targeted-tests.json)

DAA 4689 で同じ親の子ブロック2件が全ピアに到着し、各ピアの選択先が分かれた。両方を通常のブロック検証に再提出した後、3ピアは同じ先端に収束し、元の claim の受付・Panel 承認を読み戻せた。[分岐前後の3ピアと6件の再検証応答](sibling-resolution.json)

異議申立期間中の DAA 4859 で隔離VMを正常停止し、検証用ディスクの上限を42 GiBから54 GiBに拡張した。同じノードバイナリと3つのデータベースで再開し、DAA 4864 に全ピアが元の受付ブロック、Panel 承認と未消費の権利を返した。[再起動後の3ピアの照会](notice-window-restart.json)

DAA 5213 の状態を同じブロックに固定して再読した。3ピアとも7,147,447バイトが一致し、Panel 承認済みの claim は `receipt_licensed`、Gate C は `pending` だった。抽選の照合器はこの状態を `claim is not Final` で終了した。この時点では当落を判定できなかった。[3ピアの固定ブロックと照会結果](before-final-capture.json)・[固定した状態（Borsh、zstd）](before-final-state.borsh.zst)

この検査はリポジトリのルートから次のコマンドで再現できる。照合器は当該 claim の Final、Gate C の行、異議申立中の時計停止を照合し、成熟後に各 ticket と target を計算する。Court への申立てがない claim にも使える。 保存済みの旧抽選では、従来の署名付き Court 応答の照合と全21枚の ticket 再現も通った。同じ資料の network domain、価格基準クラス、成熟期間、親ブロックの DAA、単一ピアの attempt 列をそれぞれ差し替えると、照合器は全5件を拒否した。

```sh
d=docs/evidence/palw/studio-main-claim-2026-11-20
state=$(mktemp)
zstd -dc "$d/before-final-state.borsh.zst" > "$state"
cargo run --locked -p kaspa-consensus-core --example protected_studio_draw -- --draw "$d/before-final-capture.json" "$state"
```

最終結果も同じ手順で再検算できる。

```sh
d=docs/evidence/palw/studio-main-claim-2026-11-20
state=$(mktemp)
zstd -dc "$d/draw-state.borsh.zst" > "$state"
cargo run --locked -p kaspa-consensus-core --example protected_studio_draw -- --draw "$d/draw-capture.json" "$state"
```

認証後の `--draw` 出力では、`gateC.drawnBonds` に抽選順の bond、`gateC.attestations` に記録された判定・transcript digest・DAA が入る。賛成数と反対数は抽選位置で数える。同じ bond が複数回選出された場合は、抽選位置数と実際の署名 bond 数を区別できる。

DAA 5474 で3ピアをいったん停止し、コンセンサス DB の全327ファイルを圧縮ストレージに移した。各ピアの個別アーカイブを元ファイルと比較し、移動先でも全件の SHA-256 を照合した。同じノードバイナリを再起動すると、3ピアとも DAA 5477 で元の受付ブロック、Panel 承認、未消費を返した。再起動直後のブロック再検証には drill 設定の `stepTimeoutSeconds` を適用する。[ファイル照合と再起動後の3ピアの記録](compressed-storage-restart.json)

DAA 6779 の後、claim 照会はホスト上で他プロセスが 11,395,497,704 バイトを予約中だったため、一時拒否された。同じ DB で再開すると DAA 6781 で3ピアの元の受付、Panel 承認、消費数 0 が一致した。この形式の一時拒否に対する再試行は同じ時計位置で行い、待機時間に上限を設ける。[RPC の応答と再起動後の3ピア照会](host-reservation-restart.json)

### Studio main の提出済みジョブから再開する

Studio のキューで追加したジョブは、起動中の Studio main の queue ID を指定して直接追跡できる。`--config` の `studio.port` をその Studio のループバック HTTP ポートに合わせ、gateway と署名済み rail のファイルがある outbox を指定する。

```sh
python3 scripts/drills/protected_private_registration_run.py \
  --config "$CONFIG" --output "$FRESH_OUTPUT" --resume-studio "$REGISTERED_RUN" \
  --studio-main-job "$QUEUE_ID" "$OUTBOX"
```

受付済みの場合はノードの claim 記録にある受付ブロックと取引位置を使う。未受付の場合はノードが返す pruning point 以降の選択鎖で署名済み取引を探す。探索開始位置が分かっていれば第3引数で指定できる。

drill は指定 ID が `committed` になるまで待ち、該当する gateway ファイルと `.rail.json` を読み、署名済み取引から先を追跡する。Studio main の実バイナリで保存済みの queue ID `1a1f8c8c241c3fc5588` を再読し、この経路が DAA 4429 に受け付けた同じ取引 `dcb4a75f…` に結び付くことを確認した。[queue ID・取引・受付ブロックの照合](queue-id-readback.json)

隔離3ピアを起動する `scripts/drills/protected_private_registration_run.py` に `--studio-queue-submission /absolute/path/to/submission.json` を渡すと、外部で起動した Studio main、gateway、rail の結果を受け取れる。通常の `--config` と `--output` に対応し、登録済みの隔離チェーンなら `--resume-studio` も使える。ジョブを送信した側で、次の4フィールドを持つ JSON を rail の提出後に rename で配置する。

| フィールド | 入力 |
| --- | --- |
| `studioRun` | Studio の `GET /api/v1/network/mining-queue` の `jobs` 配列にある対象行（`status=committed`） |
| `outbox` | その gateway の `fp-job-*.json` と rail の `retention/` を含む絶対ディレクトリ |
| `rail` | 同じ job で実行した `misaka-palw-fp-rail --submit` の JSON 出力 |
| `scanFromBlock` | rail が取引を提出する直前の、同じ隔離チェーンの sink hash |

drill は queue の job ID から gateway のファイルを開き、claim、executor bond、output root、execution root、network domain と rail の署名済み取引 ID を照合する。受付ブロック以降は従来と同じ Panel・Final・Gate C・受賞ブロックの観測を使用する。既に受付記録のある実行には `--resume-claim` を使用する。
