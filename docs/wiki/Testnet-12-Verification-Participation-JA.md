# Testnet-12 検証参加ガイド

このページは、testnet-12 に PALW の検証席(panel seat / verifier)として参加する手順です。検証席は、他の参加者が出した PALW claim を再実行し、その結果(receipt)をチェーンに提出します。検証席は block を採掘しませんが、claim を Final に進めるのに欠かせない役割です。

対象は **現行 `main`(release commit `0e8ec984e`)と testnet-12** です。network id や consensus fingerprint が違う node は参加できません。

## testnet-11 からの主な変更

- **seat の bond は 130,000 MSK 以上**(producer floor 13,000 MSK の 10 倍)。testnet-11 の少額 bond は使えません。
- **S1 / S2 / S3 は DAA 0 から有効です。** testnet-11 のように DAA 7,200 / 8,600 / 8,700 で切り替わることはありません。fence schedule は `1000` だけです(bond maturity window)。
- **DAA 1,000 以降、bond は登録から 1,000 DAA(約 33 時間)経つまで判定に使えません**(ADR-0065 D1)。
- **seat の義務は常に on です。** `--palw-panel` は受け付けますが何もせず、警告を 1 行出すだけです。
- **1 つの bond は 1 つの process だけで動かします。** producer の node は同じ bond の seat の義務をすでに実行しているので、同じ bond で verifier を別に起動してはいけません。

## 最短手順

### 1. 必要なもの

- Linux または macOS の、常時動かせるホスト
- `kaspad` を動かせるディスク、メモリ、安定したネットワーク
- `--utxoindex` を付けた testnet-12 の node
- 検証する class の artifact(Floor だけを検証するなら不要)
- 検証する class の capability を宣言した PALW Bond(**130,000 MSK 以上**)と、その key
- bond とは別の output の fee float(0.1 MSK 以上)。receipt、readiness 証明、data-availability の応答、court の応答などの carrier の手数料に使います

検証席は、Bond が宣言した class だけを検証します。秘密鍵はコマンドラインに書かず、key file の権限は `0600` にしてください。

### 2. ビルド

```bash
git switch main
git pull --ff-only
cargo build --release -p kaspad -p misaka-cli
```

CLI は `target/release/misaka`、node は `target/release/kaspad` に出力されます。

### 3. node を参加させる

```bash
./target/release/kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

DNS seeder で peer が見つからない場合は、公開エントリポイントを追加します。

```bash
./target/release/kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default \
  --addpeer=169.58.232.113:26311
```

起動ログで fingerprint と fence schedule を確認します。

```text
Consensus params fingerprint: b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f (network testnet-12)
Consensus fence schedule: 1000 (schedule id 93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd)
```

### 4. 検証席をセットアップする

```bash
misaka --network testnet-12 verifier setup
```

対話なしで実行するときは `--yes` を付けます。既存の Bond を指定する場合は次の形です。

```bash
misaka --network testnet-12 verifier setup \
  --model floor \
  --key-file "$HOME/.misaka/miner.seed" \
  --bond <bond-txid>:<index>
```

8k の LLM class を検証する場合は、先に class を確認し、対応する artifact を渡します。

```bash
misaka --network testnet-12 model list
misaka --network testnet-12 verifier setup \
  --model <model-name-or-class-id> \
  --artifact /absolute/path/to/qwen25-1.5b-a16-8k.palwart \
  --key-file "$HOME/.misaka/miner.seed" \
  --bond <bond-txid>:<index>
