# 公開裁定 P0 — 作業ツリーの ruleset とライブの印刷

- 状態: ライブ fingerprint の欠測を、この作業ツリーと 2026-11-07 の read-only SSH で埋めた記録である。目標は完了していない。P2–P5 も完了していない。出荷 preset は武装しない。
- This is not a T12 upgrade.

## 作業ツリー

HEAD は `2afa6ccf47efca499fc95fb7f3bb1468698c7837` である。この記録を書く直前の `git status --short` は dirty である。他の作業者の変更を含む。このファイル自体はその一覧にはない。

```
 M consensus/core/src/lib.rs
 M consensus/core/src/palw_batch_court_v1.rs
 M consensus/core/src/palw_court_clock_v2.rs
 M consensus/core/src/palw_court_shared_tir_fold_v1.rs
 M consensus/core/src/palw_positive_court_v2.rs
 M consensus/core/src/palw_positive_court_v2/coordinator_recovery.rs
 M consensus/core/src/palw_tir_frame_cost_v1.rs
 M consensus/core/src/palw_tir_public_allocation_v1.rs
 M consensus/core/src/palw_tir_public_claim_v1.rs
 M consensus/core/src/palw_tir_public_fold_v1.rs
 M docs/design/palw/consensus/README.md
 M docs/maintenance/catalog.md
 M docs/maintenance/decision-catalog.json
 M docs/maintenance/numbering.md
 M docs/rfc-v2/README.md
 M docs/rfc-v2/registry.json
 M docs/rfc/INDEX.md
 M docs/rfc/TOPICS.md
 M kaspad/src/palw_tir_cone_owned_consumer_tests_v2.rs
 M kaspad/src/palw_tir_owner_replay_owned_tests_v2.rs
 M kaspad/src/palw_tir_public_filer.rs
 M kaspad/src/palw_tir_public_owner.rs
 M kaspad/src/palw_tir_source_worker_owned_tests_v2.rs
 M kaspad/src/palw_tir_source_worker_owned_v2.rs
 M misaka/palw/tir/exec/src/node/evidence.rs
 M misaka/palw/tir/exec/src/node/run.rs
?? consensus/core/src/palw_batch_public_budget_v1.rs
?? consensus/core/src/palw_public_adjudication_measured_class_v1.rs
?? docs/design/palw/consensus/public-adjudication-p4.md
?? docs/design/palw/consensus/public-adjudication-policy-v0.1.md
?? docs/design/palw/tir/evidence/
?? docs/evidence/
?? docs/rfc/0047-batch-court-time-contract.md
```

## コード側の ruleset id

`palw_t12_shipped_params()`（`consensus/core/src/config/params.rs`）の `ConsensusV2` bundle を、この作業ツリーのまま `palw_ruleset_id_v2`（`consensus/core/src/palw_mode_v2.rs`）に渡して計算した。

`palw_ruleset_id_v2=df72ba12d1804d441997e12402f303c4cc149b8d851cd06f44dbc61a4cab59d137c1d492b9afc267b0d5c002cfbdae99533ab9240247bc832f2e189377d92063`

これはホストがログに出す `Consensus params fingerprint` ではない。そちらの行は下に、印刷された文のまま書く。

## ホスト

すべて BatchMode の短い read-only である。4 台とも SSH は応答した。SSH エラーはない。プロセスを止めたり、上げたり、入れ替えたりしていない。

ハッシュは、その時点で動いていたプロセスの実行ファイルに対する `sha256sum` である。fingerprint は、そのプロセスが実際に出した行だけを書く。出ていないものは書かない。

### `ssh -o BatchMode=yes misaka-ibm`

- 状態: up。hostname `vmi3450148`。
- kaspad: pid 2980499。開始 `Fri Oct 30 15:20:48 2026`。`--palw-host-pinner`。
- 実行ファイル: `/root/t12-rel/2ca68baee2c9/bin/kaspad`
- sha256: `0a8d81f996c5d0abaa376113035f452bf42aa67c33d4fa12a76925ee9693acf3`
- 印刷: なし。開始から 60 秒の `journalctl -u misaka-palw-pinner.service` と、その pid の最新 5 行は空だった。ruleset fingerprint は出ていない。

### `ssh -i /Users/wata/.ssh/claude_key -o BatchMode=yes root@169.58.232.113`

- 状態: up。hostname `vmi3527497`。
- kaspad が 5 つ動いている。

| pid | 実行ファイル | sha256 | このプロセスが印刷した行 |
| --- | --- | --- | --- |
| 945978 | `/root/t12-rel/2ca68baee2c9/bin/kaspad` | `0a8d81f996c5d0abaa376113035f452bf42aa67c33d4fa12a76925ee9693acf3` | なし。開始 `Fri Oct 30 18:37:19 2026` から 60 秒の journal は pin のログだけで、`Consensus params fingerprint` はない |
| 1274421 | `/root/t11/kaspad` | `0dd188f907351ecc59d61b63c960d87f11b58fc47d3ce110930fbd72ecee1e75` | `2026-10-31 18:00:46.504+01:00 [INFO ] Consensus params fingerprint: 79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640 (network testnet-11)` |
| 1274428 | `/root/t11/kaspad` | `0dd188f907351ecc59d61b63c960d87f11b58fc47d3ce110930fbd72ecee1e75` | `2026-10-31 18:00:45.950+01:00 [INFO ] Consensus params fingerprint: 79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640 (network testnet-11)` |
| 1274436 | `/root/t11/kaspad` | `0dd188f907351ecc59d61b63c960d87f11b58fc47d3ce110930fbd72ecee1e75` | `2026-10-31 18:00:46.034+01:00 [INFO ] Consensus params fingerprint: 79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640 (network testnet-11)` |
| 1274445 | `/root/t12-rel/8cac52ec4ed1/bin/kaspad` | `e6d638a4f2a9fa32b6072c32ad5ce929f015efa189b721ceee2be5a9aa52e83e` | `2026-10-31 18:01:02.121+01:00 [INFO ] Consensus params fingerprint: edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189 (network testnet-12)` |

pid 1274421、1274428、1274436、1274445 の開始は `Sat Oct 31 18:00:18 2026` である。上の行は、その開始のログに出た最新の `Consensus params fingerprint` である。

### `ssh -i /Users/wata/.ssh/claude_key -o BatchMode=yes root@5.104.81.23`

- 状態: up。hostname `vmi3272359`。
- kaspad: pid 1261170。開始 `2026-10-30 15:39:23`。`--palw-host-pinner`。
- 実行ファイル: `/root/t12-rel/2ca68baee2c9/bin/kaspad`
- sha256: `0a8d81f996c5d0abaa376113035f452bf42aa67c33d4fa12a76925ee9693acf3`
- 印刷: なし。開始から 60 秒の `journalctl -u misaka-palw-pinner.service` と、その pid の最新 5 行は空だった。ruleset fingerprint は出ていない。

### `ssh -i /Users/wata/.ssh/claude_key -o BatchMode=yes root@95.111.236.186`

- 状態: up。hostname `vmi3225101`。
- kaspad: いない。`ps -C kaspad` はプロセスを返さなかった。
- kaspad の binary hash: なし。
- kaspad の ruleset fingerprint: なし。印刷されていない。
- 同じホストで pid 849 の `kaspa-pq-validator`（`/root/kaspa-pq-validator.a1e6602e`、`--network testnet-10`）は動いている。これは kaspad ではないので、その hash も fingerprint もここには書かない。
