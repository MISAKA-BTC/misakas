# PALW 参加手順(testnet-12)

## PALW の参加者のロールは 2 つ

| 役割 | 責務 | モデル | Bond | 主なコマンド |
|---|---|---|---|---|
| **Producer / miner** | PALW の仕事を実行し、受理される block / claim を作って送る | Floor は不要、8k class は必要 | PALW Bond(**13,000 MSK 以上**) | `misaka mining setup` |
| **Panel seat(verifier)** | 割り当てられた claim を再実行し、結果を検証して receipt(verdict)を返す。採掘はしない | 検証する class のものが必要 | capability を宣言した PALW Bond(**130,000 MSK 以上**) | `misaka verifier setup` |

DNS finality の validator(`misaka validator`、bond **20,000,000 MSK 以上**)は PALW とは別の役割です(→ [DNS の位置づけ](#dns-の位置づけ))。

**producer の node は panel seat の義務も常に実行します。** producer の bond で別に verifier を起動しないでください(1 つの bond は 1 つの process だけ。2 つ動かすと round permit に二重署名して slash されます)。seat だけを動かしたいときに `verifier` を使います。

## Producer

手順は [Quick Start](Quick-Start#6-setup-and-start) を見てください。testnet-12 の block producer は `kaspad --palw-produce` です。外部の hash miner は PALW の attempt envelope を作れません。block は PALW ConsensusV2 が 120 秒ごとに作ります。

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

8k class には、chain が登録した root と一致する artifact(1,799,359,436 bytes、inventory root `88096dc1…`)と、3.5 GiB 以上の memory share が必要です。作り方は [Adding a Model](Adding-a-Model-JA#artifact-を作る8k-の例) を見てください。

class ID、状態、ready seat は、Wiki の値をコピーせずにチェーンで確認してください。

```bash
misaka --network testnet-12 model list
misaka --network testnet-12 palw registry
misaka --network testnet-12 model readiness <class-id>
```

## Bond and exposure

PALW Bond は、開いている claim の exposure の上限を持ちます。testnet-12 では、claim ごとに不正で得られる額の全額(escrow + weight)を bond に予約し、上限は `collateral × 500‰` です。公開時の測定では floor claim 1 本の予約は約 3,200.85 MSK で、同時に持てる claim 1 本あたりの collateral は次のとおりです(escrow は block の subsidy に比例します。計算は [参加手順 §5](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md#5-register-a-bond))。

| class | 同時 1 本あたりの collateral |
|---|---|
| Floor | 約 6,402 MSK(13,000 MSK なら 2 本、100,000 MSK なら 15 本) |
| 8k | 約 6,451 MSK |
| 2M | 約 125,888 MSK |

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
```

実際の空きは `bond status` の `exposure_ceiling` と `reserved_exposure` で確認してください。登録後に collateral を追加することはできず、同じ key で 2 つ目の bond は登録できません。容量が足りない場合は、新しい key と新しい Bond を使います。

## Panel seat

```bash
misaka --network testnet-12 verifier setup
misaka --network testnet-12 verifier start
misaka --network testnet-12 verifier status
```

panel seat は、panel に選ばれた claim を再実行して判定します。検証する class の artifact と capability の宣言が必要です。seat の bond は 130,000 MSK 以上で、`Valid` 署名が取る lock を払える空き collateral が必要です。quorum と違う判定をした seat は課金されます。S1 / S2 / S3 は DAA 0 から有効で、別のプロセスやフラグはありません。手順、ログの見方、よくある失敗は [検証参加ガイド](Testnet-12-Verification-Participation-JA) にあります。

## 停止と課金

```bash
misaka --network testnet-12 mining stop --drain
```

testnet-12 では、消えた producer は課金されます。`ProducerWithholding` の void(DA court が default を確定)と 2 回目の `ReceiptTimeout` は weight + escrow(floor claim 1 本で約 3,200.85 MSK)を没収します。わざと隠した場合も、node が単に落ちていた場合も同じです。`BindTimeout` と `NoCapablePanel` は課金されません。

## 報酬

Final になった claim の報酬は、すぐには払われず vesting 行に入り、行が成熟して動いたときに mint されます(ADR-0152)。

```bash
misaka --network testnet-12 rewards
misaka --network testnet-12 palw vesting
```

## Free-prompt lane

free-prompt は、ユーザーが必要とした inference の receipt を work にする別のレーンです。gateway、worker、rail と producer/panel を組み合わせます。Floor producer の通常の初期設定には要りません。

## Epoch budget release

[ADR-0123](https://github.com/MISAKA-BTC/misakas/blob/main/docs/adr/0123-the-epoch-progressively-releases-unused-class-budget.md) の、使われていない class budget を解放する規則(`palw_epoch_budget_release`)は、**testnet-12 では DAA 0 から有効**です。予算を使い切った class は、他の class が埋めていない epoch の枠を借ります。途中の epoch で登録された class は、次の境界まで予算が 0 です。

## DNS の位置づけ

DNS seeder は peer を見つけるためのネットワーク基盤です。PALW の仕事を作らず、採掘せず、Panel verifier として verdict を返すこともありません。node は組み込みの seeder 名(`seeder1.misakascan.com` など)を引き、返ってきた IP の 26311 番に接続します。DNS で見つからない場合は `--addpeer=169.58.232.113:26311` を追加します。node だけを運用する場合は、PALW の Bond もモデルの artifact も要りません。

DNS finality の validator(`misaka validator`)は DNS seeder とは別物です。testnet-12 では、validator が 6 つ以上、active stake が合計 120,000,000 MSK 以上になるまで DNS finality は Bootstrap の状態で、DNS の reorg gate は効きません(公開ノートの既知の問題 3)。手順は [Validator runbook](https://github.com/MISAKA-BTC/misakas/blob/main/docs/validator-runbook.md) にあります。

## 正本

- [検証参加ガイド](Testnet-12-Verification-Participation-JA)
- [Testnet-12 参加手順](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md)
- [testnet-12 公開ノート](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md)
- [ADR-0122](https://github.com/MISAKA-BTC/misakas/blob/main/docs/adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)
- [ADR index](https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr)
