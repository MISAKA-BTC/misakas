# Testnet-12 Operator UI(ADR-0122)

`misaka` は mining、Bond、work、reward、model、verifier、validator を 1 つの入口から扱います。対象は testnet-12 と、その現行ビルド `8a0810992`(2026-09-26 の node 更新。公開時の `0e8ec984e` と consensus は同じ)です。`misaka` はネットワークを指定しなければ testnet-12 を使いますが、このページの例では `--network testnet-12` を明示しています。

## セットアップ

```bash
misaka --network testnet-12 mining setup
```

ウィザードは node、network、model、key、資金、Bond registry、artifact、capability、fee output を順に確認し、`~/.misaka/mining.toml` を作ります。途中で止まっても、同じコマンドを実行すればチェーンとローカルの状態から再開します。

- 登録には `kaspad --palw-register-bond` を使います(operator-possession 署名つき)。
- `--palw-bond-collateral` は渡しません。ノードが導出した既定の collateral を登録し、その額に合う資金を求めます(Floor で約 31,191 MSK、`--model` に 8k を指定すると約 2,000,332,625 MSK で調達できません)。額を自分で決めるとき、および model class のときは [参加手順 §5](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) の手動の形を使います。
- testnet-12 には faucet がまだないので、ウィザードは faucet の案内を出しません。

## 既存 Bond の確認

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
misaka --network testnet-12 bond status --key-file ~/.misaka/miner.seed
```

`--bond` の形は key が要りません。既定では Floor に対する exposure と必要な collateral も表示します。別の class を診断するときだけ `--class-id <128-hex>` を指定します。

`REGISTERED` の Bond に `--palw-register-bond` を再実行しないでください。collateral は登録後に追加できず、registry は追記のみです。同じ key は、bond を退役させた後も 2 つ目を登録できません(testnet-12 は `palw_operator_id_unique` が DAA 0 から有効)。容量を増やすには、新しい key と新しい Bond を使います。

## Floor

```bash
misaka --network testnet-12 mining setup \
  --model floor --key-file ~/.misaka/miner.seed \
  --bond <txid>:<index>
misaka --network testnet-12 mining start --print-command
misaka --network testnet-12 mining start
```

Floor は組み込みの整数 class なので artifact は要りません。`kaspad` を手動で起動する場合は `--palw-producer-class` と `--palw-class-artifact` を省きます。

producer の node(`--palw-producer-key` と `--palw-producer-bond` を持つ node)は、panel seat の義務と execution lane の round block を**常に**実行します。オフにするフラグはありません。`--palw-produce` で決まるのは、この node が attempt claim も開くかどうかだけです。

## 8k モデル

8k の Qwen2.5 class(`Qwen/Qwen2.5-1.5B/graph-v7@8192`)を生産するときは、artifact を指定します。

```bash
misaka --network testnet-12 mining setup --model <class-id> --artifact /path/qwen25-1.5b-a16-8k.palwart
misaka --network testnet-12 mining start
```

`mining start` は `--palw-host-memory-share` を渡しません。8k の seat には 3.5 GiB 以上(`--palw-host-memory-share=3758096384`)が要るので、`~/.misaka/mining.toml` の `[advanced] extra_kaspad_args` に書いてください。artifact の作り方は [参加手順 §6](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) にあります。

## Status

```bash
misaka --network testnet-12 mining status --watch 5
misaka --network testnet-12 doctor
misaka --network testnet-12 work list
misaka --network testnet-12 rewards
misaka --network testnet-12 palw registry                 # 各 class の lifecycle の状態と ready seat
misaka --network testnet-12 palw panel list --class <id>  # bond 済み・ready・選ばれた seat と receipt
misaka --network testnet-12 model readiness <class-id>    # 各 seat の証明の経過時間、collateral、ready でない理由
misaka --network testnet-12 palw round-lane               # execution lane の stage、permit、予定
misaka --network testnet-12 palw vesting                  # Final の報酬が入る vesting 行
```

testnet-12 では、Final になった claim の報酬はすぐには払われません。まず vesting 行に記録され、その行が成熟して動いたときに mint されます(ADR-0152)。`rewards` はこれを `vesting` / `moved` として表示します。

`mining status` や `doctor` が **`E-MODEL-NOT-ADMITTING`**(「Not mining: the chain admits no new claim of this class now.」)を出したら、その class はいま claim を受け付けていません。`misaka palw registry` で lifecycle の状態を確認してください。この finding の hint には `misaka panel list` と書かれますが、正しいコマンドは `misaka palw panel list` です。

## Dashboard

```bash
misaka --network testnet-12 dashboard --listen 127.0.0.1:8791
```

同じホストで [http://127.0.0.1:8791](http://127.0.0.1:8791) を開きます。リモートの VPS では、外部に公開する bind をせず、SSH tunnel を使います。

```bash
ssh -L 8791:127.0.0.1:8791 user@server
```

gateway の既定 `8790` と dashboard の `8791` は別のサービスです。

## Verifier and model

```bash
misaka --network testnet-12 verifier setup
misaka --network testnet-12 verifier status
misaka --network testnet-12 model list
misaka --network testnet-12 model status --help
```

verifier は、他の Bond の claim を判定する panel seat です。seat の bond には 130,000 MSK 以上が要ります。**producer として動かしている bond を、別の process の verifier として動かさないでください。** producer の node は、同じ bond の seat の義務をすでに実行しています。2 つ目の process は round permit への二重署名(slash)の原因になります。手順は [検証参加ガイド](Testnet-12-Verification-Participation-JA) を見てください。

## Stop

```bash
misaka --network testnet-12 mining stop --drain
```

防御が終わっていない claim がある間は、ふつうの停止は拒否されます。`--drain` は新しい work を止め、開いている claim、panel、court の義務が終わるまで process を動かし続けます。

**testnet-12 では、消えた producer は課金されます。** `ProducerWithholding` の void(DA court が default を確定)と、2 回目の `ReceiptTimeout` は、weight + escrow(floor claim 1 本で約 3,200.85 MSK)を没収します。わざと隠した場合も、node が単に落ちていた場合も同じです。`BindTimeout` と `NoCapablePanel` は課金されません。`--force` は緊急時だけ使ってください。

## 正本

- [Testnet-12 参加手順](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md)
- [testnet-12 公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md)
- [ADR-0122](https://github.com/MISAKA-BTC/misakas/blob/main/docs/adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)
- [ADR-0123](https://github.com/MISAKA-BTC/misakas/blob/main/docs/adr/0123-the-epoch-progressively-releases-unused-class-budget.md)
