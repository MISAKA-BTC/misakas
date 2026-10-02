# Quick Start — testnet-12

対象は公開テストネット `testnet-12` です。`misaka`(ADR-0122 の operator CLI)は mining、Bond、work、reward、model、verifier、validator を 1 つの入口から扱います。詳しい手順の正本は [testnet12-join-mining.md](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) です。

## 1. Build

```bash
git clone https://github.com/MISAKA-BTC/misakas.git
cd misakas
git switch main
git pull --ff-only
cargo build --release -p kaspad -p misaka-cli
```

`misaka-cli` から作られるバイナリは `target/release/misaka` です。以下では `target/release` を PATH に加えた前提で書きます。加えない場合は `./target/release/kaspad` と `./target/release/misaka` と読み替えてください。

## 2. Key

```bash
misaka --network testnet-12 key gen --out ~/.misaka/miner.seed      # 0600 で作成。既存ファイルは上書きしない
misaka --network testnet-12 key pubkey --key-file ~/.misaka/miner.seed
misaka --network testnet-12 key address --key-file ~/.misaka/miner.seed
```

**1 つの key で登録できる bond は、チェーンの存続期間を通じて 1 つだけです。** bond を退役させても key は解放されません(`palw_operator_id_unique`)。collateral は後から追加できません。容量を増やしたいときは、新しい key を作って新しい bond を登録します。

## 3. Node

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

node は組み込みの DNS seeder から peer を探します。DNS で見つからない場合は、公開エントリポイントを追加します(`--addpeer` にはホスト名ではなく IP アドレスを書きます)。

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default \
  --addpeer=169.58.232.113:26311
```

起動ログに次の 2 行が出ることを確認します(値の正本はリポジトリの `release.json`)。

```text
Consensus params fingerprint: 254509533bb693ced0fed823a4c25e166ba2542d576e4021b0e4b4d6fe4079e1 (network testnet-12)
Consensus fence schedule: 750, 1000, 1300, 1700, 2000, 3600 (schedule id 1e39c738b97a695c8a2c2d4129660eda8fa7ac5f1e8b529b916314c01750c593)
```

最初の testnet-12(genesis `a8cabac4…`)や旧ネットワークの datadir を使うと、起動時に genesis mismatch で拒否されます。その datadir は削除せずに別名へ退避し、新しい node には使わないでください。

`misaka` は何も指定しなければ testnet-12 を使います。同じシェルで別のネットワークも扱う場合は、毎回ネットワークを明示するか、次のように testnet-12 に固定します。

```bash
export MISAKA_NETWORK=testnet-12        # または: misaka --network testnet-12 config init
```

## 4. 資金

**Faucet は未定です。** 資金を持っている人からは次のコマンドで送金できます(`--yes` がなければ dry-run)。

```bash
misaka --network testnet-12 wallet send --key-file <key> --to <addr> --amount <MSK> --yes
```

bond collateral は producer で **13,000 MSK 以上**、panel seat で **130,000 MSK 以上** です。同時に持てる claim の本数は collateral に比例します(→ [PALW Participation](PALW-Participation-JA#bond-and-exposure))。このほかに、bond とは別の output として fee float(0.1 MSK 以上)を key のアドレスに置きます。

## 5. Existing Bond

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
misaka --network testnet-12 bond status --key-file ~/.misaka/miner.seed
```

`--bond` の形は key が要りません。既定では Floor に対する exposure を表示します。別の class を診断するときだけ `--class-id <128-hex>` を指定します。

