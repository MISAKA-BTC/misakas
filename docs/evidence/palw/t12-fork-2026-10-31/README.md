# t12 分岐の調査とノード復旧 — 2026-10-31

本記録は公開 testnet-12 の運用調査。本件のノード局所修正は合意規則・パラメーター・モデル係数を変更しない。
`SecurityCertified`、claim ごとの検証者認証、court の要件も変更しない。
別作業の全モデル G14 移行の実装・有効化・配備判断とは区別する。
実機配備には、その別作業の移行版に本件修正を含めた統合バイナリを使用する（下記）。

## 確認した原因

18:20 JST 頃の [9ノードの観測](initial-fleet.json) では全ノードの genesis と
consensus params/schedule ID が一致。稼働 binary SHA-256 は
`0a8d81f996c5d0abaa376113035f452bf42aa67c33d4fa12a76925ee9693acf3`。
複数ノードで先端の遅延があり、`isSynced` だけでは分岐と遅延を区別できなかった。

- IBM b0 は virtual DAA 18,441、sink `7632e597321fcdec…` に停止。
  この sink と selected parent の複数標本は 95 のノードには header 自体がなかった。
- `.113` b6 の sink `d6556768a93b0c3a…` に対する 95 の
  `getVirtualChainFromBlock` は `removedChainBlockHashes=[]`、added 1,482。
  この時点の b6 の先端は 95 の選択チェーンの祖先であり、先端差だけを実分岐と扱わない。
- b0 の [実ログ](b0-reorg-refusals.log) は `DominanceViolation` を明示。
  06:23:28 サーバー時刻には自枝の live 値が候補より高く、06:44:46 には
  shallow window より深い全経済値同率の候補を拒否。09:56:59 には自枝の
  frontier/safe/live が `60244 / 5653974800811264 / 5654247498262575`、
  候補が `60218 / 5651196946866432 / 5651469644317743` で、
  より大きい blue work でも切り替わらない。

直接確認したのは、経済値を保持した枝から既存の deep-reorg 規則で戻れなくなった状態。
最初の伝播分断の発生原因や、同一ブロックについて検証結果が不一致になったことまで
立証したものではない。peer の更新境界不一致、通信 timeout、メモリ逼迫は併存している。

95 の 10:27:11 サーバー時刻の再起動は kernel の
`CONSTRAINT_MEMCG` / `/system.slice/kaspad-t12.service` による OOM kill
（pid 1272695、anon RSS 9,397,984 kB）。ホスト全体の空きや build 用 swap の追加だけでは、
サービス固有のメモリ上限による kill を防げない。

## 修正

`fb3dd18de` は optional IBD candidate summary の返信 queue を、relay の
Block/BlockLocator 返信 queue から分離する。従来は 10 秒の summary timeout を許容した後、
その遅延返信が次の Block 待ちに入り `expected … Block but got … IbdCandidateSummary`
で接続を切っていた。復旧に使える peer をこの誤判定で失わないようにする。

`2766fb9e5` は verified resync の `status=syncing` を通常再起動でも読み戻し、
複数の outbound peer による確認待ちを再設定する。同期途中の DB を再退避せず、
最初の退避先と進捗を維持する。破損・別 DB の report は起動を拒否する。
従来は one-shot flag が消えた後の再起動で、この確認待ちが失われた。

両変更とも node-local。合意の fingerprint、fence、fork-choice の比較規則を変更しない。

## 運用と検証の状態

- このチャットでは IBM b0 のみを停止・復旧の担当とし、別チャットは b1 と他 fleet、
  候補 binary の build/配備を担当する。同時に同じノードを操作しない。
- b0 を停止し、元の約 9.6 GB の DB を保持。停止は既存の 180 秒 timeout まで drain
  できず systemd が終了させた。ホストの MemAvailable は約 17.7 GB に改善。
- b1 の再同期は別担当が進行。18:35:32 JST 時点では header 98,225（60%）。
  header 取得やプロセス起動だけを復旧完了・peer 確認済みと扱わない。
- `cargo test -p kaspa-p2p-lib --lib candidate_summary_route_tests -- --nocapture`:
  **2/2 PASS**。実 Router の共有 queue で不具合を再現し、独立 queue では
  遅延 summary の後も Block と BlockLocator を受け取れることを確認。
