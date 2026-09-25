# PALW 参加手順(testnet-12)

PALW の参加者のロールは 2 つです。

| 役割 | モデル | Bond | 主なコマンド |
|---|---|---|---|
| Producer / miner | Floor は不要、8k class は必要 | PALW Bond(**13,000 MSK 以上**) | `misaka mining setup` |
| Panel seat(verifier) | 検証する class のものが必要 | capability を宣言した PALW Bond(**130,000 MSK 以上**) | `misaka verifier setup` |

**producer の node は panel seat の義務も常に実行します。** producer の bond で別に verifier を起動しないでください(1 つの bond は 1 つの process だけ)。seat だけを動かしたいときに `verifier` を使います。

## Producer

```bash
misaka --network testnet-12 mining setup
misaka --network testnet-12 mining start --print-command
misaka --network testnet-12 mining start
```

testnet-12 の block producer は `kaspad --palw-produce` です。外部の hash miner は PALW の attempt envelope を作れません。block は PALW ConsensusV2 が 120 秒ごとに作ります。

- producer の node(`--palw-producer-key` と `--palw-producer-bond` を持つ node)は、panel seat の義務と execution lane の round block を常に実行します。`--palw-produce` で決まるのは、attempt claim も開くかどうかだけです。
- S1 は DAA 0 から有効なので、producer は capture から決定論的な checkpoint `SC01` を公開します。checkpoint 用の別コマンドはありません。partial 席はこの opening から自分の区間だけを再開するので、**producer も検証席と同じ世代のバイナリ**で起動し、receipt の期限まで capture を持ち続けてください。

## Floor

BASE-0/Floor は組み込みの決定論的な整数 class で、常に `Active` です。GPU や artifact のダウンロードは要りません。class のフラグを省いて手動で起動すると Floor になります。

## Model class

testnet-12 の genesis にある model class は 2 つです。

| model id | class id | 状態 |
|---|---|---|
| `Qwen/Qwen2.5-1.5B/graph-v7@8192`(8k) | `ebf44d0a…` | `Prefetching` から始まる。ready seat が 7 つ揃うと `Probation` に進む |
| `Qwen/Qwen2.5-1.5B/graph-v7@2097152`(2M) | `74c67e63…` | **公開時点では規則で閉じている**(`ClassDeadlineUnmeasured`) |

8k class には、chain が登録した root と一致する artifact(1,799,359,436 bytes、inventory root `88096dc1…`)と、3.5 GiB 以上の memory share が必要です。作り方は [参加手順 §6](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) を見てください。Qwen3.6 の hybrid 行は genesis に入っていません。

class ID、状態、ready seat は、Wiki の値をコピーせずにチェーンで確認してください。

```bash
misaka --network testnet-12 model list
misaka --network testnet-12 palw registry
misaka --network testnet-12 model readiness <class-id>
```

## Bond and exposure

PALW Bond は、開いている claim の exposure の上限を持ちます。testnet-12 では、claim ごとに不正で得られる額の全額(escrow + weight)を bond に予約し、上限は `collateral × 500‰` です。floor claim 1 本の予約は約 3,200.95 MSK なので、同時に持てる floor claim は collateral 約 6,402 MSK ごとに 1 本です。

| class | 同時 1 本あたりの collateral |
|---|---|
| Floor | 約 6,402 MSK |
| 8k | 約 6,451 MSK |
| 2M | 約 125,888 MSK |

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
```

登録後に collateral を追加することはできず、同じ key で 2 つ目の bond は登録できません。容量が足りない場合は、新しい key と新しい Bond を使います。

## Panel seat

```bash
misaka --network testnet-12 verifier setup
misaka --network testnet-12 verifier start
misaka --network testnet-12 verifier status
```

panel seat は、panel に選ばれた claim を再実行して判定します。検証する class の artifact と capability の宣言が必要です。seat の bond は 130,000 MSK 以上で、`Valid` 署名が取る lock を払える空き collateral が必要です。quorum と違う判定をした seat は課金されます。

S1 / S2 / S3 は DAA 0 から有効で、別のプロセスやフラグはありません。上の 3 つのコマンドのまま動きます。手順、ログの見方、よくある失敗は [検証参加ガイド](Testnet-12-Verification-Participation-JA) にあります。

## 停止と課金

```bash
misaka --network testnet-12 mining stop --drain
```

testnet-12 では、消えた producer は課金されます。`ProducerWithholding` の void と 2 回目の `ReceiptTimeout` は weight + escrow(floor claim 1 本で約 3,200.85 MSK)を没収します。`BindTimeout` と `NoCapablePanel` は課金されません。

## 報酬

Final になった claim の報酬は、すぐには払われず vesting 行に入り、行が成熟して動いたときに mint されます(ADR-0152)。

```bash
misaka --network testnet-12 rewards
misaka --network testnet-12 palw vesting
```

## Free-prompt lane

free-prompt は、ユーザーが必要とした inference の receipt を work にする別のレーンです。gateway、worker、rail と producer/panel を組み合わせます。Floor producer の通常の初期設定には要りません。

## Epoch budget release

ADR-0123 の、使われていない class budget を解放する規則(`palw_epoch_budget_release`)は、**testnet-12 では DAA 0 から有効**です。予算を使い切った class は、他の class が埋めていない epoch の枠を借ります。途中の epoch で登録された class は、次の境界まで予算が 0 です。testnet-11 では無効のままです。

## 正本

- [検証参加ガイド](Testnet-12-Verification-Participation-JA)
- [Testnet-12 参加手順](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md)
- [testnet-12 公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md)
- [ADR index](https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr)
