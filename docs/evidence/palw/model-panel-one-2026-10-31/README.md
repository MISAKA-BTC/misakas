# 全モデル G14 Panel=1 — 2026-10-31 検証記録

初回配布の状態（2026-10-31 12:39 UTC）: 実装・検証・Linuxバイナリ配布済み。95 relayとb0〜b7のunitを移行版へ更新し、復旧・同期を進めている。t12 の移行 fence は DAA **18,640**。**実モデルclaimのpanel=1・Finalは確認中**。mainnet 等は無効のまま。
[RFC-0042](../../../rfc/0042-all-model-g14-panel-one.md) / [ADR-0210](../../../adr/0210-all-model-g14-panel-one.md)。


## 18,660再停止の修正（2026-10-31 UTC / 11月1日 JST）

公開95のPALW pruning snapshotは46分24秒の再構成後に自然完了し、
公開95/b6で18,660→18,677以上への継続前進を確認した。
b0/b1では起動時の同期pruning復旧中に4つの処理workerが未起動なのに、
同期済み・producer drawingが先行していた。詳細は
[停止原因と復旧ログ](pruning-18660-stall.md)。

共有PALW lockの範囲短縮、検証済checkpointが近い場合の短経路選択、
キャンセル/import世代確認、worker起動前の参加holdを実装した。
[Snapshot 9件](pruning-capture-short-anchor-test.log)、
[起動readiness 1件](pruning-startup-readiness-test.log)、
[owned-state guard 1件](pruning-owned-state-caller-test.log)が成功。
新バイナリのビルド・配布と、実Qwen claimの公開受理・panel=1・Finalは継続確認中。
以下の初回配布版のSHAを、新しい実行コードのSHAとして扱わない。

## 現在の配布版

