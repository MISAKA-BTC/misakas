# Testnet-11 Operator UI（ADR-0122）

> [!WARNING]
> **このページは旧ネットワーク testnet-11 向けの記録です。** 現行の公開テストネットは `testnet-12` です(2026-09-25/26 JST 公開、release commit `0e8ec984e`)。testnet-12 の手順は **[Testnet 12 Operator UI](Testnet-12-Operator-UI-JA)** を見てください。
> 現行 `main` のビルドは testnet-11 に参加できません。testnet-11 を続ける場合は commit `1f98d3bf4` をビルドし、fingerprint `79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640` を確認してください。このページの fingerprint、fence、公開エントリポイント(`169.58.39.220:26311`)は当時の値で、現在は正しくない場合があります。

`misaka` は mining、Bond、work、reward、model、verifier、validator を同じ入口から扱います。対象は current `main` / Testnet-11 Relaunch 5f です。

## セットアップ

```bash
misaka --network testnet-11 mining setup
```

ウィザードは node と identity、model、key、funding、Bond registry、artifact、panel capability、fee outpoint を検査し、`~/.misaka/mining.toml` を作ります。途中で止まっても同じコマンドで再開できます。

## 既存 Bond の確認

```bash
misaka --network testnet-11 bond status --bond <txid>:<index>
```

このモードは key 不要です。既定では Floor に対する exposure と必要 collateral も表示します。別クラスを診断するときだけ `--class-id <128-hex>` を指定します。

`REGISTERED` の Bond に `--palw-register-bond` を再実行しません。collateral は登録後に top-up できず、registry は append-only です。同じ key は退役後も二つ目を登録できないため、継続容量を増やす場合は新しい key と新しい Bond を使います。

## Floor

```bash
misaka --network testnet-11 mining setup \
  --model floor --key-file ~/.misaka/miner.seed \
  --bond <txid>:<index> --peer 169.58.39.220:26311
misaka --network testnet-11 mining start --print-command
misaka --network testnet-11 mining start
```

Floor は組み込み整数クラスなので artifact は不要です。手動 `kaspad` 起動では `--palw-producer-class` と `--palw-class-artifact` を省略します。

## Status

```bash
misaka --network testnet-11 mining status --watch 5
misaka --network testnet-11 doctor
misaka --network testnet-11 work list
misaka --network testnet-11 rewards
```

## Dashboard

```bash
misaka --network testnet-11 dashboard --listen 127.0.0.1:8791
```

同じホストで [http://127.0.0.1:8791](http://127.0.0.1:8791) を開きます。リモート VPS では公開 bind せず SSH tunnel を使います。

```bash
ssh -L 8791:127.0.0.1:8791 user@server
```

gateway の既定 `8790` と dashboard の `8791` は別サービスです。

## Verifier and model

```bash
misaka --network testnet-11 verifier setup
misaka --network testnet-11 verifier status
misaka --network testnet-11 model list
misaka --network testnet-11 model status --help
```

verifier は他 Bond の claim を判定する panel seat で、現状は無報酬です。

## Stop

```bash
misaka --network testnet-11 mining stop --drain
```

未防御 claim がある間は通常停止を拒否します。`--drain` は新規 work を止めて既存の責務を完了します。`--force` は緊急時のみ使用します。

## 正本

- [Testnet-11 mining runbook](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet11-join-mining.md)
- [ADR-0122](https://github.com/MISAKA-BTC/misakas/blob/main/docs/adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)
- [ADR-0123](https://github.com/MISAKA-BTC/misakas/blob/main/docs/adr/0123-the-epoch-progressively-releases-unused-class-budget.md)
