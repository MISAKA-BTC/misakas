# testnet-12 を node なしで掘る

自分の PC に `kaspad` を置かなくてよい。現行 `main` をビルドし、下の公開 wRPC に接続する。接続先の正本は `169.58.232.113`（`wallet.misakascan.com`）と、quorum 用のもう 1 台 `169.58.39.220`（`llm.misakascan.com`）です。P2P の `26311` は node を立てる人の入口で、node なしの miner は使いません。

参加ビルドは現行 `main` です。公開 fleet は commit `247ade1f9`（rev `247ade1f90b6`）で、fingerprint は `ef379378f09bf33f29e97854bbee157a8f86fa1dd8248e1a847f08c5c6e7f15e`、fence schedule の末尾は `15500` です。この手順はその chain に載ります。faucet は未整備（TBD）です。bond の資金は、資金を持っている参加者からの送金で用意します。

```bash
git clone https://github.com/MISAKA-BTC/misakas.git
cd misakas
git switch main && git pull --ff-only
cargo build --release -p misaka-cli -p kaspad --bin palw-attempt-exec \
  -p misaka-palw-remote --features rpc --bin palw-remote-miner --bin palw-evidence
```

`palw-remote-miner --network testnet-12` は `--rpc` を省くと、次の 2 本へ接続します。

| ホスト | 接続する URL（Borsh。miner はこちら） | JSON |
|---|---|---|
| `169.58.232.113` / `wallet.misakascan.com` | `wss://wallet.misakascan.com/t12/wrpc/borsh` | `wss://wallet.misakascan.com/t12/wrpc/json` |
| `169.58.39.220` / `llm.misakascan.com` | `wss://llm.misakascan.com/t12/wrpc/borsh` | `wss://llm.misakascan.com/t12/wrpc/json` |

## 公開 wRPC

上の表が接続先です。`misaka node endpoints --network testnet-12` と同じ一覧で、定義は `misaka/endpoints` の `testnet12_public_wrpc_v1` です。2 台は別ホストです。remote miner は両方に聞き、`virtual_daa` の差が `max_daa_skew`（12）を超えたら止まります。1 台だけでは起動しません。

### 公開する RPC

remote miner と node なし登録が使う読み書きです。

- 読み: `getBlockDagInfo`、`getBlock`、`getBlockTemplate`、`getPalwProducerFacts`、`getPalwStateProof`（op 202）、`getPalwForkChoiceOpening`（op 203）、登録・carrier の状態を読む PALW RPC
- 書き: `submitBlock`、transaction / 登録 carrier の submit

### 公開しないもの

- node gRPC と EVM RPC は各ホストの `127.0.0.1` のままです。
- `Ban`、`AddPeer`、接続一覧、停止などの管理操作は、この公開入口の対象外です。

入口は wRPC の WebSocket だけです。1 IP あたり同時接続は 16 本までです。`submitBlock` と carrier submit もこの接続枠を使います。メソッド名で落とすプロキシは、この段では Borsh フレームの前に置いていません。管理系を外に出さない境界は「gRPC を公開しない」ことです。

## 公式 executor

`palw-attempt-exec` は `PALW_ANCHOR` を受け、producer と同じ `for_node_v1` → `resolve` → `job_for_anchor` → `execute` で roots と material を出します。

```bash
# floor（PALW-BASE-0、class f1c5635c…）。artifact は要りません。
PALW_ANCHOR=<128 hex> palw-attempt-exec --material-out "$STATE/capture.bin"

# 8k。class ebf44d0a…、inventory root 88096dc1…、artifact は fleet と同じ qwen25-1.5b-a16-8k.palwart。
PALW_ANCHOR=<128 hex> palw-attempt-exec \
  --class ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01 \
  --artifact-root 88096dc177826d880c1c5fca4ec93cffe5ab51af108ed169a8e03cd4726308f91263f79f81904b043327bfa277e3558b1656f14259f6dd33603a9f91871aae20 \
  --artifact /path/qwen25-1.5b-a16-8k.palwart \
  --material-out "$STATE/capture.bin"
```

stdout は `trace_root`、`output_root`、`execution_root`、`trace_manifest_root`、`trace_chunk_count`、`material_file` の JSON です。

## material

採掘の接続先は上の wss です。material の publish 先は `http://169.58.232.113:8088`（`.113` の `palw-evidence serve`）で、`palw-evidence publish` は `--providers` を省くとこの URL へ送ります。公開は 1 台なので、`status` の本数判定は `--min-copies 1` です。

fleet の panel は `--palw-evidence-provider-dir` にディレクトリと `http://` / `https://` を受けます。manifest が claim の roots と一致したときだけ、そのチャンクを producer からの取得と同じ検証に渡します。provider が無い claim は、これまでどおり producer へ取りに行きます。

material を provider に置き終わるまで、採掘した PC は止めません。置いたあとの再取得は provider 側です。

## checkpoint

`palw-remote-miner --checkpoint <daa>:<block hash>` は、利用者がピンする信頼起点です。このバイナリは署名付き checkpoint ファイルをまだ読みません（`--checkpoint-trust pinned`）。`getBlockDagInfo` の `virtualDaaScore` とその sink hash を、自分で確認して渡します。

`palw-fork-choice-issuer` の公開鍵と attestation を既定の信頼起点にする配布は、L2 の fence が dormant な間は検証表示を上げません。

## node なしの手順

producer floor は **13,000 BILI** です。claim ごとの担保は、その class の予約で、bond の残高が同時に持てる本数を決めます。panel seat は **130,000 BILI** です。carrier（登録、receipt、readiness、court 応答）には bond とは別の fee float（0.1 BILI 以上）が要ります。

1. 現行 `main` をビルドする（`docs/testnet12-join-mining.md`）。
2. `misaka --network testnet-12 key gen --out ~/.misaka/miner.seed`
3. 資金をそのアドレスへ送る。faucet は TBD なので、ここは既存の参加者が送ります。
4. bond 登録は node なしの export / sign / submit です。submit の relay は上の公開 wRPC です。
   `misaka bond register-export`、署名、`submit --relay`。
5. class を宣言する。自分で足すモデルは `misaka model add --export-bundle`、署名、submit です。floor は genesis の class です。
6. checkpoint を `getBlockDagInfo` から取り、`palw-remote-miner` を 2 台の wRPC、bond、class、artifact root、受取アドレス、鍵、`palw-attempt-exec`、state dir で起動する。
7. 当選した attempt だけが署名され、両方の node へ `submitBlock` されます。block を追い、Panel が material を検証して claim が `Final` になるまで state dir を残します。
8. 報酬は `misaka bond status` と wallet で見ます。同じ公開 wRPC を `--rpc` に渡せます。

## 将来の fence

- `palw_fork_choice_commitment_v1` は公開ネットワークでは dormant です。node なしの表示は、ピンした checkpoint までの `UNVERIFIED_REMOTE_STATE` と、op 202 で開いた範囲です。L2 まで含めた `VERIFIED_REMOTE` は、この fence が有効になるまで出ません。有効化の高さはまだありません。
- `palw_receipt_spend_v4` は testnet-12 の int-13 リストに入り、**DAA 15,500** で有効になります。それ以降、free-prompt の V4 公開 redemption を node なしの carrier として扱えます。mainnet など他のネットワークでは、この fence は出ません。
