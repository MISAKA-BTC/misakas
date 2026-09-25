# Testnet-11 検証参加ガイド

> [!WARNING]
> **このページは旧ネットワーク testnet-11 向けの記録です。** 現行の公開テストネットは `testnet-12` です(2026-09-25/26 JST 公開、release commit `0e8ec984e`)。testnet-12 の手順は **[Testnet 12 Verification Participation](Testnet-12-Verification-Participation-JA)** を見てください。
> 現行 `main` のビルドは testnet-11 に参加できません。testnet-11 を続ける場合は commit `1f98d3bf4` をビルドし、fingerprint `79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640` を確認してください。このページの fingerprint、fence、公開エントリポイント(`169.58.39.220:26311`)は当時の値で、現在は正しくない場合があります。

このページは、Testnet-11 の PALW 検証席（panel / verifier）としてネットワークに参加するための手順です。検証席は、他の参加者が提出した PALW claim を再実行し、checkpoint の結果をチェーンへ提出します。

検証席はブロックを採掘せず、現行ネットワークでは検証そのものに対する直接報酬はありません。一方で、claim を Final へ進めるための重要な役割です。

対象は **current `main` / Testnet-11** です。別の network id や consensus fingerprint のノードは参加できません。

## 最短手順

### 1. 必要なもの

- Linux または macOS の常時稼働できるホスト
- `kaspad` を実行できるディスク、メモリ、安定したネットワーク
- `--utxoindex` を有効にした Testnet-11 ノード
- 検証対象クラスに対応した artifact（Floor のみを検証する場合は不要）
- 対象クラスの capability を宣言した PALW Bond と、その鍵

検証席は Bond が宣言したクラスだけを検証します。すでに採掘用 Bond がある場合は、その Bond を検証席にも使えます。秘密鍵はコマンドラインへ書かず、鍵ファイルの権限を `0600` にしてください。

### 2. ビルド

```bash
git switch main
git pull --ff-only
cargo build --release -p kaspad -p misaka-cli
```

CLI は `target/release/misaka`、ノードは `target/release/kaspad` に出力されます。以降の `misaka` は、必要に応じて `./target/release/misaka` に読み替えてください。

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
./target/release/kaspad \
  --testnet --netsuffix=11 \
  --utxoindex \
  --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311
```

起動ログで、現在の fingerprint と fence schedule を確認してください。

```text
Consensus params fingerprint: ae1d61628da50c7becea62f0a8f08c8654d190c60b2e104df0010b121ba4d3d8
Consensus fence schedule: 1150, 1900, 2150, 2400, 3500, 4000, 6900, 7000, 2125000
```

### 4. 検証席をセットアップする

既存の Bond を自動検出できる場合は、これが推奨手順です。

```bash
misaka --network testnet-11 verifier setup
```

対話なしで実行する場合は `--yes` を追加できます。既存 Bond と artifact を明示する場合は次の形です。

```bash
misaka --network testnet-11 verifier setup \
  --model floor \
  --key-file "$HOME/.misaka/miner.seed" \
  --bond <bond-txid>:<index> \
  --peer 169.58.39.220:26311
```

LLM クラスを検証する場合は、先に現在のクラスを確認し、対応する artifact を渡します。

```bash
misaka --network testnet-11 model list
misaka --network testnet-11 verifier setup \
  --model <model-name-or-class-id> \
  --artifact /absolute/path/to/class-artifact \
  --key-file "$HOME/.misaka/miner.seed" \
  --bond <bond-txid>:<index>