- `registry: REGISTERED`: 正式に登録された PALW bond です。登録し直さないでください(`--palw-register-bond` を再実行しない)。
- `registry: NOT REGISTERED`: ふつうの UTXO、予約された output、または lock された output で、registry の記録ではありません。
- `UNDERSIZED`: 古い weight だけの式より collateral が少ない、という表示です。実際の空きは `exposure_ceiling` と `reserved_exposure` で確認します([FAQ](FAQ-Troubleshooting#registered-なのに-undersized--holding))。

## 6. Setup and start

対話式のセットアップ(推奨):

```bash
misaka --network testnet-12 mining setup
```

ウィザードは node、network、model、key、資金、Bond registry、artifact、capability、fee output を順に確認し、`~/.misaka/mining.toml` を作ります。途中で止まっても、同じコマンドを実行すればチェーンとローカルの状態から再開します。対話なしで実行するときは `--yes` を付けます。

- 登録には `kaspad --palw-register-bond` を使います(operator-possession 署名つき)。
- ウィザードは `--palw-bond-collateral` を渡しません。ノードが導出した既定の collateral(古い weight だけの式。Floor で約 31,191 MSK、`--model` に 8k を指定すると調達できない額)を登録し、その額に合う資金を求めます。額を自分で決めるとき、および model class のときは [参加手順 §5](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md#5-register-a-bond) の手動の形で `--palw-bond-collateral=<sompi>` を明示してください。

### Floor

既存の key と Bond を指定する場合:

```bash
misaka --network testnet-12 mining setup \
  --model floor \
  --key-file ~/.misaka/miner.seed \
  --bond <registered-bond-txid>:<index>
```

Floor は組み込みの整数 class なので artifact は要りません。`kaspad` を手動で起動する場合は `--palw-producer-class` と `--palw-class-artifact` を省きます。

### 8k モデル

8k の Qwen2.5 class(`Qwen/Qwen2.5-1.5B/graph-v7@8192`)を生産するときは、artifact を指定します。artifact の作り方は [参加手順 §6](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md#6-start) にあります。

```bash
misaka --network testnet-12 mining setup --model <class-id> --artifact /path/qwen25-1.5b-a16-8k.palwart
```

`mining start` は `--palw-host-memory-share` を渡しません。8k の seat には 3.5 GiB 以上(`--palw-host-memory-share=3758096384`)が要るので、`~/.misaka/mining.toml` の `[advanced] extra_kaspad_args` に書いてください。

### Start

起動コマンドを確認してから起動します(`--detach` でバックグラウンド、`--service` で systemd / launchd の unit を表示)。

```bash
misaka --network testnet-12 mining start --print-command
misaka --network testnet-12 mining start
```

起動ログに `PALW duties (on by construction; …)` の行が 1 行出ます。この行に、動いている義務と、止まっている義務がある場合はその理由が書かれます。producer の node(`--palw-producer-key` と `--palw-producer-bond` を持つ node)は、panel seat の義務と execution lane の round block を**常に**実行します。オフにするフラグはありません。`--palw-produce` で決まるのは、この node が attempt claim も開くかどうかだけです。

## 7. Observe

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

Final になった claim の報酬はすぐには払われません。まず vesting 行に記録され、その行が成熟して動いたときに mint されます(ADR-0152)。`rewards` はこれを `vesting` / `moved` として表示します。

### Dashboard

```bash
misaka --network testnet-12 dashboard --listen 127.0.0.1:8791
```

同じホストで [http://127.0.0.1:8791](http://127.0.0.1:8791) を開きます。リモートの VPS では、外部に公開する bind をせず、SSH tunnel を使います。gateway の既定 `8790` と dashboard の `8791` は別のサービスです。

```bash
ssh -L 8791:127.0.0.1:8791 user@server
```

## 8. Stop

```bash
misaka --network testnet-12 mining stop --drain
```

防御が終わっていない claim がある間は、ふつうの停止は拒否されます。`--drain` は新しい work を止め、開いている claim、panel、court の義務が終わるまで process を動かし続けます。`--force` は claim を失う可能性がある緊急停止です。testnet-12 では消えた producer は課金されます([PALW Participation](PALW-Participation-JA#停止と課金))。

## やってはいけないこと

- `kaspa-pq-miner` や `misaminer` を testnet-12 の PALW producer として使わない(attempt envelope を作れない)。
- `REGISTERED` になっている key で `--palw-register-bond` を再実行しない。
- **同じ bond を 2 つの process で動かさない。** producer の bond で `misaka verifier` を別に起動することも含みます([Operations Notes](Operations-Notes#1-bond--1-process))。
- app dir を消して再同期しない。最後に署名した round の記録(`palw-round-last-signed`)が消え、二重署名の原因になります。移すか復元してください。
- Bond collateral と fee outpoint に同じ outpoint を使わない。
- Floor に 8k artifact や `--palw-producer-class` を付けない。
- block 数、blue score の深さ、DAA の差で入金を確定とみなさない([公開ノート §3](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md))。
