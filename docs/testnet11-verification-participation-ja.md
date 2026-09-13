# Testnet-11 検証参加ガイド

このページは、Testnet-11 の PALW 検証席（panel/verifier）としてネットワークに参加するための手順です。検証席は他の参加者が提出した PALW claim を再実行し、checkpoint の結果をチェーンへ提出します。検証席はブロックを採掘せず、現行ネットワークでは検証そのものに対する直接報酬はありませんが、claim を Final へ進めるために重要な役割です。

対象ネットワークは **testnet-11 Relaunch 5f** です。古い testnet-10、過去の relaunch、別の fingerprint のノードは参加できません。

## 最短手順

### 1. 必要なもの

- Linux または macOS のホスト（常時起動推奨）
- `kaspad` を実行できるディスク、メモリ、安定したネットワーク
- `--utxoindex` を有効にした Testnet-11 ノード
- 検証に使うクラスの artifact（Floor は不要、LLM クラスは必須）
- PALW Bond と、Bond の鍵

検証席は Bond が宣言したクラスだけを検証します。すでに採掘用 Bond がある場合は、その Bond を検証席にも使えます。秘密鍵はコマンドラインに書かず、ファイルの権限を `0600` にしてください。

### 2. ビルド

```bash
git switch main
git pull --ff-only
cargo build --release -p kaspad -p misaka-cli
```

現在の CLI は `target/release/misaka`、ノードは `target/release/kaspad` です。

### 3. ノードを参加させる

DNS seed が使える環境では、次だけで起動できます。

```bash
./target/release/kaspad \
  --testnet --netsuffix=11 \
  --utxoindex \
  --rpclisten-borsh=default
```

DNS が使えない場合は、公開エントリポイントを追加します。

```bash
  --addpeer=169.58.39.220:26311
```

起動ログで次を確認してください。

```text
Consensus params fingerprint: ae1d61628da50c7becea62f0a8f08c8654d190c60b2e104df0010b121ba4d3d8
Consensus fence schedule: 1150, 1900, 2150, 2400, 3500, 4000, 6900, 7000, 2125000
```

### 4. 検証席をセットアップする

既存の Bond を自動検出できる場合は、これが推奨手順です。

```bash
./target/release/misaka --network testnet-11 verifier setup
```

対話なしで実行する場合は `--yes` を追加できます。既存 Bond や artifact を明示する場合は次の形です。

```bash
./target/release/misaka --network testnet-11 verifier setup \
  --model floor \
  --key-file "$HOME/.misaka/miner.seed" \
  --bond <bond-txid>:<index> \
  --peer 169.58.39.220:26311
```

LLM クラスを検証する場合は、ネットワークの `model list` で現在のクラスを確認して、同じクラスに対応する artifact を渡します。

```bash
./target/release/misaka --network testnet-11 model list
./target/release/misaka --network testnet-11 verifier setup \
  --model <model-name-or-class-id> \
  --artifact /absolute/path/to/class-artifact \
  --key-file "$HOME/.misaka/miner.seed" \
  --bond <bond-txid>:<index>
```

`verifier setup` は中断しても、もう一度実行すれば完了済みの項目を再利用して続きから進みます。Bond の登録や資金移動など支出を伴う操作は、`--yes` を指定しない限り確認を求めます。

### 5. 検証席を起動する

まず実行予定のコマンドを確認します。

```bash
./target/release/misaka --network testnet-11 verifier start --print-command
```

問題がなければ、バックグラウンドで起動します。

```bash
./target/release/misaka --network testnet-11 verifier start --detach
```

このモードでは、ノードは `--palw-panel` で起動し、`--palw-produce` は付与されません。つまり検証席は検証だけを行い、誤って採掘プロセスを二重起動しません。

## 状態確認と検証結果の確認

### セットアップ前後の診断

```bash
./target/release/misaka --network testnet-11 doctor
```

