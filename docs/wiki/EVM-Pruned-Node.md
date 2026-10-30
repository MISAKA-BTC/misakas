# EVM Pruned Node（current main）

EVM state の保持方法は current `kaspad --help` と [flat backend runbook](https://github.com/MISAKA-BTC/misakas/blob/main/docs/misaka-evm-flat-backend-runbook-v0.1.md) を正とします。旧 `--evm-storage-profile` / `--evm-node-role` 手順は現行 CLI ではありません。

## Current flags

- `--evm-history-mode=head|recent|archive` — state history retention
- `--evm-shadow-state-backend` — flat state を従来 snapshot と照合する
- `--evm-flat-authoritative` — flat backend を authoritative にする段階
- `--evm-retire-206` — legacy per-block snapshot の新規保存を止める
- `--evm-prune-legacy-206` — legacy snapshot を回収する破壊的移行
- `--node-profile=full|bootstrap-pruned|recovery-sync|validator|archive|public-rpc`

利用できる組み合わせと拒否条件は build の `kaspad --help` を確認してください。

## Safe migration order

1. current main を build する。
2. 稼働 binary、service unit、appdir の復旧可能な backup を作る。
3. `recent` または `archive` で shadow/backfill を完了する。
4. node の検証ログと EVM RPC を確認する。
5. flat authoritative へ移行する。
6. legacy writer を停止する。
7. 十分な soak と復旧試験後にのみ legacy 206 を回収する。

`head` は legacy snapshot 回収後の再構築元を保持しないため、runbook が許可する場合を除き migration の保持モードとして使いません。

## Destructive step

`--evm-prune-legacy-206` はディスク上の legacy data を削除します。通常起動フラグとして常設せず、backup と rollback 手順を確認した一回の保守作業として実行します。

## testnet-12

testnet-12 の PALW node/producer の通常の参加には EVM prune 設定は不要です。まず [Quick Start](Quick-Start) で同期し、EVM RPC/履歴を提供する運用者だけがこのページの移行を行います。
