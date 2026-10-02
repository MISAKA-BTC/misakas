# Operations Notes — testnet-12

## Network and ports

| 用途 | 値 |
|---|---|
| network | `testnet-12` |
| node flag | `--testnet --netsuffix=12` |
| P2P | `26311` |
| gRPC | `127.0.0.1:26210` |
| wRPC Borsh | `127.0.0.1:27210` |
| wRPC JSON | `127.0.0.1:28210` |
| EVM JSON-RPC | `8545` |
| dashboard | `127.0.0.1:8791`(gateway は `8790`) |
| 公開エントリポイント | `169.58.232.113:26311`(DNS seeder `seeder1.misakascan.com` と同じホスト) |

RPC は原則 loopback のままにして、リモートからの操作は SSH tunnel で行います。

## Minimal node

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

DNS seeder で peer が見つからない場合は `--addpeer=169.58.232.113:26311` を追加します。同期と監視だけの node には、model artifact、producer key、Bond は要りません。

## Identity の確認

起動ログの次の 2 行を、リポジトリ直下の `release.json`(`consensus_params_fingerprint`、`consensus_schedule_id`)と比べます。

```text
Consensus params fingerprint: 254509533bb693ced0fed823a4c25e166ba2542d576e4021b0e4b4d6fe4079e1 (network testnet-12)
Consensus fence schedule: 750, 1000, 1300, 1700, 2000, 3600 (schedule id 1e39c738b97a695c8a2c2d4129660eda8fa7ac5f1e8b529b916314c01750c593)
```

上は 2026-10-02 時点の値です。post-launch fence が追加されると両方が変わります。

## Updating

1. `git pull --ff-only` のあと release build を作る。
2. 新旧のバイナリの SHA-256 を記録する。
3. node を止め、旧バイナリを名前を付けてバックアップする。
4. バイナリを入れ替え、**同じ app dir** で起動し直す。
5. fingerprint、fence schedule、peer、同期、サービスの状態を確認する。

既知の問題(公開ノート §2)は、公開後に fence またはノードの更新で直します。更新の告知があったら、fence の高さより前に上の手順でバイナリを入れ替えてください。古いビルドのノードは、その高さから handshake で拒否されます。

最初の testnet-12(genesis `a8cabac4…`)や旧ネットワークの datadir は genesis が違うので使えません。削除せずに別名へ退避します。バイナリを更新するだけのときは、現在の app dir をそのまま使います。

**app dir は消さないでください。** app dir には、この bond が最後に署名した round の記録(`palw-round-last-signed`)があります。消して再同期すると、まだ merge されていない permit にもう一度署名し、bond が slash される可能性があります。ホストを移すときは app dir ごと移すか、復元します。

## 1 bond = 1 process

testnet-12 の node の義務には止めるスイッチがありません。bond の key と outpoint を持って起動した `kaspad` は、どれもその bond の義務(execution lane の round block を含む)を実行します。2 つが同時に動くと、同じ round permit に 2 つの別の block で署名し、bond 全体が slash されます(`RoundPermitEquivocated`)。

次のものはすべて 2 つ目の process です。

- standby の node、別のホストにコピーした node
- 動いている node の横で起動した `kaspad --palw-register-class …`(class が登録されても終了しない)。class は `misaka model add` で登録するか、動いている node をそのフラグ付きで起動し直す
- 古いものが動いたまま、新しい app dir で起動した同じ node

node は起動時にこの規則を表示します(`PALW bond …: run it in exactly ONE process …`)。

## Service health

```bash
systemctl is-active <service>
systemctl --failed
journalctl -u <service> -n 100 --no-pager
misaka --network testnet-12 doctor
misaka --network testnet-12 mining status
```

ログでは panic のほかに、次を確認します。

- fingerprint mismatch、genesis mismatch、fork-id mismatch
- `PALW duties NOT as planned: …`(義務が予定どおり動いていない)
- `NOT PRODUCING for N min — holding: <reason>`
- `[palw-lane-watch] no PALW work block …`(heartbeat だけで時計が進んでいる)
- bond の exposure が満杯、fee UTXO、artifact mismatch
- 8k の seat の readiness: `no proof —`、`waits for a carrier slot`(公開ノートの既知の問題 6。詰まった seat は再起動で回復させる)

## Resource profiles

`kaspad --help` の `--node-profile` が現行の入口です。

- `full`
- `bootstrap-pruned`
- `recovery-sync`
- `validator`
- `archive`
- `public-rpc`

profile が拒否する組み合わせを、手動のフラグで回避しないでください。model class の producer や seat には、artifact そのものの RAM、ディスク、実行時間が必要です(8k は `--palw-host-memory-share` 3.5 GiB 以上)。Floor はモデルのダウンロードが要りません。

## Keys and state

- producer と panel seat の key と設定をバックアップする。
- secret seed を CLI の引数やログに貼らない。
- 同じ validator seed を 2 か所で起動しない。同じ bond を 2 つの process で動かさない。
- `~/.misaka/mining.toml` と app dir の役割を混同しない。
- Bond の outpoint、fee outpoint、class artifact の hash を記録する。genesis の bond と fee float は premine txid `5e0d5f1b…` の上にある(index は bond 0〜7、fee float 41〜48)。

## Graceful maintenance

producer を止める前に:

```bash
misaka --network testnet-12 mining stop --drain
```

panel や court の義務がある場合は、drain が終わるまで待ってからサービスを止めます。testnet-12 では、消えた producer は課金されます([PALW Participation](PALW-Participation-JA#停止と課金))。
