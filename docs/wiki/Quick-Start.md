# Quick Start — testnet-12

対象は公開テストネット `testnet-12` です。ビルドする commit は 2026-09-27 の post-launch fence release `c3dbaee3c` です。13 本の fence が DAA 750(2026-09-27 07:29 JST)で有効になりました。詳しい手順の正本は [testnet12-join-mining.md](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) です。

## 1. Build

```bash
git clone https://github.com/MISAKA-BTC/misakas.git
cd misakas
git switch main
git pull --ff-only
git checkout c3dbaee3c
cargo build --release -p kaspad -p misaka-cli
```

`misaka-cli` から作られるバイナリは `target/release/misaka` です。以下では `target/release` を PATH に加えた前提で書きます。加えない場合は `./target/release/kaspad` と `./target/release/misaka` と読み替えてください。

公開 fleet は 2026-09-27 02:15 JST に `c3dbaee3c` の x86_64 Linux release build へ入れ替えました(kaspad の sha256 は `0233f845aa19a5e1edca47f786f4d45bb6a49d7af9d5f011c77301cc31ad2571`)。DAA 750 で 13 本の fence が有効になり、`0e8ec984e`・`8a0810992` のノードは handshake で拒否されます。古いビルドのまま DAA 750 を越えたノードは `<appdir>/misaka-testnet-12/datadir` を退避し、`c3dbaee3c` で同期し直します(公開ノート §00)。

> 現行 `main` のビルドは testnet-11 に参加できません。testnet-11 を続ける場合は commit `1f98d3bf4` をビルドします。

## 2. Key

```bash
misaka --network testnet-12 key gen --out ~/.misaka/miner.seed      # 0600 で作成。既存ファイルは上書きしない
misaka --network testnet-12 key pubkey --key-file ~/.misaka/miner.seed
misaka --network testnet-12 key address --key-file ~/.misaka/miner.seed
```

**1 つの key で登録できる bond は、チェーンの存続期間を通じて 1 つだけです。** bond を退役させても key は解放されません。collateral は後から追加できません。容量を増やしたいときは、新しい key を作って新しい bond を登録します。

## 3. Node

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

node は組み込みの DNS seeder から peer を探します。DNS で見つからない場合は、公開エントリポイントを追加します(`--addpeer` にはホスト名ではなく IP アドレスを書きます)。

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default \
  --addpeer=169.58.232.113:26311
```

起動ログに次の 2 行が出ることを確認します。

```text
Consensus params fingerprint: dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9 (network testnet-12)
Consensus fence schedule: 750, 1000 (schedule id 7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397)
```

最初の testnet-12(genesis `a8cabac4…`)の datadir を使うと、起動時に genesis mismatch で拒否されます。その datadir は削除せずに別名へ退避し、新しい node には使わないでください。

`misaka` は何も指定しなければ testnet-12 を使います。同じシェルで別のネットワークも扱う場合は、毎回ネットワークを明示するか、次のように testnet-12 に固定します。

```bash
export MISAKA_NETWORK=testnet-12        # または: misaka --network testnet-12 config init
```

## 4. 資金

**Faucet は未定です。** testnet-12 の faucet にはまだ資金が入っていません。資金を持っている人からは次のコマンドで送金できます。

```bash
misaka --network testnet-12 wallet send --key-file <key> --to <addr> --amount <MSK> --yes
```

testnet-12 の bond は mainnet 想定の額です。

| 役割 | 必要な bond collateral |
|---|---|
| Floor producer | **13,000 MSK 以上**。同時に持てる floor claim 1 本につき約 6,402 MSK(13,000 MSK なら 2 本、100,000 MSK なら 15 本) |
| Panel seat | **130,000 MSK 以上** |
| DNS finality の validator | **20,000,000 MSK 以上**(別の bond) |

このほかに、bond とは別の output として fee float(0.1 MSK 以上)を key のアドレスに置きます。

## 5. Existing Bond

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
misaka --network testnet-12 bond status --key-file ~/.misaka/miner.seed
```

- `registry: REGISTERED`: 正式に登録された PALW bond です。登録し直さないでください。
- `registry: NOT REGISTERED`: ふつうの UTXO、予約された output、または lock された output で、registry の記録ではありません。
- `UNDERSIZED`: 古い weight だけの式(Floor で約 31,191 MSK)より collateral が少ない、という表示です。testnet-12 では 13,000 MSK 以上なら生産でき、同時に持てる本数が collateral に比例するだけです(下の表)。

## 6. Floor producer

対話式のセットアップ(推奨):

```bash
misaka --network testnet-12 mining setup
```

ウィザードは `kaspad --palw-register-bond` で登録します。collateral にはノードが導出した既定値(Floor で約 31,191 MSK。`--model` に 8k を指定すると約 2,000,332,625 MSK と調達できない額)を使い、その額に合う資金を求めます。額を自分で決めたい場合と model class の場合は、[参加手順 §5](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) の手動の形で `--palw-bond-collateral=<sompi>` を明示してください。

既存の key と Bond を指定する場合:

```bash
misaka --network testnet-12 mining setup \
  --model floor \
  --key-file ~/.misaka/miner.seed \
  --bond <registered-bond-txid>:<index>
```

起動コマンドを確認してから起動します。

```bash
misaka --network testnet-12 mining start --print-command
misaka --network testnet-12 mining start
```

起動ログに `PALW duties (on by construction; …)` の行が 1 行出ます。この行に、動いている義務と、止まっている義務がある場合はその理由が書かれます。

## 7. Observe and stop

```bash
misaka --network testnet-12 mining status --watch 5
misaka --network testnet-12 doctor
misaka --network testnet-12 work list
misaka --network testnet-12 rewards
misaka --network testnet-12 palw registry
misaka --network testnet-12 mining stop --drain
```

ブラウザ画面:

```bash
misaka --network testnet-12 dashboard --listen 127.0.0.1:8791
```

[http://127.0.0.1:8791](http://127.0.0.1:8791) を開きます。

## やってはいけないこと

- `kaspa-pq-miner` や `misaminer` を testnet-12 の PALW producer として使わない(attempt envelope を作れない)。
- `REGISTERED` になっている key で `--palw-register-bond` を再実行しない。
- **同じ bond を 2 つの process で動かさない。** standby の node、別ホストのコピー、動いている node の横で起動した `kaspad --palw-register-class`、新しい app dir で起動し直した node は、どれも 2 つ目の process になります。bond 全体が slash されます。
- app dir を消して再同期しない。最後に署名した round の記録(`palw-round-last-signed`)が消え、二重署名の原因になります。移すか復元してください。
- Bond collateral と fee outpoint に同じ outpoint を使わない。
- Floor に 8k artifact や `--palw-producer-class` を付けない。
- block 数、blue score の深さ、DAA の差で入金を確定とみなさない([公開ノート §3](https://github.com/MISAKA-BTC/misakas/blob/main/docs/t12-launch-2026-09-25.md))。
