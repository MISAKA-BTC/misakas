# misaka-probe 使い方メモ

このドキュメントは、`scripts/misaka-probe.sh` をVPSに入れて、IPからMISAKA node / DNS seeder / validator状態を確認するための手順です。

## できること

`misaka-probe` で確認できることは以下です。

| 確認項目 | IPだけで確認できるか | 内容 |
|---|---:|---|
| node参加 | かなり可能 | `26211/tcp` に到達できるか、seedに載っているか |
| DNS seeder動作 | 可能 | `53/udp` と `53/tcp` が複数Aレコードを返すか |
| local service状態 | 可能 | VPS上の `systemctl` と `misaka node doctor` を見る |
| validator登録状態 | bondが必要 | `--stake-bond <txid>:0` があれば確認可能 |
| validator実稼働の完全証明 | IPだけでは不可 | validatorはIPではなく `validator_id` / `stake bond` で識別されるため |

## 重要な前提

このスクリプトは読み取り専用です。

以下は行いません。

- node再起動
- DNS seeder再起動
- validator起動
- bond作成
- 秘密鍵読み取り
- 秘密鍵出力

つまり、試しても他の参加者に基本的な悪影響はありません。

## VPSへインストール

VPS上で、repoが `/opt/misakas` にある前提です。

```bash
cd /opt/misakas
install -o root -g root -m 0755 scripts/misaka-probe.sh /usr/local/bin/misaka-probe
```

確認します。

```bash
misaka-probe --help
```

helpが表示されればOKです。

## 基本コマンド

自分のVPS IPを確認します。

```bash
misaka-probe --ip 217.76.57.217
```

あなたのVPSでは、まずこの形で使えば大丈夫です。

## 出力例

正常な場合は、最後にこのような表示になります。

```text
Verdict
-------
Node verdict                    NODE_OK: reachable and advertised by seed         OK
DNS seeder verdict              DNS_SEEDER_OK                                     OK
Validator verdict               UNKNOWN: IP alone cannot prove validator participation  INFO
```

この場合の意味です。

| 表示 | 意味 |
|---|---|
| `NODE_OK` | nodeとして外から到達でき、seedにも載っている |
| `DNS_SEEDER_OK` | DNS seederとしてUDP/TCP 53で複数peer IPを返している |
| `Validator UNKNOWN` | IPだけではvalidator判定できない |

`Validator UNKNOWN` はエラーではありません。

まだbondしていない場合、または `--stake-bond` を渡していない場合は、この表示が正常です。

## validator bond後の確認

validatorのbond作成後は、`bond_outpoint` を渡します。

```bash
misaka-probe --ip 217.76.57.217 --stake-bond <txid>:0
```

`<txid>:0` は、`kaspa-pq-validator bond` 実行後に表示された `bond_outpoint` に置き換えます。

期待する表示です。

```text
Stake bond                      active                                            OK
Validator verdict               VALIDATOR_REGISTERED_ACTIVE                       OK
```

ただし、この確認で分かるのは「そのbondがregistry上でactive」ということです。

IPとvalidator keyが完全に紐付いていることまでは、IPだけでは証明できません。

## よく使うコマンド集

### 1. 自分のIPを自動取得して確認

`--ip` を省略すると、`api.ipify.org` からIPv4取得を試します。

```bash
misaka-probe
```

明示した方が確実なので、通常は以下がおすすめです。

```bash
misaka-probe --ip 217.76.57.217
```

### 2. network / RPC / seedを明示して確認

```bash
misaka-probe \
  --ip 217.76.57.217 \
  --network testnet-10 \
  --rpc 127.0.0.1:27210 \
  --seed seeder1.misakascan.com
```

### 3. local checkを飛ばして外部確認だけ行う

```bash
misaka-probe --ip 217.76.57.217 --skip-local
```

これは、手元PCや別サーバーから軽く確認したい場合に便利です。

### 4. DNS seederだけを重点確認

```bash
# 通常のリゾルバ経由（これが新規ノードの実際の経路）
dig seeder1.misakascan.com A +short
dig +tcp seeder1.misakascan.com A +short
```

各 `seederN.misakascan.com` は `ns-seederN.misakascan.com` に**委任**されているので、権威サーバ
(`ns1.xdomain.ne.jp` 等) に直接聞いても空が返る。委任先そのものを疑うときは、まず委任先の
アドレスを引いてからそこに聞く。

```bash
dig @ns1.xdomain.ne.jp ns-seeder1.misakascan.com A +short   # → 委任先の IP
dig @<その IP> seeder1.misakascan.com A +short              # → 委任先が答えるか
```

**seeder2 / seeder4 は 2026-09-06 時点で SERVFAIL。** 委任 (`ns-seeder2` → `217.76.57.217`、
`ns-seeder4` → `217.178.101.111`) は生きているが、どちらのホストも :53 に応答しない。
どちらも当プロジェクトが管理していないホストで、既知の状態
(`docs/testnet11-relaunch5-runbook.md` 項目 2)。**答えるのは seeder1 / seeder3 の 2 本**で、
新規ノードの discovery はその 2 本で足りる。

`misaka-probe` の中でも同じ系統の確認をしています。

### 5. P2P portだけを確認

```bash
nc -vz -w 5 217.76.57.217 26211
```

成功例です。

```text
Connection to 217.76.57.217 port 26211 [tcp/*] succeeded!
```

## 判定の意味

### Node verdict

| 表示 | 意味 | 対応 |
|---|---|---|
| `NODE_OK` | P2P到達OK、seedにも掲載 | 問題なし |
| `NODE_REACHABLE` | P2P到達OK、seedには未掲載 | DNS seeder側のpeer収集やseed応答を確認 |
| `NOT_REACHABLE` | P2Pに到達できない | firewall、Contabo panel、UFW、kaspadを確認 |

### DNS seeder verdict

| 表示 | 意味 | 対応 |
|---|---|---|
| `DNS_SEEDER_OK` | UDP/TCP 53で複数Aを返す | 問題なし |
| `PARTIAL_DNS_SEEDER` | UDP/TCPの片方だけ成功 | firewallやDNS seeder serviceを確認 |
| `NOT_A_DNS_SEEDER_OR_NOT_PUBLIC` | DNS seederとして応答していない | 通常nodeなら問題なし。seeder運用なら要確認 |

### Validator verdict

| 表示 | 意味 | 対応 |
|---|---|---|
| `UNKNOWN` | `--stake-bond` がないため判定不可 | bond後に `--stake-bond` を付ける |
| `VALIDATOR_REGISTERED_ACTIVE` | bondがactive | validator registry上はOK |
| `BOND_NOT_ACTIVE_OR_UNKNOWN` | bondがactiveではない、または確認失敗 | `kaspa-pq-validator status` で詳細確認 |