```

`verifier setup` は中断しても再実行すれば、完了済みの項目を再利用して続きから進みます。Bond の登録や資金移動など支出を伴う操作は、`--yes` を指定しない限り確認を求めます。

### 5. 検証席を起動する

まず、実行予定のコマンドを確認します。

```bash
misaka --network testnet-11 verifier start --print-command
```

問題がなければバックグラウンドで起動します。

```bash
misaka --network testnet-11 verifier start --detach
```

このモードではノードに `--palw-panel` が付与され、`--palw-produce` は付与されません。検証だけを行い、採掘プロセスを二重起動しません。S1 / S2 / S3 も **この同じ起動** です。段階ごとの別プロセスや `--s1` のようなフラグはありません。詳細は下の「S1 / S2 / S3 の panel 起動」を見てください。

## S1 / S2 / S3 の panel 起動

S1・S2・S3 は別のサービスではありません。上の `verifier setup` → `verifier start` で panel を1つ起動し、チェーンの DAA が fence を超えた claim から席の検証の仕方が切り替わります。オペレータが決めた順は **V1（全再実行）→ S1（区間 replay）→ S3（層サンプル）→ S2（optimistic）** です。ゼロ知識証明は使いません。

| 段階 | Testnet-11 DAA | 席の動き | 起動時に足すフラグ |
|---|---:|---|---|
| V1 | 7,200 未満 | 5席とも job 全体を再実行。3席の `Valid` で license | なし（従来の panel） |
| **S1** | **7,200** | 1席が全区間（full-replay）。残り4席は重複しない1区間だけ。producer は決定論的 checkpoint `SC01` を出す。partial は checkpoint から自分の区間だけ replay し、割り当てどおりの mask と receipt を提出。genesis からの全再実行は full 席のみ | なし。**現行バイナリ**で同じ `verifier start` |
| **S3** | **8,600** | S1 のうえに、partial が層・位置サンプルでも検証する | なし |
| **S2** | **8,700** | full 席の `Valid` だけで optimistic licence 可。collector は coverage → optimistic → V1 の順 | なし |

### 検証席の起動（S1 / S2 / S3 共通）

手順は「最短手順」と同じです。

```bash
misaka --network testnet-11 verifier setup
misaka --network testnet-11 verifier start --print-command
misaka --network testnet-11 verifier start --detach
```

`verifier start --print-command` に `--palw-panel` があり、`--palw-produce` が無いことを確認します。

起動前に次を満たすビルドを使います。

- `git switch main && git pull --ff-only` のあと `kaspad` と `misaka` を再ビルドする
- 起動ログまたは `misaka doctor` の `Consensus fence schedule` に **7200, 8600, 8700** が含まれる
- fingerprint は [Home](Home) および `doctor` と一致する（このページに残っている古い例示値をコピーしない）

tip の DAA が 7,200 を超えているのに、席が毎回 job 全体を genesis から再実行しているなら、古いバイナリです。再ビルドしてください。

### Producer 側（S1 の前提）

S1 の partial 席は producer が公開する `SC01` checkpoint から resume します。producer も **同じ世代のバイナリ** で起動します。checkpoint 用の別コマンドはありません。`mining start` が capture から `SC01` を決定論的に出します。

```bash
misaka --network testnet-11 mining setup
misaka --network testnet-11 mining start --print-command
misaka --network testnet-11 mining start
```

producer ノードは receipt 期限まで capture を保持し、席からの checkpoint / opening 要求に応えます。partial 席は checkpoint が開けないとき **待って receipt を出しません**。genesis から全区間を歩き直しません。

Producer の参加手順の全体は [PALW 参加手順](PALW-Participation-JA) です。

### 5席の割り当て（S1）

panel は 5席、quorum は 3席です。割り当ては bind の `(anchor, claim, seat_count)` から決まり、席が自分で区間を選びません。

- 1席: 全区間を検証（full-replay）
- 残り4席: 互いに重複しない1区間
- license: すべての区間が少なくとも2つの `Valid`（full 席 + その区間の partial）で覆われ、かつ `Valid` が3席以上
- 欠落・重複・誤った checkpoint / root / class / mask の receipt は拒否される
- Court も告発された区間を同じ checkpoint から resume する
- CanonicalWork / payout / quanta は検証の分割では変わらない

自分が full 席か partial 席かは抽選結果です。`verifier status` と node ログを見てください。

### 動作確認

```bash
misaka --network testnet-11 doctor
misaka --network testnet-11 verifier status
misaka --network testnet-11 logs --component node
```

S1 以降の正常ログの例:

```text
[palw-panel] claim …: licensed by V2 segment resume — mask 0x… (ADR-0133 S1)
```

DAA が fence 未満なら、その段階は動きません。S3 を 8,600 より前に、S2 を 8,700 より前に先走らせる設定はありません。

## 状態・検証結果の確認

### 参加前後の診断

```bash
misaka --network testnet-11 doctor
misaka --network testnet-11 --output json doctor
```

`doctor` は、node の fingerprint、fence schedule、同期状態、peer、Bond、artifact、ホスト資源を確認します。JSON 出力は監視・スクリプトへ組み込めます。

### 自分の検証席の状態

```bash
misaka --network testnet-11 verifier status
misaka --network testnet-11 verifier status --output json
```

ここでは Bond の登録状態、宣言したクラス、panel の稼働状況、現在座っている claim、提出期限を確認できます。`paid: false` は、現行仕様で検証席が直接報酬を受けないことを表します。

### claim 単位の追跡

```bash
misaka --network testnet-11 work list
misaka --network testnet-11 work show <claim-id>
misaka --network testnet-11 work why <claim-id>
misaka --network testnet-11 logs --component node
```

正常な検証席では、他ノードからの opening / checkpoint 要求、再実行、panel receipt の提出がログに現れます。claim がまだ表示されないことは異常とは限りません。対象はチェーン上の claim と seat 抽選で決まります。

## 参加できないとき

| 表示・症状 | 原因 | 対処 |
|---|---|---|
| fingerprint mismatch | 古いバイナリまたは別 relaunch | `main` から再ビルドし、現在の datadir を継続して起動 |
| fence schedule mismatch | 現行 fence を含まないビルド | `git pull --ff-only` 後に `kaspad` と `misaka` を再ビルド |
| `0 peers` | DNS、firewall、または peer の ruleset 不一致 | `--addpeer=169.58.39.220:26311`、P2P `26311/tcp`、ログの fork-id を確認 |
| `bond unknown` | outpoint の誤り、または別ネットワーク | `bond status --bond <txid>:<index>` で正確な outpoint を確認 |
| `judges nothing` | Bond がクラス capability を宣言していない | `verifier setup --model ...` を実行。登録済み Bond は再登録せず、表示内容を確認 |
| artifact missing / mismatch | クラスに対応しない、またはパスが誤り | `model list` でクラスを再確認し、絶対パスを指定 |
| panel が動かない | node が同期中、または起動引数に `--palw-panel` がない | `verifier start --print-command` と `doctor` を確認 |
| fence schedule に 7200 / 8600 / 8700 が無い | S1 / S3 / S2 を含まないビルド | `git pull --ff-only` 後に `kaspad` と `misaka` を再ビルドし、`doctor` で schedule を確認 |
| 席が毎回 job 全体を genesis から再実行する | 古い panel、または full 席の duty | DAA ≥ 7200 なら現行バイナリか確認。full 席だけ全再実行するのは正常 |
| `SC01` が来ない / partial が待ち続ける | producer が古い、または capture を保持していない | producer も現行バイナリで `mining start`。receipt 期限まで capture を残す |
| receipt が mask / checkpoint / root / class で拒否される | 割り当て以外の区間を証明した、または不正な opening | 席は割り当て区間だけ replay する。producer の `SC01` と class artifact を合わせる |
| claim が長時間 pending | chain tip の進行、seat の稼働、producer の遅延、または S1 の coverage 不足 | `verifier status`、`work show`、node log、peer 数を確認。S1 は full 席と各区間の partial が揃う必要がある |

## 安全な停止

```bash
misaka --network testnet-11 verifier stop
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
| claim の追跡 | `misaka work list`, `show`, `why` |
| ログ確認 | `misaka logs --component node` |
| 機械可読出力 | 各コマンドの `--output json` |

検証専用の別スクリプトや、秘密鍵を引数に渡す手順は不要です。`verifier setup` と `verifier start` は同じ `mining.toml` を共有しますが、verifier 起動時は採掘フラグを付けず panel のみを起動します。

## 関連ページ

- [PALW 参加手順](PALW-Participation-JA)
- [Testnet-11 Operator UI](Testnet-11-Operator-UI-JA)
- [Quick Start](Quick-Start)
- [FAQ / Troubleshooting](FAQ-Troubleshooting)
- [Testnet-11 ノード運用（リポジトリ）](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet11-node-operator.md)
- [Testnet-11 参加・採掘（リポジトリ）](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet11-join-mining.md)
- [検証参加ガイド（リポジトリ）](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet11-verification-participation-ja.md)