`doctor` はノードの fingerprint、fence schedule、同期状態、peer、Bond、artifact、ホスト資源を確認します。JSON を使えば監視やスクリプトに組み込めます。

```bash
./target/release/misaka --network testnet-11 --output json doctor
```

### 自分の検証席の状態

```bash
./target/release/misaka --network testnet-11 verifier status
./target/release/misaka --network testnet-11 verifier status --output json
```

ここでは、Bond が登録済みか、宣言したクラス、panel が稼働中か、現在座っている claim、提出期限を確認できます。`paid: false` は現行仕様で検証席が直接報酬を受けないことを表します。

### claim 単位の追跡

```bash
./target/release/misaka --network testnet-11 work list
./target/release/misaka --network testnet-11 work show <claim-id>
./target/release/misaka --network testnet-11 work why <claim-id>
```

ログを追う場合は次を使います。

```bash
./target/release/misaka --network testnet-11 logs --component node
```

正常な検証席では、他ノードからの opening/checkpoint 要求、再実行、panel receipt の提出がログに現れます。claim がまだ表示されないことは異常とは限りません。検証対象はチェーン上の claim と seat 抽選によって決まります。

## 参加できないとき

| 表示・症状 | 原因 | 対処 |
|---|---|---|
| fingerprint mismatch | 古いバイナリまたは別 relaunch | `main` から再ビルドし、現在の datadir を継続して起動 |
| fence schedule mismatch | 現行 fence を含まないビルド | `git pull --ff-only` 後に `kaspad` と `misaka` を再ビルド |
| `0 peers` | DNS、firewall、または peer の ruleset 不一致 | `--addpeer=169.58.39.220:26311`、P2P 26311/tcp、ログの fork-id を確認 |
| `bond unknown` | outpoint の誤り、または別ネットワーク | `bond status --bond <txid>:<index>` で正確な outpoint を確認 |
| `judges nothing` | Bond がクラス capability を宣言していない | `verifier setup --model ...` を実行。登録済み Bond は再登録できないため、表示内容を確認 |
| artifact missing / mismatch | クラスに対応しない、またはパスが誤り | `model list` でクラスを再確認し、絶対パスを指定 |
| panel が動かない | ノードが同期中、または起動引数に `--palw-panel` がない | `verifier start --print-command` と `doctor` を確認 |
| claim が長時間 pending | chain tip の進行、seat の稼働、または producer 側の処理遅延 | `verifier status`、`work show`、ノードログ、peer 数を確認 |

## 安全な停止

```bash
./target/release/misaka --network testnet-11 verifier stop
```

停止を急ぐ場合だけ `--force` を使います。検証中の claim を放棄する可能性があるため、通常は先に `verifier status` で duties がないことを確認してください。

## CLI の対応状況

検証参加に必要な操作は unified CLI に揃っています。

| 目的 | コマンド |
|---|---|
| ネットワークへ参加 | `misaka join` または `kaspad --testnet --netsuffix=11` |
| 参加前診断 | `misaka doctor` |
| Bond・クラス・artifact の準備 | `misaka verifier setup` |
| 起動コマンド確認 | `misaka verifier start --print-command` |
| 検証席の起動・停止 | `misaka verifier start --detach` / `misaka verifier stop` |
| 座席と claim の確認 | `misaka verifier status` |
| claim の追跡 | `misaka work list|show|why` |
| ログ確認 | `misaka logs --component node` |
| 機械可読出力 | 各コマンドの `--output json` |

したがって検証専用の別スクリプトや、秘密鍵を引数に渡す手順は不要です。`verifier setup` と `verifier start` は同じ `mining.toml` を共有しますが、verifier 起動時は採掘フラグを付けず panel のみを起動します。

## 関連ページ

- [Testnet-11 ノード運用](testnet11-node-operator.md)
- [Testnet-11 参加・採掘](testnet11-join-mining.md)
- [PALW public classes runbook](palw-public-testnet-classes-runbook.md)