```

`verifier setup` は途中で止めても、再実行すれば終わった項目を使って続きから進みます。Bond の登録や送金のように資金を使う操作は、`--yes` を付けない限り確認を求めます。

setup は capability の宣言(`bond capability --declare`)も行います。何も宣言していない bond は panel に選ばれません。宣言は追加ではなく置き換えです。手動で宣言する場合は [参加手順 §5](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) を見てください。

### 5. 8k seat のメモリ

8k の seat が全区間を replay するには約 3.37 GiB 必要です(artifact 1.68 GiB + trace scratch 1.67 GiB)。**`--palw-host-memory-share=3758096384`(3.5 GiB)以上**を指定してください。3.0 GiB では `readiness … no proof — a replay needs 3.37 GiB as full-seat …` がログに出て、seat がいつまでも ready になりません。ほかには何も知らせてくれません。

`misaka verifier start` はこのフラグを渡さないので、`~/.misaka/mining.toml` の `[advanced] extra_kaspad_args` に書きます。

### 6. 検証席を起動する

まず、実行されるコマンドを確認します。

```bash
misaka --network testnet-12 verifier start --print-command
```

`--palw-produce` が含まれていないことを確認します(`--palw-panel` が含まれていても、testnet-12 では何もしません)。問題がなければバックグラウンドで起動します。

```bash
misaka --network testnet-12 verifier start --detach
```

起動ログに `PALW duties (on by construction; …)` の行が出ます。fee outpoint がないと `panel seat duties ON as bond … (receipts only: …)` になり、carrier をチェーンに出せません。key file が読めない、または bond が解釈できないときは `PALW duties NOT as planned: …` が出ます。

## seat が選ばれる条件

- Bond が class の capability を宣言している(Floor の場合)。model class の場合は、新しい readiness(possession)証明がある。証明は artifact を持った panel が自動で提出します。
- seat の空き collateral が、その claim で `Valid` 署名が取る lock を満たしている(floor claim で約 112.56 MSK)。満たせない bond は、選ばれてから失敗するのではなく、最初から抽選で外されます。
- model class の readiness には、空き collateral が 39,000 MSK(producer floor の 3 倍)必要です。
- DAA 1,000 以降は、bond の登録から 1,000 DAA 経っている。

## S1 / S2 / S3

S1・S2・S3 は別のサービスではなく、フラグもありません。testnet-12 では **DAA 0 から**、上の `verifier start` で起動した panel がこの方式で検証します。ゼロ知識証明は使いません。

| 方式 | 席の動き |
|---|---|
| **S1**(区間 replay) | 5 席のうち 1 席が全区間を replay(full 席)。残りの 4 席は重ならない 1 区間ずつ。producer は決定論的な checkpoint `SC01` を出し、partial 席は checkpoint から自分の区間だけを replay する |
| **S3**(層サンプル) | S1 に加えて、partial 席が層と位置のサンプルでも検証する |
| **S2**(optimistic) | full 席の `Valid` だけで optimistic に licence できる。collector は coverage → optimistic → V1 の順に試す |

- 割り当ては bind の `(anchor, claim, seat_count)` から決まります。席が区間を選ぶことはありません。自分が full 席か partial 席かは抽選の結果なので、`verifier status` と node のログで確認します。
- 抜けや重なりがある receipt、checkpoint・root・class・mask が違う receipt は拒否されます。
- partial 席は、checkpoint を開けないときは待ち、receipt を出しません。genesis から全区間を歩き直すことはありません。
- producer は receipt の期限まで capture を持ち続け、席からの checkpoint や opening の要求に応えます。

## 報酬と課金

- testnet-12 では、Final になった claim の報酬は vesting 行を通して支払われます(ADR-0152)。確認には `misaka rewards` と `misaka palw vesting` を使います。
- `verifier status` は現状 `paid: false` と表示します。
- **quorum と違う判定をした seat は課金されます。** 手元の artifact と build が正しいことを確認してから座ってください。

## 状態の確認

```bash
misaka --network testnet-12 doctor
misaka --network testnet-12 verifier status
misaka --network testnet-12 verifier status --output json
misaka --network testnet-12 palw panel list --class <class-id>
misaka --network testnet-12 model readiness <class-id>
misaka --network testnet-12 work list
misaka --network testnet-12 work show <claim-id>
misaka --network testnet-12 work why <claim-id>
misaka --network testnet-12 logs node
```

`getPalwNodeStatus.panelRunning` でも、panel が実際に起動したかどうかを確認できます。claim がまだ表示されなくても、異常とは限りません。対象は、チェーン上の claim と seat の抽選で決まります。

### 8k seat の readiness を監視する

公開ノートの既知の問題 6 のとおり、8k の seat の readiness が切れると被害が広がります。修正が入るまでは次を監視し、詰まった seat は再起動で回復させてください。

- `getPalwPanelSeats` の `proved` / `expires`
- ログの `no proof —` と `waits for a carrier slot`

## 参加できないとき

| 表示・症状 | 原因 | 対処 |
|---|---|---|
| fingerprint mismatch | 古いバイナリ、または別のネットワーク | `main` から再ビルドする。起動ログの fingerprint を確認する |
| genesis mismatch | 最初の testnet-12(`a8cabac4…`)の datadir | その datadir を削除せずに退避し、新しい datadir で同期する |
| `0 peers` | DNS、firewall、peer の ruleset の不一致 | `--addpeer=169.58.232.113:26311`、P2P `26311/tcp`、ログの fork-id を確認 |
| `bond unknown` | outpoint の間違い、または別のネットワーク | `bond status --bond <txid>:<index>` で outpoint を確認。genesis の bond は txid `5e0d5f1b…` の上にある |
| `judges nothing` | Bond が capability を宣言していない | `verifier setup --model ...` を実行する。登録済みの Bond は登録し直さない |
| seat が選ばれない | 空き collateral が `Valid` lock に足りない、bond が 1,000 DAA 未満、readiness 証明がない | `bond status`、`model readiness`、`palw panel list` を確認 |
| `readiness … no proof — a replay needs 3.37 GiB` | memory share が足りない | `--palw-host-memory-share=3758096384` 以上を `extra_kaspad_args` に書く |
| artifact missing / mismatch | class に合わない artifact、またはパスの間違い | `model list` で class を確認し、絶対パスを指定する。`palw-class manifest --check` で sidecar を確認 |
| panel が動かない | node が同期中、key file が読めない、bond が解釈できない | `PALW duties NOT as planned: …` の行、`doctor`、`getPalwNodeStatus.panelRunning` を確認 |
| `RoundPermitEquivocated` で slash された | 同じ bond を 2 つの process で動かした | 1 つの bond は 1 つの process だけで動かす。standby やコピーを止める |
| `SC01` が来ない / partial が待ち続ける | producer が古い、または capture を持っていない | producer も現行バイナリで `mining start`。receipt の期限まで capture を残す |
| claim が長く pending のまま | tip の進行、seat の稼働、producer の遅れ、S1 の coverage 不足 | `verifier status`、`work show`、node のログ、peer 数を確認 |

## 安全な停止

```bash
misaka --network testnet-12 verifier stop
```

この bond の claim がまだ必要としている間は、停止を拒否します。急ぐときだけ `--force` を使ってください。検証中の claim を放棄する可能性があるので、ふつうは先に `verifier status` で義務が残っていないことを確認します。

## 関連ページ

- [PALW 参加手順](PALW-Participation-JA)
- [Testnet-12 Operator UI](Testnet-12-Operator-UI-JA)
- [Quick Start](Quick-Start)
- [FAQ / Troubleshooting](FAQ-Troubleshooting)
- [Testnet-12 参加手順(リポジトリ)](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md)
- [testnet-12 公開ノート(リポジトリ)](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md)
