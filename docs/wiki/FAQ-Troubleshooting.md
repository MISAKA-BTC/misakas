# FAQ / Troubleshooting — testnet-12

## Bond carrier は正式に登録されているか

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
```

`registry: REGISTERED` なら正式な PALW Bond です。locked UTXO が見えるだけでは、登録済みかどうかは判断できません。

## bond の額はいくらか

testnet-12 は mainnet 想定の額です。producer は **13,000 MSK 以上**、panel seat は **130,000 MSK 以上**、DNS finality の validator は **20,000,000 MSK 以上** です。testnet-11 の少額 bond(0.004 MSK)は通りません。

## REGISTERED なのに UNDERSIZED / holding

登録されていることと、生産できる量は別です。testnet-12 では、claim ごとに不正で得られる額の全額(escrow + weight、floor claim 1 本で約 3,200.95 MSK)が bond に予約されます。予約できる上限は `collateral × 500‰` です。

- 1 つの bond が同時に持てる floor claim は、collateral 約 6,402 MSK ごとに 1 本です(13,000 MSK なら 2 本)。
- 上限に達すると、producer は `the bond's exposure ceiling leaves no room for another claim` で待ちます。止まったままになるわけではありません。panel を引き直さずに全 seat が `Valid` を返した claim は、licence の時点で escrow 分が先に解放されます(2M class を除く)。それ以外は claim が `Final` になるか void になると空きます。
- ノードが出す「may then hold forever」の警告と `bond status` の `UNDERSIZED` は、古い weight だけの式(Floor で約 31,191 MSK)との比較です。testnet-12 の実際の空きとは別物なので、`exposure_ceiling` と `reserved_exposure` を見てください。
- 登録後に collateral を追加することはできません。容量を増やすには、新しい producer key と新しい Bond を作ります。
- 同じ key での再登録は `DuplicateBondKey` で拒否されます。

## `NOT PRODUCING for N min — holding: <reason>`

producer が 30 分待っていると ERROR で出ます。`holding:` の後ろの理由を読んでください。exposure の上限、class が claim を受け付けていない、などが書かれます。

## `E-MODEL-NOT-ADMITTING`

「Not mining: the chain admits no new claim of this class now.」は、その class がいま claim を受け付けていないという意味です。lifecycle の状態が `Candidate`、`Registered`、`Prefetching`、`Held` のどれかか、ready seat に空きがありません。

```bash
misaka --network testnet-12 palw registry
misaka --network testnet-12 model readiness <class-id>
```

`Prefetching` の class は、ready seat が 7 つになるのを待っています。よくある原因は seat の memory share 不足です(8k は 3.5 GiB 以上)。finding の hint には `misaka panel list` と出ますが、正しいコマンドは `misaka palw panel list` です。

## `[palw-lane-watch] no PALW work block …`

block は届いて DAA も進んでいるのに、selected chain に PALW の work がない状態です(heartbeat だけで時計が進んでいる)。floor producer が動いているか、panel seat が上がっているか、各 producer の `holding:` の行を確認してください。`PALW work is back on the chain: …` が出れば解消です。

## Floor で artifact を要求された

Floor は artifact が要りません。`misaka mining setup --model floor` を使い、手動で起動する場合は `--palw-producer-class` と `--palw-class-artifact` を外します。

## 2M の class で生産できない

`Qwen/Qwen2.5-1.5B/graph-v7@2097152`(2M)は、公開時点では規則で閉じています。attempt と free-prompt claim は `ClassDeadlineUnmeasured` で拒否されます。

## Connection refused / WebSocket error

用途ごとにポートが違います。

| 接続 | 既定 |
|---|---|
| P2P | `26311` |
| gRPC | `26210` |
| wRPC Borsh | `27210` |
| wRPC JSON | `28210` |
| EVM JSON-RPC | `8545` |

`misaka`、wallet、validator は wRPC Borsh を使います。node を `--rpclisten-borsh=default` を付けて起動してください。bind 先を変えた場合は `--rpc host:port` を明示します。

## peers が増えない / genesis mismatch

まず network と fingerprint を確認します。

```bash
misaka --network testnet-12 doctor
```

- node は `--testnet --netsuffix=12` で起動します。`--netsuffix` を省略しないでください。
- DNS で peer が見つからない場合は `--addpeer=169.58.232.113:26311` を追加します。
- genesis mismatch で起動しない場合は、最初の testnet-12(`a8cabac4…`)か testnet-11 の datadir を使っています。削除せずに退避し、新しい datadir で同期します。
- 現行 `main` のビルドは testnet-11 に参加できません。testnet-11 を続ける場合は commit `1f98d3bf4` をビルドします。

## hash miner が block を作らない

testnet-12 の producer は `kaspad --palw-produce` です。`kaspa-pq-miner` と `misaminer` は PALW の attempt を作れません。ADR-0122 の `misaka mining setup/start` を使います。

## bond が slash された(`RoundPermitEquivocated`)

同じ bond の key と outpoint で `kaspad` が 2 つ動いていました。standby の node、別ホストのコピー、動いている node の横で起動した `kaspad --palw-register-class`、新しい app dir で起動し直した node は、どれも 2 つ目の process になります。1 つの bond は 1 つの process だけで動かし、app dir は消さずに保ってください。

## `--palw-panel` / `--palw-round-lane` の警告

testnet-12 では、panel seat の義務と execution lane の round block は常に動きます。この 2 つのフラグは受け付けますが何もせず、警告を 1 回ずつ出すだけです。外してかまいません。

## fee outpoint のエラー

Bond collateral と fee outpoint は別の UTXO です。fee outpoint は、成熟済みで、coinbase ではなく、まだ使われていないものが必要です。wizard を再実行すると候補を確認し直します。fee outpoint がない node は receipt だけを出し、carrier(data-availability の応答、filer など)をチェーンに出しません。

## mining stop が拒否される

終わっていない claim の防御の義務があります。

```bash
misaka --network testnet-12 mining stop --drain
```

testnet-12 では、消えた producer は課金されます(`ProducerWithholding` の void と 2 回目の `ReceiptTimeout` は weight + escrow を没収)。`--force` は claim を失う可能性がある緊急停止です。

## 入金はいつ確定とみなせるか

block 数、blue score の深さ、DAA の差、heartbeat の本数は使わないでください。どれも heartbeat で水増しできます。少額なら自分の node の selected chain で、取引を含む block より blue score が 30 以上高い位置に blue の attempt があり、その後も attempt が続いていること。高額なら finality depth 600 blue(約 4 時間)か、`misaka palw settlement` で Final の anchor に達していることを確認します。詳しくは [公開ノート §3](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md) を見てください。

## Faucet はあるか

testnet-12 の faucet はまだ資金が入っていません(未定)。

## dashboard が開かない

```bash
misaka --network testnet-12 dashboard --listen 127.0.0.1:8791
```

リモートのホストなら SSH tunnel を使います。`8790` は gateway、`8791` は dashboard です。
