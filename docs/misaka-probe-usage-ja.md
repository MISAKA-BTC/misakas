# misaka-probe 使い方メモ

このドキュメントは、`scripts/misaka-probe.sh` をVPSに入れて、IPからMISAKA node / DNS seeder / validator状態を確認するための手順です。

**testnet-12 では `--network testnet-12 --p2p-port 26311` を必ず付けてください。** スクリプトの既定値は
退役済みの testnet-10(`--network testnet-10`、P2P `26211`)のままです。testnet-12 の P2P port は
`26311` です(`consensus/core/src/network.rs` の `default_p2p_port`)。環境変数
`MISAKA_NETWORK` / `MISAKA_PROBE_P2P_PORT` でも指定できます。

## できること

`misaka-probe` で確認できることは以下です。

| 確認項目 | IPだけで確認できるか | 内容 |
|---|---:|---|
| node参加 | かなり可能 | P2P port(testnet-12 は `26311/tcp`)に到達できるか、seedに載っているか |
| DNS seeder動作 | 可能 | `53/udp` と `53/tcp` が複数Aレコードを返すか |
| local service状態 | 可能 | VPS上の `systemctl`(`misaka-kaspad` / `misaka-dnsseeder` / `misaka-validator`)と `misaka node doctor` を見る |
| validator登録状態 | bondが必要 | `--stake-bond <txid>:0` があれば確認可能 |
| validator実稼働の完全証明 | IPだけでは不可 | validatorはIPではなく `validator_id` / `stake bond` で識別されるため |

## 重要な前提

このスクリプトは読み取り専用です。

行うのは TCP connect 確認、DNS query、systemd status 確認、`misaka node doctor`、
`kaspa-pq-validator status` だけです。以下は行いません。

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
misaka-probe --ip <VPS_IP> --network testnet-12 --p2p-port 26311
```

まずこの形で使えば大丈夫です。以下の例では `--network testnet-12 --p2p-port 26311` を省略していますが、
毎回付けてください。

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
misaka-probe --ip <VPS_IP> --stake-bond <txid>:0
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
misaka-probe --ip <VPS_IP>
```

### 2. network / RPC / seedを明示して確認

```bash
misaka-probe \
  --ip <VPS_IP> \
  --network testnet-12 \
  --p2p-port 26311 \
  --rpc 127.0.0.1:27210 \
  --seed seeder1.misakascan.com
```

### 3. local checkを飛ばして外部確認だけ行う

```bash
misaka-probe --ip <VPS_IP> --skip-local
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

node に組み込まれている seed は `seeder1`〜`seeder4.misakascan.com` の 4 本です。
`misaka-probe` の既定の `--seed` は `seeder1.misakascan.com` です(スクリプトのコメントでは
seeder2 / seeder4 は当プロジェクトが管理していないホストに委任されていて応答しない、とされています。
現在の状態は上の `dig` で確認してください)。

`misaka-probe` の中でも同じ系統の確認をしています。

### 5. P2P portだけを確認

```bash
nc -vz -w 5 <VPS_IP> 26311
```

成功例です。

```text
Connection to <VPS_IP> port 26311 [tcp/*] succeeded!
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

## 今後: Rust CLI 化の案

このスクリプトは、`misaka` CLI に入れる前に運用確認の UX を試すための PoC です。`misaka node probe` は
まだ実装されていません。取り込む場合は次の形が自然です。

```bash
misaka node probe --ip <ipv4>
misaka node probe --ip <ipv4> --stake-bond <txid>:0
```

- `misaka-cli/src/main.rs` の `NodeCmd` に `Probe` を追加し、`misaka-cli/src/node.rs` に実装する。
- P2P 到達性は `TcpStream::connect_timeout` で見る。
- 既知 peer の判定は `getPeerAddresses`、接続中 peer の判定は `getConnectedPeerInfo` を使う。
- validator 判定は `--stake-bond` がある場合だけ `getStakeBond` を使う。`--validator-id` を足すなら
  `getStakeBonds` をページングして探す。
