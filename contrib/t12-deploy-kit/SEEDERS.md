# t12 regenesis: DNS seeder の切替手順

2026-09-23 に作成。調べた tree は `wt-t12` @ `5a559459`（feat/testnet-12-regenesis）。ホストは 4 台とも**読み取りのみ**で調べた（15:28〜15:38 CEST）。この手順書と scripts はどれもまだ実行していない。例外は読み取り専用の `00-preflight.sh` と `40-verify.sh` で、スクリプトが動くか確かめるために一度ずつ走らせた。

## 1. 結論: seeder は新 genesis に合わせて作り直す必要がない

- **再ビルドは不要、フラグの変更も不要。** 4 台とも今の `misaka-dnsseeder-t12`（sha `1174b965…`）のままにする。`fleet.env` の `SEEDER_SHA256=KEEP` と同じ判断。
- 切替のときに seeder 側でやる作業は次の 2 つだけ。
  (a) node を切り替えた**後で**検証する: `40-verify.sh` と、DNS だけで参加できるかを見る `60-join-check.sh`。
  (b) anchor の構成を変えると決めた場合に限り、`30-swap.sh --anchors`（§6 Q1）。

### 1.1 コードから見た根拠（`misaka-dnsseeder/src/main.rs`、479 行）
- genesis・`Params`・`consensus_params_id` をまったく読まない。依存しているのは `kaspa-consensus-core` の `NetworkId` だけ（P2P port と wRPC port の既定値を出すため）と、`misaka-endpoints`、wRPC Borsh クライアント。
- P2P の handshake をしない。anchor に対しては `TcpStream::connect((ip, 26311))` で 3 秒以内に繋がるかを見るだけで、繋がったらすぐ閉じる。**genesis が違っても判定は変わらない。**
- 4 台とも `--anchors-only` で動いている。このモードで co-located node に wRPC で投げるのは `get_server_info` だけで、しかも best-effort（失敗しても anchor は配る）。`get_connected_peer_info` と `get_peer_addresses` は呼ばない。
- `--network-id testnet-12` から決まる P2P port は **26311**（`network.rs:286` の `Some(11) | Some(12) => 26311`）。port の設定は `5a559459` でも同じ。
- 初回配備のビルド `de857a71` から `5a559459` までの間に、`misaka-dnsseeder/`・`misaka-endpoints/`・`consensus/core/src/network.rs`・`rpc/core/src/api/ops.rs` の差分はない。`rpc/core/src/model/message.rs` の差分は `RpcPalwClaimRow`・`RpcPalwLocalPanelClass`・`GetPalwNodeStatusResponse` だけで、seeder が使う 3 つの型には触れていない。
- node の DNS 名の一覧（`palw_rc_base_params` → t12 が継承）は `seeder{1..4}.misakascan.com` で、`consensus_params_id` には含まれない。新 genesis でも名前は変わらない。

### 1.2 実測での根拠
- 4 台とも `1174b965…` がこのビルドのバイナリで動いていて（`/proc/<pid>/exe` と on-disk の sha が一致）、再起動回数 `NRestarts=0`、2026-09-22 21:59 CEST から連続稼働している。
- ログに `:26311` が出ている（seeder3 の `NONE of the 2 anchors dial on :26311`、他 3 台の `anchor 169.58.39.220:26311 is not reachable`）。旧 seeder が `:26411` を見てしまう問題（memory `old-dnsseeder-advertises-26411-for-t12`）は、このバイナリでは**起きない**。
- seeder は状態をディスクに持たない（DB なし、peer 集合はメモリに持って 30 秒ごとに取り直す）。そのため、regenesis で消したり再起動したりするものは seeder 側にない。

## 2. 実測した構成（2026-09-23 15:28〜15:38 CEST）

| 名前 (scripts) | ホスト | 待受 | unit / 設定の置き場所 | バイナリ | DNS 委任 | 同じホストの公開 node | 返している A |
|---|---|---|---|---|---|---|---|
| `c5104` | 5.104.81.23 | 5.104.81.23:53 | `misaka-dnsseeder-t12`、`EnvironmentFile=/etc/default/misaka-dnsseeder-t12` | `/usr/local/bin/misaka-dnsseeder-t12` | **なし** | なし | 169.58.232.113 |
| `seeder3` | 95.111.236.186 | 95.111.236.186:53 | unit ファイルの ExecStart に直書き。`AmbientCapabilities=CAP_NET_BIND_SERVICE` | `/root/misaka-dnsseeder-t12` | `seeder3.misakascan.com` | なし（node を持たない。egress が絞られている） | 169.58.232.113, **169.58.39.220** |
| `ibm` | 169.58.39.220 | 169.58.39.220:53 | EnvironmentFile（c5104 と同じ形） | `/usr/local/bin/misaka-dnsseeder-t12` | **なし** | **あり**（`misaka-t12-node1`、**:26321**） | 169.58.232.113 |
| `seeder1` | 169.58.232.113 | 169.58.232.113:53 | ExecStart に直書き（`After=misaka-t11-node.service` が残っている。害はない） | `/usr/local/bin/misaka-dnsseeder-t12` | `seeder1.misakascan.com` | **あり**（`misaka-t12-node`、**0.0.0.0:26311**） | 169.58.232.113 |

