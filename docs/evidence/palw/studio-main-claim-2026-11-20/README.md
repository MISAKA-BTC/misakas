# MISAKA-Studio main から生成した TFO3 claim

2026-11-20 に MISAKA-Studio の `main`（`c9257516b23b14527ef9cef06af143756a422e8f`）を未変更のソースからビルドした。チャット画面と共通の `POST /api/v1/network/mining-queue` に “Continue the count: three, four,” を投入した。隔離 testnet-12 の登録済み TIR クラスで17入力トークン、1出力トークン、85,938 leaves を実行した。Studio の queue、gateway の receipt、rail による Borsh の再読は同じ claim `05ba473f…e1c0cce9` を返した。[入力・実行 root・ファイルハッシュ](commitment.json)

| 段階 | 観測 |
| --- | --- |
| 出力と証拠の配置 | gateway が証拠 24,621,472 バイトを24チャンクに分け、隔離 provider から読み戻した。 |
| protected quote | 3ピアが DAA 3617 で同じ Borsh を返した。[3ピアのハッシュ](protected-quote.json) |
| 初回の提出 | rail が元の claim と実行 root に署名した。取引 `d964e427…721d4b6` は選択鎖で受理された。再起動後の3ピアはいずれも、この取引による claim の受付を `found=false` と報告した。[取引の受理と claim の照会結果](carrier-acceptance.json) |
| 初回の受付拒否 | 実行者 bond のクラスは21ブロック分の枠を必要とした。epoch 4 の最終 DAA 3679 でも配分比率から得られる上限は13.61ブロック分だった。[ノードの拒否理由](allocation-refusal.log) |
| 資本割当 | 実行者が隔離チェーン上で、既存 bond の 10,000,000,000 sompi をこのクラスに割り当てた。3ピアとも sequence 1 を epoch 4 の pending として記録した。[署名済み取引の ID と3ピアでの照会結果](capital-assignment.json) |

別入力 “Hi” も Studio main の queue で実行した。入力は10トークン、価格は同じ21 quanta だった。この比較用 claim の処理は outbox までで、上表はすべて元の claim を追跡している。[比較用の実行記録](short-prompt-price.json)

保存した commitment は、次のコマンドでオフラインで再読できる。引数はこのディレクトリからの相対パス。

```sh
misaka-palw-fp-rail --artifact ./fp-job-5853ff54e729b614 --print-claim
```

隔離 VM の再起動後、3ピアで同じ bond・金額・sequence の割当を再読した。3ピアに共通して残っていた直後の子ブロックを通常の `submitBlock` で各ピアに再提出すると、3ピアすべてで選択鎖に戻り、以降のブロック生成も再開した。[再起動後の子ブロックの照会結果](retained-child-after-restart.json)

見積もり RPC は、同じ epoch の bond 指定時に、受付側と共通の関数でクラスの残り配分枠を照合する。テストで空の初期配分、必要量ちょうど、枠不足、epoch 切替を確認した。Rust の bond budget 91件、TB1 3件、MOE2 5件、Python の protected drill 62件、Studio main の mining queue 5件が通った。ノード側は `cargo check --locked -p kaspa-consensus --lib` も通った。
