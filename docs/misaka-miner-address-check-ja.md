# misaka-miner-address-check 使い方メモ

`scripts/misaka-miner-address-check.sh` は、**mining報酬アドレス**を指定して、そのアドレスにcoinbase報酬UTXOが来ているかを確認する読み取り専用ツールです。Discord bot からの `/miner address:<address>` 用にも使えます。

## できること

同期済みnodeのRPCに対して `misaka wallet utxo list` を実行し、指定アドレスのUTXO状況から報酬の有無を判定します。

| 見る値 | 内容 |
|---|---|
| `total` | UTXO総数 |
| `mature.count` / `mature.sompi` | 成熟済み（使用可能）UTXO |
| `immature.count` / `immature.sompi` | 未成熟（直近のcoinbase）UTXO |

## 重要な前提

このスクリプトは読み取り専用です。以下は行いません。

- `send` / `unbond` / bond作成
- 秘密鍵ファイルの読み取り・出力
- `systemctl restart`

つまり、`misaka wallet utxo list` の照会以外は何も変更しません。

なお、報酬UTXOが見えても「今この瞬間 miner プロセスが動いている」ことの証明にはなりません。リアルタイムの稼働確認には miner 自身のログ / metrics、または miner が接続している node 側の情報が必要です。

## 必要なもの

```text
同期済み MISAKA node
misaka CLI（環境変数 MISAKA_BIN で差し替え可能。既定は misaka）
127.0.0.1:27210 に接続できること
jq
```

## 基本コマンド

```bash
misaka-miner-address-check --address misakatest:...
```

Discord bot 向けの1行表示:

```bash
misaka-miner-address-check --address misakatest:... --discord
```

サンプルJSONで表示だけ確認（node不要）:

```bash
misaka-miner-address-check --self-test
misaka-miner-address-check --self-test --discord
```

## オプション

| オプション | 内容 | 既定 |
|---|---|---|
| `--address <addr>` | 報酬 / ウォレットアドレス（必須） | — |
| `--network <id>` | ネットワークID | `testnet-10` |
| `--rpc <host:port>` | node wRPC Borsh エンドポイント | `127.0.0.1:27210` |
| `--discord` | Discord bot 向けのコンパクト1行表示 | off |
| `--self-test` | 埋め込みサンプルJSONで表示確認 | off |
| `-h, --help` | ヘルプ | — |

環境変数 `MISAKA_NETWORK` / `MISAKA_RPC` / `MISAKA_BIN` でも既定値を変更できます。

## 出力例

人間向け:

```text
MISAKA miner address check
==========================
Address              misakatest:qexampleaddress...
Total UTXOs          4
Mature               3 (25.00000000 MSK)
Immature             1 (5.00000000 MSK)
Total reward         30.00000000 MSK
Verdict              RECENT_REWARD_SEEN
Meaning              recent reward, immature coinbase present

Note: reward UTXOs can show mining reward history, but do not prove the miner process is running right now.
```

Discord 1行:

```text
MISAKA miner | Address:misakatest:q...00000000 | Reward:RECENT_REWARD_SEEN (recent reward, immature coinbase present) | UTXO:4 | total:30.00000000MSK | mature:3/25.00000000MSK | immature:1/5.00000000MSK
```

## 判定（Reward verdict）の意味

| verdict | 表示上の意味 | 状態 |
|---|---|---|
| `RECENT_REWARD_SEEN` | recent reward, immature coinbase present | 未成熟UTXOあり。直近で報酬を受け取れている可能性が高い |
| `REWARD_HISTORY_SEEN` | past reward, mature only | 成熟UTXOのみ。過去に報酬受領の履歴あり |
| `NO_REWARD_UTXO` | no reward UTXO for this address | このアドレスに報酬UTXOなし |

`immature` UTXO が増えている、または `mature` 残高が増えているなら、報酬を受け取れている可能性が高いです。ただし前述の通り、これは miner プロセスがリアルタイムで稼働している証明ではありません。