4 台に共通のフラグ: `--network-id testnet-12 --anchors 169.58.232.113,169.58.39.220 --anchors-only --node-wrpc-borsh 127.0.0.1:26313`。26313 で待ち受けているのは .113 の t12 kaspad だけ。他の 3 台のログでは `Connection refused` になるが、anchors-only なので実害はない。
どのホストにも、以前のバイナリ `misaka-dnsseeder`（`b773b81a…`）と古い `.bak*` がそのまま残っている。t11 用の unit（`misaka-dnsseeder-t11`、`misaka-dnsseeder`）は disabled になっている。

### DNS の委任（親ゾーン `ns1.xdomain.ne.jp` に直接聞いた結果）
| 名前 | NS | glue A | 実際の応答 |
|---|---|---|---|
| seeder1.misakascan.com | ns-seeder1 | 169.58.232.113 | 答える |
| seeder2.misakascan.com | ns-seeder2 | **217.76.57.217** | **タイムアウト**（管理外のホスト） |
| seeder3.misakascan.com | ns-seeder3 | 95.111.236.186 | 答える |
| seeder4.misakascan.com | ns-seeder4 | **217.178.101.111** | **タイムアウト**（管理外のホスト） |

NS と glue の TTL は 3600 秒、seeder が返す A の TTL は 30 秒。kaspad は 4 つの名前を**並列に**引き、返ってきた IP には **既定 port 26311** で接続する（`connectionmanager::dns_seed_many`）。つまり実際に効いている名前は 2 つだけで、c5104 と ibm の seeder は誰からも引かれていない。

## 3. 今の構成の弱点（切替の前後で同じ。直すかどうかはユーザーが決める）
1. **既定 port で入れる入口が .113:26311 の 1 か所しかない。** ibm の node は :26321 で待ち受けているので、A レコードで配っても 26311 では繋がらない。
2. **seeder3 は、死んでいる 169.58.39.220:26311 を毎回配っている。** seeder3 は anchor のどれにも接続できない（egress が絞られている）。どれにも繋がらないときは「全部配る」側に倒れる作りなので、このホストでは port の判定がまったく効いていない。新しく参加するノードは ibm への接続に失敗し、.113 だけに繋がる（遅くなるが、別のチェーンに繋がることはない）。
3. 4 つの DNS 名のうち 2 つ（seeder2 と seeder4）は、管理外の IP に委任されていて応答しない。
4. .113 が止まると、既定 port で入れる入口がなくなる。
5. ibm はディスクが 95% 使用（残り 16 GB）。seeder とは関係ないが、node 切替で新しい appdir を作るときに問題になる。

## 4. node 切替との順序

| 段階 | seeder 側の作業 | 理由 |
|---|---|---|
| 0. いつでも | `./seeders/00-preflight.sh`（読み取りのみ） | 現状を記録するため |
| 1. node 切替の前 | **何もしない。** 再ビルドを選んだ場合も `20-stage.sh` で置くだけで、有効にはしない | seeder は genesis に依存しない |
| 2. node 切替の最中（node 側の kit が担当） | **seeder は止めない、触らない** | 公開 node が落ちている間は、TCP の判定で外れるか、全部配って接続に失敗させるかのどちらかになる。genesis が違う node に当たっても、handshake が `Genesis mismatch` または `Consensus params mismatch` で拒否する（`protocol/p2p/src/common.rs`）。別のチェーンと peer になることはない |
| 3. .113 の新 node が :26311 で待ち受け、heartbeat が進み始めた後 | `./seeders/40-verify.sh` → `CONFIRM=yes ./seeders/60-join-check.sh <release kaspad on 5.104>` | DNS だけで新しい chain に届くことを、行動の後の証拠で確かめる |
| 4. **公開の告知は段階 3 に合格してから** | — | 告知の前に旧 t12 のユーザーが .113 に来ると `Genesis mismatch` で拒否される。これは想定どおりの挙動 |
| 5. 決めた場合だけ | `30-swap.sh <name> --anchors …` または `--binary …` を 1 台ずつ、c5104 → seeder3 → ibm → seeder1 の順に | 影響の小さい順。seeder1 と seeder3 を同時に再起動しない |

守らなければならない前提は 1 つだけ: **公開の入口になる node は `0.0.0.0:26311` で待ち受けること。** これが崩れると、seeder を変えなくても新しい参加者が入れなくなる。node の構成を切り替える側で必ず確認すること。

node 切替を rollback した場合（旧 t12 の unit に戻す場合）も、seeder は何も変えなくてよい。IP も port も同じなので、そのまま使える。

## 5. scripts（`deploy-t12/seeders/`。書き込む script は `CONFIRM=yes` を付けないと動かない）