- [最新Release](https://github.com/MISAKA-BTC/misakas/releases/tag/testnet12-g14-panel1-18640): GitHubのlatest APIで公開・latest指定と3 assetsのuploaded状態を確認。
- 初回Release tag: `8b19b4a66fa0aa7ac3cd2ed0a41aa3cdbf47742e`。
- バイナリのimmutable build source: `8cac52ec4ed15fb4ea982d46235a5c3b5f78e4cc`。初回Release tagの実行コードとの差分なし（文書・release metadata・cfg(test) fixture/assertionのみ）。
- 現行params: `edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189`。
- 現行schedule: `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04`。
- Linux x86_64 GNU archive: 54,238,819 bytes、SHA-256 `0d0f53aa40e615e8412060e2a5b13acf1d575259a69ffe3c0a52f51a36cda7c9`。glibc 2.39以上・libstdc++が必要。
- 展開後の`bin/kaspad`: 155,485,528 bytes、SHA-256 `e6d638a4f2a9fa32b6072c32ad5ce929f015efa189b721ceee2be5a9aa52e83e`。

公開APIのasset digestはローカルarchive/manifest/checksumと一致。認証用秘密鍵・モデルartifact・DB・運用configは配布物に含まない。
旧候補のrawログは以下に履歴として残す。Gate Cの認証・発行完了を今回の配布やlocal replayから主張しない。

## 配備候補の基点

稼働中の `2ca68baee2c9` に実装を重ねる。main の `6e31509bb` に含まれる別件の terminal service / operational release 修正は持ち込まない。
過去の状態遷移を変更したバイナリへ同時更新しないためであり、その修正自体の採否の変更ではない。
元の main 基点の実装はローカル branch `codex/g14-model-panel-one-main-base` に保存した。

## 撤回した18,600候補（未配備）

未配備だった18,560候補を撤回し、DAA **18,600**へ延期した。Linuxビルド中に追加したnative TIR ORIGINAL回帰で、1席の初回割当後、無応答からの再抽選が固定済みcustody契約と衝突して後続block全体を拒否する不具合を再現した。
[再現ログ](tir-original-redraw-reproduction.log) / [修正後ログ](tir-original-redraw-fixed.log)。再抽選先へ契約を移さず、当該claimをPanelUnavailableで非fraud終了する。producer collateral不変、元custody保持、delta revert、cold restoreを確認した。

- params: `3d33cad25caeada14dd729bf622c122b6832e94d50891d5a0662ca2c8aadc350`
- schedule: `f55a70399992d223d92b61c35f38012919024ef291edb5722026d66640a17508`
- handshake / kernel policy: `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5`（変更なし）

[18600-regression.log](18600-regression.log): **302 passed / 0 failed**。TIRの既存公開court・PanelDa・Final/支払・虚偽承認の回帰も含む。
[18600-node-check.log](18600-node-check.log): P2P応答分離とpending resync再開を取り込んだ候補で、`cargo check -p kaspad --features evm --all-targets`成功。Linux releaseビルド・配備・実モデル運用は未確認。

## 最初の未配備候補（撤回）

ユーザー指定の全モデル移行を段階配備するため、稼働チェーンの先に DAA 18,560 を設定した。数値は配備上の境界であり、過去の kernel fence 18,400 は変更しない。

- params: `ee8ec945ba47dd384711156adb8d06aa76f7039dca10b2c187e056b9f327a328`
- schedule: `825a2d3f62d0a04531b68eac001daf7ec51b2a18b1c5b76bc4859c67149a97b9`
- handshake / kernel policy: `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5`（変更なし）

既存 release の constructor は新しい fence を明示的に解除し、既存 identity を保存する。境界前後、manifest と schedule、Held かつ ready=0 のモデルを含む全経路の [release-regression.log](release-regression.log) は **283 passed / 0 failed**。

## 実行結果

[core-regression.log](core-regression.log): **123 passed / 0 failed**。新経路と関連する既存 Panel・R-core・vertex・経済条件・params の回帰試験。

```sh
RUST_MIN_STACK=33554432 RUST_TEST_THREADS=2 \
CARGO_BUILD_JOBS=2 cargo test -p kaspa-consensus-core --lib \
  --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.test.package.kaspa-consensus-core.debug=0' -- \
  model_panel_one palw_panel_v2 rcore_m4_sampled_and_v3_door \
  palw_vertex_v1 palw_weight_cap_v1 palw_user_status_v1 \
  palw_g14_gates_v1 palw_economic_safety_v1 config::params::tests --nocapture
```

このコマンドではコンパイル時間を抑えるため core の最適化を無効にし、非最適化時の大きな既存 fixture に合わせてテストスレッドの stack を 32 MiB にした。
指定なしの非最適化実行では既存の maturity fixture が stack overflow した。protocol の上限や判定を緩めたものではない。

[node-check.log](node-check.log): 稼働中の基点上で `cargo check -p kaspad --features evm --all-targets` **成功**。
文書検査は新規不備 0、既知の未解決参照 43。baseline は変更していない。

移行高さを設定した候補でも [release-node-check.log](release-node-check.log) の全 target 型検査に成功。ローカル main は `c4810c02d` に fast-forward し、作業中の Explorer・文書編集は保持した。公開リモート main の更新とは区別する。

## 確認した境界

- commit 自身の anchor では抽選しない。1 個の独立 bond から 1 席を割り当てる。commit 後に登録した bond はその抽選に入れない。
- 移行後に登録された別モデルも同じ経路。旧 claim の 5 席と新 claim の 1 席が同じ fold に混在できる。
- 全 scope の承認、実際の ML-DSA-87 署名・context、署名不正・重複・mask 不正の拒否。
- licence tag 4 / basis 1 / served mask 1、実際の担保予約、Final、delta apply / revert、carriage の cold restore。
- 1 席の lock は全 gain の価格。Final 前後の虚偽承認では lock と既存 S4 action が徴収される。
- Panel=1 Final だけでは Gate C の weight / frontier を得られない。future fence は旧 handshake / kernel policy identity を維持し、params / schedule identity は変わる。

## 未確認

この記録の unit fixture は claim / ledger の境界を試す小さい構成であり、1.5B・8K の実計算、Linux release 実行、fleet の切り替え、実際の公開 TIR court の全手順、実運用の検出率・応答率を測った結果ではない。
Linux 用ビルドと配備検証は別途記録する。これらの pass を配備完了や `SecurityCertified` と読み替えない。

## 追加のrelease整合性検査

[SDK cache identity](18600-sdk-cache-test.log)は1件成功。[旧int13 constructor](historical-v7-test.log)は新fenceを引き継がず、過去のparamsのvalidationが成功。旧G14 isolated sandboxも同じ歴史基準を使う。
既存のrelease re-pin toolでRustから値を再算出し、現行t12/copyの64 pinsを更新した。26件のhistoryは更新せず、mainnet・testnet-11・v22 goldenは不変。新規fenceを意図的に持たない旧経路の負のfixtureを明示化し、新経路の必須依存guard自体は維持した。元ログ・fleetの旧identity記録は保存した。

## Linux配備前の復旧準備（10:36 UTC）

95.111.236.186を正常停止（exit 0）して取得した、meta・consensus・utxoindexを含む同一時点のバックアップを使用する。
これは自分の検証済みnodeのoffline backupであり、独立した全履歴再実行の結果ではない。
archiveは7,438,164,368 bytes、SHA-256 `2a4af052174ebc554929fa2673a53f4c81f831d6718019f925578182fc2e002d`。
IBM受信側でもSHA-256一致と`zstd -t`成功を確認した。転送専用の一時鍵とauthorized_keysの許可行は削除済み。

b1は停止後、元fork DB・以前のresync DB・今回の途中DBをすべて別名で保持し、展開済みbackupをdatadirへrenameした。
`palw-resync-report.json`はstatus `syncing`のままbytes不変（SHA-256 `e8cd99af98a7ed667ae13eadca3743c2cf4d884295d0749ccf9809980a880e1a`）。
旧binaryの再起動ではholdを失うため、旧SHAに対するlauncher guardを追加し、exit 78を実行確認した。
まだ新binaryで起動していない。native pending-resync再開と、実際の2 outbound peersによる確認を必要とする。reportのconfirmed偽装やGate Cの代替には使わない。

node producerの追加点検で、authoritative producer factsとは別の`registry_holds_class`が旧Held/5席roomを再び要求することを発見した。
候補blockのDAAがmodel-one境界以降は、この歴史的な補助チェックを起動しない。factsが持つclaimのclass cap・共有network room・予算・担保の判定は維持する。

## 現在の18,640候補

18,600候補はLinux最適化ビルドとfleetの起動・同期確認の時間を確保するため、未配備のまま撤回した。
新規claimの移行高さのみ18,640へ延期する。18,400と履歴claimの規則は変えない。旧候補のraw logとidentityは上に保存した。
旧registryのrowは履歴として返すが、新経路が有効な時点では旧registryをadmission authorityとしてRPCでactive表示せず、clientが旧jury・5席案内を再生成しない。
このRPCのinactive registry回帰8件と、fleet identity回帰4件は成功。新候補のidentityと配備記録は別途追記する。

18,640候補の再計算値: params `edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189`、schedule `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04`。handshake / kernel policyは`5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5`で不変。
18,600候補の最終pin検証はlib 186件、integration 47対象237件が成功（明示的ignored 2件）。これは18,640候補の未実施テストを代替するものではない。

[18,640 pin lib](18640-pin-lib.log): 186 passed。
[18,640 pin integration](18640-pin-integration.log): 48対象241 passed / 0 failed / 明示的ignored 2件。
[18,640 node型検査](18640-node-check.log): `kaspad --features evm --all-targets`成功。
[registry RPC回帰](registry-rpc-tests.log): 8 passed。
Linux build対象sourceは`8cac52ec4ed15fb4ea982d46235a5c3b5f78e4cc`で固定し、build中にsourceを変更しない。

## Remote main integration and Linux artifact (2026-10-31)

The remote main at `5ba94e47e` is merged without rewriting history. Its test-only
TIR material-supplier fixes (`da97d6a4a`, `d5fbc11f0`) are retained. The unfenced
operational collateral-release change in `6e31509bb` is deliberately deferred:
it changes historical fold/reconstruction semantics and was never deployed to
the public `2ca68baee` fleet. Its associated new semantic test is deferred with
that change. This merge retains the already-tested G14 source behavior instead;
a future collateral-release fix requires its own activation and historical-state
compatibility verification. This is an explicit integration decision, not a
claim that the underlying collateral-release issue is resolved.

The production Linux x86_64 artifact was built from immutable source
`8cac52ec4ed15fb4ea982d46235a5c3b5f78e4cc`, with the standard release profile
and `pq-only,evm` features. It is 155,485,528 bytes; SHA-256:
`e6d638a4f2a9fa32b6072c32ad5ce929f015efa189b721ceee2be5a9aa52e83e`.
The 18,640 fence, `edf47229…` params and `4024a6e7…` schedule are the current
release identities. Earlier 18,560/18,600 candidate metadata above is historical.
The release manifest and SDK cache-key assertion are corrected to the 18,640
identities. Test and release-metadata integration changes do not constitute a
rebuild of that immutable binary. Fleet installation and actual model-claim Final
must be recorded separately after observation.
