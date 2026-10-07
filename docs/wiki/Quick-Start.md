# Quick Start — testnet-12

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

対象は公開テストネット `testnet-12` です。ビルドする commit は 2026-09-27 の 2 回目の post-launch flag day のリリース `587cab2b0` です。2 本の fence が DAA 1,300(2026-09-28 05:25 JST 前後)で有効になるので、それより前に動かしてください。詳しい手順の正本は [testnet12-join-mining.md](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) です。

## 1. Build

```bash
git clone https://github.com/MISAKA-BTC/misakas.git
cd misakas
git switch main
git pull --ff-only
git checkout 587cab2b0
cargo build --release -p kaspad -p misaka-cli
```

`misaka-cli` から作られるバイナリは `target/release/misaka` です。以下では `target/release` を PATH に加えた前提で書きます。加えない場合は `./target/release/kaspad` と `./target/release/misaka` と読み替えてください。

公開 fleet は 2026-09-27 19:37 JST に `587cab2b0` の x86_64 Linux release build へ入れ替えました(kaspad の sha256 は `84b8c8c931de24ed57a9197b26e11692989ce4c15e2029c8e99aef0121926f2e`)。DAA 1,300 で 2 本の fence が有効になり、`c3dbaee3c` 以前のノードは DAA 1,300 から handshake で拒否されます。古いビルドのまま DAA 1,300 を越えたノードは `<appdir>/misaka-testnet-12/datadir` を退避し、`587cab2b0` で同期し直します(公開ノート §000)。

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
Consensus params fingerprint: 24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff (network testnet-12)
Consensus fence schedule: 750, 1000, 1300 (schedule id d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a)
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
| Floor producer | **13,000 BILI 以上**。同時に持てる floor claim 1 本につき約 6,402 BILI(13,000 BILI なら 2 本、100,000 BILI なら 15 本) |
| Panel seat | **130,000 BILI 以上** |
| DNS finality の validator | **20,000,000 BILI 以上**(別の bond) |

このほかに、bond とは別の output として fee float(0.1 BILI 以上)を key のアドレスに置きます。

## 5. Existing Bond

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
misaka --network testnet-12 bond status --key-file ~/.misaka/miner.seed
```

- `registry: REGISTERED`: 正式に登録された PALW bond です。登録し直さないでください。
- `registry: NOT REGISTERED`: ふつうの UTXO、予約された output、または lock された output で、registry の記録ではありません。
- `UNDERSIZED`: 古い weight だけの式(Floor で約 31,191 BILI)より collateral が少ない、という表示です。testnet-12 では 13,000 BILI 以上なら生産でき、同時に持てる本数が collateral に比例するだけです(下の表)。

## 6. Floor producer

対話式のセットアップ(推奨):

```bash
misaka --network testnet-12 mining setup
```

ウィザードは `kaspad --palw-register-bond` で登録します。collateral にはノードが導出した既定値(Floor で約 31,191 BILI。`--model` に 8k を指定すると約 2,000,332,625 BILI と調達できない額)を使い、その額に合う資金を求めます。額を自分で決めたい場合と model class の場合は、[参加手順 §5](https://github.com/MISAKA-BTC/misakas/blob/main/docs/testnet12-join-mining.md) の手動の形で `--palw-bond-collateral=<sompi>` を明示してください。

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