| script | ホストに書き込むか | 内容 |
|---|---|---|
| `lib-seeders.sh` | — | 実測した構成表（上の順序）、ssh の設定、`fleet.env` の**読み込み**（`SEEDER_SHA256`、`EXPECT_FP`） |
| `00-preflight.sh [name]` | 書かない | unit・cmdline・running と on-disk の sha・anchors・直近のログ・ディスク、各 seeder への直接問い合わせ（UDP と TCP）、親ゾーンの委任、公開 resolver、Mac から :26311 に繋がるか |
| `10-build-seeder.sh` | 書かない（Mac の中だけ） | **任意。** Mac で `cargo zigbuild --release --locked -p misaka-dnsseeder --target x86_64-unknown-linux-gnu.2.35`（toolchain 1.93.0 の linux target、zig 0.16、cargo-zigbuild があることは確認済み。ネイティブ依存は `cc` と `ring` だけ）。空きが 15 GB 未満ならビルドせずにコマンドだけ表示する（今の空きは 19 GB）。`CARGO_TARGET_DIR=scratchpad/target-seeder-linux` にまとめるので、後から丸ごと消せる。zigbuild で ring が通るかは**試していない** |
| `20-stage.sh <name> <bin>` | `<bin path>.new-<sha8>` を置くだけ | sha を照合する。公開 node のないホスト（c5104、seeder3）では loopback で smoke test をする（TEST-NET の anchor で `dial on :26311` が出ること、`:26411` が出ないこと、DNS の待受が上がること）。公開 node のホストでは smoke をしない |
| `30-swap.sh <name> [--binary <sha256>] [--anchors A,B]` | 書く | バイナリと設定を `*.bak-t12seed-<TS>` に退避してから、バイナリは横に置いて rename で入れ替える（動いているファイルには書き込まない）。env の行か `--anchors` の値を 1 つだけ置き換え、daemon-reload と restart をしてから、`40-verify` を「その時刻より後」の証拠で走らせる |
| `40-verify.sh [name] [epoch] [sha]` | 書かない | active か、sha、`--network-id testnet-12` と `--anchors-only`、指定時刻の後に `verified peer set refreshed` が出ているか、`:26411` が出ていないか、UDP で空でない応答が返るか（FAIL 扱い）、TCP の応答（WARN 扱い）、配っている IP の :26311 に繋がるか（WARN 扱い）。**今の 4 台で PASS した** |
| `50-rollback.sh <name> <TS>` | 書く | `.bak-t12seed-<TS>` のバイナリと設定を戻し（バックアップは消さない）、restart して verify する |
| `60-join-check.sh <kaspad>` | 5.104 に appdir とログを置く | 段階 3 の e2e 確認。5.104 で release の kaspad を `--addpeer` も `--connect` も付けず、127.0.0.1 だけに bind し、palw 系のフラグなしで 240 秒動かす。見るのは次の点: 指紋が `EXPECT_FP` と一致するか、`Retrieved N addresses from DNS seeder`、outbound peer ができるか、IBD が進むか、mismatch 系のメッセージ |

再ビルドを選ぶ場合も、ビルドを 1 回増やさずに済む方法がある。node の release は `misaka-dnsseeder` も一緒に作る — Mac の `build-release-local.sh`（推奨、PLAN §13）なら `.cache/<rev12>/misaka-dnsseeder` がそのまま、5.104 の `build-release-5104.sh`（フォールバック）ならその成果物を Mac に取ってきて、`20-stage.sh` に渡せばよい。この場合は `fleet.env` の `SEEDER_SHA256` にその sha を入れる。

## 6. ユーザーに決めてもらうこと
- **Q1 anchor 構成。** 新しい t12 で ibm の公開 node を :26311 に移すかどうか。
  - 移す場合: anchor は今のままでよい（4 台すべてで ibm も配られるようになる）。
  - 移さない場合: 全台の anchor を `169.58.232.113` だけにするのを勧める（`30-swap.sh <name> --anchors 169.58.232.113`）。こうすれば seeder3 が死んだ入口を配らなくなる。
- **Q2 seeder2 と seeder4 の委任。** xdomain の管理画面で、`ns-seeder2` の A を 5.104.81.23 に、`ns-seeder4` の A を 169.58.39.220 に向け直すかどうか。向け直すと、動いているのに使われていない seeder 2 台が有効になる。ただしレジストラ側の操作で、反映には最大 1 時間かかる。ibm は公開 node のホストでもある（seeder のメモリ使用量は数 MB）。
- **Q3 provenance。** seeder も release と同じ commit でビルドし直すかどうか。
  - 動作上は必要ない。
  - 揃える場合は、5.104 の release ビルドに相乗りする形にすれば追加のビルドは 0 回で済む。
- **Q4 軽微な修正。** seeder1 の unit にある `After=misaka-t11-node.service` を `misaka-t12-node.service` に直すかどうか。起動順の問題だけで、動作への影響はない。

## 7. 公開を止める要因
- **seeder 側には、公開を止める要因はない。** 今のバイナリと設定のまま、新 genesis の node を配れる。
- 公開の前に必要なのは、node を切り替えた後の `40-verify.sh` PASS と `60-join-check.sh` PASS。`EXPECT_FP` と release の kaspad のパスは node 側の kit で決まる。
- 公開は止めないが、落ちると困る点: 既定 port で入れる入口は .113 の 1 か所だけ（§3-1、§3-4）。