- 18:39 JST に b0 の verified resync を開始。元 DB を aside し、95・local b3・公開 b6 の3台へ接続。
  修正版配備までは旧 binary の通常再起動で pending hold が失われないよう、
  launcher が未確認 report を見た再起動を exit 78 で拒否する暫定 guard を使用。
- `cargo check -p kaspa-p2p-flows --features evm`: **PASS**。
- `cargo test -p kaspad --lib --features evm palw_verified_resync -- --nocapture`: **2/2 PASS**。
  通常の連続再起動が partial DB・元 DB・report を変更せず hold を再開し、confirmed 後は解除すること、
  破損・不整合 report は拒否すること、元 DB がない fresh sync でも hold を保つことを確認。
- 実行ログ: [P2P](route-tests.log)、[EVM 型検査](flow-check.log)、[再同期](resync-tests.log)。
- 先行ビルドは live `2ca68baee` に今回の4ファイルだけを適用した
  `f7d1998f043d8fc6c82a8683e02dc134aa6ae1e9`。合意 core の変更は含めない。
  別担当の統合版が先に完成したため、先行ビルドと待機 probe は停止した。
  この停止をビルド成功・Linux probe 成功と扱わない。
- 実配備に選んだ統合版は `8cac52ec4ed15fb4ea982d46235a5c3b5f78e4cc`、
  SHA-256 `e6d638a4f2a9fa32b6072c32ad5ce929f015efa189b721ceee2be5a9aa52e83e`。
  別担当の全モデル移行（DAA 18,640）も含むため、旧 live と params/schedule ID は異なる。
  [build manifest](deployed-build-manifest.json) に全 ID と features/profile を記録した。
- 統合版の Linux 隔離2起動試験は **PASS**。
  初回は `--palw-verified-resync`、2回目は通常起動で、pending report の byte 一致、
  partial DB sentinel、aside 数、hold 再開ログを確認。両回の params/schedule/genesis と
  DAA 0・unsynced が一致し、両回とも `Kaspad has stopped` / clean stop を確認した。
  [結果](linux-resume-probe.json)、[ログ](linux-resume-probe.log)。末尾の stop 警告は、
  正常停止後に GC 済みの transient unit を cleanup でもう一度 stop したもの。
- 21:21 JST 時点で b0 の launcher は統合版への SHA・全 flags・always-on duties 検査を通過し、
  native hold 再開を使用する準備が完了。b0 はまだ停止中で、新しい ID の95・b3を待つ。
  モデル移行担当との調整で `--palw-produce` のみ一時除去し、旧5席 claim の新規生成を止める。
  bond/key・relay・検証者 duties は保持し、producer 再開は移行版での別担当作業とする。

19:51 JST 時点では b0 は停止中。95 の offline backup を復元先に配置し、
元の分岐 DB と fresh sync の partial DB を両方保持している。
pending report は byte 単位で変更せず、旧 binary に対する launcher の再起動拒否も実測した。
修正版の隔離起動試験が完了するまで、この復元 DB を起動しない。
配置ログは [backup-stage.log](backup-stage.log)。

全履歴 IBD の body/fold が毎秒約 7–20 ブロックにとどまるため、同じ管理下の95から
clean offline backup を取得して復元する運用も進める。95 は18:57:07 JST に
`Kaspad has stopped`、18:57:09 に正常終了（status 0）を確認した。
これは自分の検証済み DB のバックアップ復元であり、復元先での独立した全履歴再検証ではない。
転送データの digest を照合し、native pending-resync hold と実際の複数 peer 確認を維持する。
report の `confirmed` を手書きしたり、claim の認証状態を加算したりしない。

転送済み archive は 7,438,164,368 bytes、SHA-256 は
`2a4af052174ebc554929fa2673a53f4c81f831d6718019f925578182fc2e002d`。
受信側で size・SHA-256・zstd integrity を確認し、1,213 files / 9,108,885,840 bytes を展開した。
復元には meta・consensus・utxoindex をまとめて使用し、producer key は移動していない。

元 DB は削除しない。verified resync の report が `confirmed` となり、選択チェーン上の
共通ブロック・先端の進行・複数 outbound peer の確認が得られるまで完了としない。
