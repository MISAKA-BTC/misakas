# Testnet公開候補の接続・検証台帳

## 2026-11-26 改訂後の公開候補設計

現行のモデル追加・管理方針は [RFC-0007](../rfc/0007-bft-model-admission.md) と [RFC-0017](../rfc/0017-model-stop-and-retirement.md) に従う。以下の過去の試験と当時の残件は、各試験で用いたprofileの記録として読む。

11月26日統合後の正本では、[RFC-0007（BFTモデル追加）](../rfc/0007-bft-model-admission.md)・[RFC-0016（MOE3）](../rfc/0016-position-market-and-rewards.md)・[RFC-0017（BMR1）](../rfc/0017-model-stop-and-retirement.md) は採択済み・未有効化。[RFC-0013（MWA1）](../rfc/0013-model-work-accounting.md) は提案段階。以下の作業順で第22〜27回の合格条件を更新する。これ以前の実測・決済証跡は当時のprofileに属する。

| 回 | 改訂後の実装・検証方針 |
|---|---|
| 22 | 新規モデル追加・採用はRFC-0007に従い `BFT_DECISION` のみに統一し、`BaseCompatible` 検査・対象に紐づく任意測定資料の追跡・正規BFT承認による原子登録と市場作成を検証する。`KERNEL_AUTO` の自動採用は廃止（kernel解析・lowering・技術互換検査は維持、測定証拠の添付や一律合格閾値は票の有効条件にしない）。MOE3（RFC-0016）では元genesisを含む全モデルを共通管理し、係数1.0、一意市場とする。新規のfork特別区分・元genesisの管理免除は廃止。BMR1（RFC-0017・RFC-0012）の恒久廃止では対象モデルの登録者・採掘bondと、Panel・Gate Cの個別Court責任（検証bondは一律保護）を区別して検証する。QF1の常駐配送証跡と公開保留を引き継ぐ。 |
| 23 | 同一仕事でFOW1の有償適格性・独立再実行・成功Finalを確認。MWA1の仕事量・報酬・EXEC credit案は別に参照検証し、価格・量子・targetの提案値を公開条件へ組み込むには採択を要する。 |
| 24 | 新規経済経路はMOE3。POSITIONは小数8桁。原資Cの50%は採掘報酬用POSITION購入、20%は永久LP用POSITION購入、20%はMISAKA側永久LP、5%はPanel・Gate C、5%はBFT。genesisのcreator不在を含む一回決済・端数・供給保存を同じ仕事で照合。旧MOE2の未決済分は元契約で完了させる。 |
| 25–26 | 移行前後の市場、POSITION残高、永久LP、用途別bond、廃止・slash・未決済receiptを対象状態に含め、PrunedとArchivalのIBD・reorg・再起動後の一致、保持義務、媒体ピークと回収量を測定する。既存市場を再初期化しない。 |
| 27 | 全対象の短時間検証後に総合試験。保存S3のDAA 8602の受賞・残決済は旧条件で照合し、MOE3新規経路の結果と個別に記録する。公開有効化・QF1 REVIEW_HOLD解除・VPS配備には別途承認を要する。 |

BMR1のBFT恒久廃止では対象モデルに拘束された該当資産を規定どおりburnする。Courtの個別slashは別の回収であり、新規則では実回収MISAKAの49%がPanel・Gate C、残余51%がburnとなる。各移行の発効条件と旧権利の扱いは[MOE3移行契約](../governance/fractional-position-economics-transition.md)・[BMR1移行契約](../governance/model-retirement-transition.md)に従う。日程は到達目標であり、改訂仕様のnative接続・実媒体検証は未達。

## 11月25日時点の接続表と以後の試験履歴

2026-11-25。S3の基準はT12作業ブランチの ad3016e40、MAD2の規範は採択commit 7ad72a9e0 にある。下表の既存試験には隔離profileでの結果を含む。公開候補全体の INTEGRATION VERIFIED と RELEASE READY は未達。公開有効化・VPS配備は承認待ち。

| 経路・SPEC | 確認した実装入口 | 拒否・保持条件 | 残る公開候補の検証 |
|---|---|---|---|
| TFO3・旧ADR-0176 | private node、PFS3受理、selected state、cold-settlement | quantumの二重使用を拒否。未認証Finalでは報酬を保持 | S3で実受賞の決済・再起動・reorgを確認。全経路接続後の公開候補総合試験が残る |
| Pruned Node・RFC-v2 0012 | pruning processor、palw_pruning_capture_v1、row_stream_v1、PALW retention janitor、storage_capacity | pruning pointの状態を確保してから削除。保持義務が不明な資料は保持 | Round 16のTCP取込点DAA 13798と選択先端51258でroot・全73 UTXO一致。未決Court・決済等の実データ、資料取得、容量増加率と公開運用予算は対象別に検証 |
| TB1・RFC-0061、旧ADR-0229 | palw_tb1、private_tb1_close、remoteのtb1_exposure | 資格・署名・期限・finalityを検査。finality撤回後のshare公開を停止 | 隔離3プロセスの証跡あり。通常Nodeとholderの配信、再起動、EXEC/PANEL双方の端から端までの確認 |
| FOW1・RFC-0062、旧ADR-0230 | TB1 EXEC登録、TIR実行、QF1 Readoutとwork carrier | 入力・seed・契約・Ticketの一致、採用basisに応じた評価条件 | 実LLM内部値から裁定・権利消費まで同一claimで追跡 |
| QF1・RFC-0066、旧ADR-0234 | tir/lower、tir/exec、qf1_local、palw_qf1のCourt、qf1_terminal_court | REVIEW_HOLD。artifact、tokenizer、trace、Readoutの一致 | 資料の期限とreorg、実機評価、通常受付とFinal後の権利を隔離profileで照合。公開採掘は別途承認 |
| MAD2・RFC-0067、旧ADR-0235 | 非公開MAS1の案件受付・停止適用で、nativeのgenesis登録と選択台帳のclass・artifactを照合 | 元genesisの管理操作を拒否。追加モデルの停止が承認に優先。BFT票をclaim認証に流用しない | kernel自動・BFT YES/NOの二経路、共通技術条件、原子的fork指定、停止集合・履歴を通常処理で検証 |
| MOE2・RFC-0064、旧ADR-0232 | palw_model_market_v1/moe2、private_moe2_settlement、remoteのexec_input/moe2 | 永久LPの移転・償還を拒否。過去cohortの資産・責任を保持 | 通常quoteとnative/EVM残高、同じ受賞権利の一回消費、支払と市場更新の原子性 |

## 保存台帳の基準

S3の永久保存世代は ef7c7a37e92c440fa3ef4f487455f1b8。既存の手順は [S3の決済追跡](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-difficulty.md) にある。3ピアの停止DBから報酬受付DAA 7867に対する決済応答の全項目一致を再確認した。DAA 8602でfrontierは7866、後続claimのFinalは8527、Gate Cはpending。

S3ホームの観測値は約35 GiB、うち3ピアのconsensus DBは合計約32 GiB、PALW retentionは約94 MiB。snapshotツリーのdu集計は約146 GiBで、APFSの共有extentを含む。これを解放可能な物理容量には計上しない。Court serviceのブロック観測は予約簿を更新し、prefix 216のdeltaに更新前後の行を保存する。既存private-palwは各行をZstandardで圧縮する。ローリング保存に指定した世代は検証済みの最新2世代を保持する。以前の世代と手動で固定した世代は別管理。

短時間検証はリポジトリ直下で次を実行する。

```sh
python3 -m unittest discover -s scripts/drills/tests -p test_palw_devnet.py
python3 -m unittest discover -s scripts/drills/tests -p test_protected_receipt_payout.py
```

世代交代と復元を含む18件、決済ランナー23件が成功。これらは新規Pruned IBDや実受賞の最終決済の証跡には含めない。

## Pruned NodeとArchival Nodeの保持範囲

通常のPruned Nodeはpruning pointと設定された履歴保持期間に従ってretention rootを進める。pruning pointのPALW snapshotと必要なEVM anchorを用意し、対象ブロックのbodyとPALW deltaを同じバッチで落とす。支払済みの公開資料には固有の保持期限があり、archive GCは不可逆なpruning pointのDAAを使う。Court資料を整理するローカルjanitorは、その時点のclaimの保持義務を別途照会する。

`--archival` でもpruning pointの計算は続く。retention rootは維持し、履歴のprune処理は入口で戻る。snapshotを復元した新規参加ピアの検証には、同じ選択ブロックのrootと、その後に同じ入力を処理した結果を照合する。[pruning processorの実装](../../consensus/src/pipeline/pruning_processor/processor.rs)

## DAA 8602のDB内訳と旧blobの回収

停止DBを読み取り専用で開く診断を追加した。下記の `DB` は対象ピアの consensus-001 ディレクトリ。prefix別の値はRocksDBによるflush済み範囲の推計で、blob・WAL・APFS共有を含む回収可能量ではない。

```sh
cargo +1.93.0 run --locked --offline -p kaspa-database --example cold-store-sizes -- "$DB"
MISAKA_S3_COLD_CONSENSUS="$DB" cargo +1.93.0 test --locked --offline -p kaspa-database --test private_palw_journal -- --ignored --nocapture
```

| DAA 8602での測定対象 | バイト |
|---|---:|
| peer 0 SSTファイル長合計 | 5,933,773,336 |
| peer 0 旧blobファイル長合計 | 5,114,865,628 |
| peer 0 番号付きWALファイル長合計 | 51,129,831 |
| peer 0 / 1 / 2 のPALW delta SST範囲推計 | 5,777,443,736 / 5,780,125,962 / 5,749,160,297 |

従来のprivate-palw圧縮は新規書き込みに適用される。旧HDD presetの生存blob参照はGCが無効なままだとcompaction後も外部ファイルを必要とした。private-palwでblob GCを有効にし、通常compactionの対象となったSST内の生存値を圧縮SSTへ移す。pruning pointやdeltaの適用・取り消し形式は共通。

peer 0から抽出した8 MiB超の16行を一時DBに書いた。旧形式71,764,554バイト、新形式22,609,967バイト（SST＋blob、68.5%減）。旧形式DBを新設定で開き、対象prefixのcompaction後に旧blob 71,761,656バイトのファイルがすべて消え、再オープンした16行の全バイトが元の保存行に一致した。同じ16行の独立した旧形式DBで、全通常ファイルの割当はcompaction前84,529,152バイト、後24,506,368バイトになり、60,022,784バイト（71.0%）減少した。保存世代自体は書き換えていない。世代共有を含むボリューム全体の実回収量、DAAあたりの長期増加量は未測定。

同じDAA 8602の3ピアに `--cold-journal` を実行し、先端ブロック `76f6765de188b75eb889cf02d9daf913a8686f5b0e8d2d9ee6da42fce76f28f64eafe31f5012ccd7a4437e4f850ef0feb39c3eefd00244aea9ba5ac27bbad44f` の4件のjournal entryを巻き戻して再適用した。3件の応答は全項目で一致し、各ピアが `fullStateRestored=true` を返した。この検証は保存先端の局所的な再適用であり、決済後の分岐試験は未達。

## ファイルの割当と再開時の重複

`scripts/palw-devnet storage --home "$HOME/.local/share/misaka-tfo3-award/network-s3"` は停止ホームと全snapshotを読み取り、WAL、consensus SST・blob、UTXO index、資料、ログなどの区分ごとにファイル長と割当バイトを集計する。macOSではAPFSのファイル専有extentも照会し、取得不能な区分はnullを返す。ボリュームの空きは別に表示する。ファイル専有量には複数世代で共有する領域やディレクトリメタデータを含まないため、世代集合を削除した場合の回収予測には使えない。

保存ホームの全ピアのSST割当量は18,185,936,896バイト、blobは15,431,081,984バイト、WALは153,427,968バイト。停止ホームの計測ファイルは全区分で専有0バイト。最新世代の計測ファイルの専有量は2,449,408バイト、全保存世代・復元前バックアップのファイル専有量合計は39,175,168,000バイト。保存世代は削除していない。

DAA 8602のmanifestには同一ハッシュのPanelモデルが17件あり、2件目以降のファイル長は合計2,885,381,120バイト。再開時に元台帳のrunにあるモデルと隣接tokenizerの両方のSHA-256を入力と照合して既存パスを使用する。どちらかが異なれば独立した保持先にコピーする。既存コピーを削除する移行は行わない。

## DAA 8602のpruning pointと資料保持

3ピアでpruning point、retention root、checkpointはいずれもgenesisにあり、pruning point indexは0だった。この世代では通常の履歴pruningで先端のdeltaを回収できない。先端から選択親を16ブロック遡るとDAAは8594になった。各ピアで該当する16件のdelta（値の合計99,642,903バイト）を独立したprivate-palw DBに書き、flushと再起動後に全バイトを照合した。SSTの割当は12,161,024バイト、区間のDAA増分8あたりで割ると1,520,128バイト/DAA。対象は選択されたdeltaの短区間で、他のDB区分、並列ブロック、履歴の削除を含むノード全体の増加率は別途計測が必要。

別に、DAA 8305から8602までの297 DAAで、3ピアと実走資料を含む保存世代全体のファイル割当合計は1,332,961,280バイト増（約4.49 MB/DAA）。SSTは950,411,264バイト増、blobは同量、WALは338,239,488バイト減。各世代を単独で走査した差分にはAPFSの共有extentを含む。

先端16ブロックのdeltaではCourtの `StagedCourtSeed`（wire tag 210）が99,631,680バイトで、全行の値の99.98%以上を占めた。全件が単一のCourtServiceキー（tag 12）の変更前後を記録したものだった。旧世代DAA 8305でも、16ブロックの区間に同じキーが99,631,680バイトあり、圧縮後SSTの割当も同じだった。未決claim数は8305で2件、8602で1件。各区間のCourt行の値99,631,648バイト中43,588,846バイトは、同区間で既出の長さ・BLAKE2b digestと一致した。反復量は論理値で、物理削減量は別に実測する。

受賞claimのGate Cはcertified、後続FinalのGate Cはpendingで、両方とも資料保持対象だった。各claimのopen court、open DA sessionは0件。合意状態からの保持判断に、係争、未回答のDA session、進行中claim、Final後の独立Gate C待ちを含めた。3ピアの測定結果は全項目が一致した。[保存世代の測定値](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-storage-retention.json)

既存のS3 cold readerのDB・policy・salt指定を使い、診断を再現できる。

```sh
"$reader" --cold-storage-window "$db" "$policy" "$salt" 16
"$reader" --cold-retention "$db" "$policy" "$salt" "$claim"
```

保持判定の変更commitは `7f50782d5`。kaspadの `palw_retention::` 21件、Panelの `seat_r_tests::foreign_gc` 4件と `seat_r_tests::the_foreign_retention` 1件が成功。読み取り失敗、一覧の不完備、資料差し替え、再起動後と分岐変更後の保持義務を含む。

## Pruned Nodeの短時間fixture

`p2_t49_a_node_that_imports_its_pruning_point_inside_a_backlog_mints_what_an_archival_node_mints` は0.36秒で成功。試験の支払待ち状態（50行、reporter 3件）をpruning pointで渡し、point以下のdeltaを持たないimporterのrootが一致した。さらに連続11ブロックのheader hash、coinbase、選択状態のrootをArchival Nodeと照合した。`p2_t48_a_reorg_across_latch_move_mint_and_burn_lands_on_a_fresh_replay_s_root` は0.59秒で成功し、mint・burnをまたぐ分岐変更後のrootを新規replayと照合した。いずれもプロセス内のfixture。pruning snapshotの読み取り失敗、同一点での再試行、storeからの再取得を含む12件も1.11秒で成功。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib p2_t49_a_node_that_imports_its_pruning_point_inside_a_backlog_mints_what_an_archival_node_mints
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib p2_t48_a_reorg_across_latch_move_mint_and_burn_lands_on_a_fresh_replay_s_root
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib pruning_snapshot_readiness_ -- --test-threads=1
```

容量予算（`--storage-cap-bytes`）を有効にしたノードで、物理空き容量が不明な間は新規予約を拒否し、監視周期で書き込み受付を停止する。空き容量を取得し直し、再開余裕を満たすと受付を戻す。取得障害だけを理由に回収処理を実行しない。Unixの資料回収では割当ブロックで見積もり、別のhard linkが残るinodeを解放量に算入しない。再開判定には回収後の実測値を使う。

`kaspa-utils --lib storage_budget::` の10件、`kaspad --features private-integration-clock --lib palw_retention::` の22件が成功。後者は資料の長さで回収量を期待していた2つのfixtureを割当ブロックの実測に更新して再実行した。

## 容量制限付きPruned Nodeの最下層SST

通常のdefault presetで、PALW ConsensusV2、pruned、容量予算ありの3条件を満たす場合に、activeとstagingのconsensus DBで最下層SSTのZstandard level 12圧縮を使う。最下層への通常compactionで適用する。ノード起動時に手動compactionを要求する運用は追加しない。

DAA 8602の独立した16行fixtureで通常presetの容量制限を比較した。最下層compaction後のSST割当は152,129,536バイトから22,609,920バイト（85.1%減）。同じマシンでcompactionは1.19秒から16.72秒になった。両方のDBを一般の読み取り設定で再開し、16行すべて元の値と一致した。実運用でのcompactionの処理待ちと長期増加率は未計測。既存の旧blob移行fixtureも同一実行で成功した。

```sh
MISAKA_S3_COLD_CONSENSUS="$DB" cargo +1.93.0 test --locked --offline -p kaspa-database --test private_palw_journal -- --ignored --nocapture --test-threads=1
cargo +1.93.0 check --locked --offline -p kaspa-consensus --features evm
```

## TFO3の後続attemptとGate Cの時計

DAA 8602の3ピアで、選択された履歴から、直近の確定frontierになったattemptの当該ブロックのjournalを照合した。DAA 7866でProvisional、8370でFinalとGate Cのcommit、8402でdraw、8440で7 attestationsによるcertificationと同じブロックでのfrontier更新を確認した。これは報酬受付7867に達する前の実履歴である。次の後続Finalでは、8527のcommit時点のspanは8402、必要なanchor spanは8403、8602時点のanchor spanは8402。席の抽選と署名の受付はまだ始まっていない。未抽選の期限は10687。[3ピアの履歴](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-native-frontier-path.json)

```sh
"$reader" --cold-frontier-path "$db" "$policy" "$salt"
"$reader" --cold-settlement "$db" "$policy" "$salt" 7867
```

既存受賞の1回の再開には `scripts/palw-devnet up --home "$home" --resume-node "$node" --settlement-budget SECONDS MIN_FREE_BYTES` を使う。受賞の検証済みpayout checkpointを必要とし、時間上限、容量下限、空き容量の読み取り失敗で停止する。通常の停止処理で各プロセスを閉じる。devnet試験21件（制限時間による実際の割り込みと終了理由の記録を含む）、決済ランナー試験23件が成功。

## S3の300秒再開とDAA 8667保存世代

DAA 8602の保存世代を保持し、既存の受賞から300秒、空き容量下限12 GiBの条件で一度再開した。新規Studio推論は0件。時間上限で終了し、3ピアのDAA 8667の保存世代が一致した。受賞受付は7867、決済frontierは7866、後続Finalのanchor spanは8402である。実受賞の報酬は未決済。[保存世代と受理結果](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-bounded-8667.json)

DAA 8603で生成したブロックのworkとdeltaは保存されていた。3ピアともmerge時にモデル当期のブロック割当不足でattemptを拒否した。通常のlead producerの事前判定に、既存の会計規則で候補DAAとartifactに対応するモデルの残り単位を確認する処理を追加した。選択親の状態を使うため、生成後に競合が生じた場合の受理判断は引き続きmerge時のfoldが行う。

生成済みブロックからclassとartifactを取得して、新しい事前判定を保存DB上で実行した。各行とも3ピアの結果が一致した。1ブロックの必要量は1,000,000単位。候補DAAの結果は、親の状態を固定した場合の割当判定である。

| 保存親DAA | 候補DAA | 残り単位 | leadの割当判定 |
|---:|---:|---:|---|
| 8602 | 8603 | 998,548 | 不足（実際のmerge拒否と一致） |
| 8602 | 8604 | 1,003,513 | 通過 |
| 8667 | 8668 | 1,321,240 | 通過 |

```sh
"$reader" --cold-produced-attempt "$db8667" "$policy" "$salt" "$block"
"$reader" --cold-lead-room "$db8602" "$policy" "$salt" "$class" "$artifact" 8603
"$reader" --cold-lead-room "$db8602" "$policy" "$salt" "$class" "$artifact" 8604
"$reader" --cold-lead-room "$db8667" "$policy" "$salt" "$class" "$artifact" 8668
```

`cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib allocation_view_reports_only_models_and_their_remaining_block_units` が成功。同一epochで残りがちょうど1ブロックの時、使用直後、次のDAAで回復した時、epoch切替前後の判定を確認した。private nodeと読取exampleのビルドも成功した。

## Pruned IBDのPALW snapshot転送容量

分割転送の受信側はセッション開始時に容量予算の受付を確認し、最初のチャンクのpoint・順序・長さ・hashを照合してから、最初の書き込み前に転送全体のwire長を予約する。予約は中断からの再開を含めてspillと同じ期間保持する。旧形式はメモリ内で処理するため、送受信とも共通のセッション枠だけを使用する。分割送信側はエンコード済みのバッファをspillへ書く前に、その書き込みで必要になるファイル長まで予約を増やす。圧縮転送では圧縮後の長さが予約対象となる。転送中はその予約を保持する。

送受信それぞれに64 MiBのspill予算を設定し、35,000件の支払待ち行を複数の4 MiBチャンクに分けて実TCPルーターで転送し、復元した状態とrootを照合した。別のrootを期待する場合は最初のチャンクで中止し、予約が解放された。1,024バイトの予算では1,024バイトの転送が成功し、1,025バイトは書き込み前にローカル容量不足になった。圧縮されたsnapshotを実際のwire長ちょうどの予算で生成し、無制限時と同一のrootを確認した。1バイト少ない予算では生成を拒否し、ディスク予約・セッション枠を解放した。正常・失敗・中断再開を含むsnapshot関連46件が3.19秒で成功。このfixtureはルーターとPALW復元を対象とし、空DBの通常ノードがpruning pointから先端まで同期する総合確認は残る。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-p2p-flows --lib snapshot -- --test-threads=1
```

## 空DBからの通常P2P IBD（simnet）

同一プロセス内で別々の空DBを開く2台のkaspadをloopback TCPで接続した。送信側のみ3,500ブロックを生成し、各submitの受理を確認した。短縮pruning設定でpruning pointが動いている間に一度pointの不一致で同期を中止し、自動再接続後にheaders proofを検査してstagingを採用した。参加ノードは1,499 UTXOを受信し、DAA 3500、sink、pruning pointが送信側と一致した。単一ソースのchain participationは所定どおり審査待ちだった。104.67秒で成功。[通常IBDの観測結果](../evidence/palw/pruned-ibd-2026-11-25/simnet.json)

このsimnetはPALW ConsensusV2無効。前節のPALW snapshot転送fixtureと、実際のConsensusV2 pruning pointを経由する空DBからの同期は別の検証項目である。後者は未達。通常のIBDではローカルヘッダーを読み出せない場合の扱いも修正した。同期方式の選択、stagingと現行tipの時刻検査、PALWの適用規則を決める現行sinkのDAA取得で、対象hashを伴うローカルの読み取りエラーを返す。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-testing-integration --lib ibd_participation_tests::a_node_that_ran_an_ibd_does_not_report_itself_synced -- --exact --nocapture
cargo +1.93.0 test --locked --offline -p kaspa-p2p-flows --lib unavailable_local_import_header
```

## TB1 holderの署名返却とfinality撤回

隔離fixtureのEXEC・PANEL holderは、登録鍵を照合し、署名対象をjournalへ同期書き込みしてから署名する。署名を返す直前に選択履歴・finality・元の公開期限を再確認する。開いたjournalとパス上のファイルの同一性も確認する。署名中のfinality撤回を観測したroundは再起動後も停止した。journalが削除された場合や、同内容の別ファイルに交換された場合も署名を返さなかった。

journal関連8件が3.79秒で成功した。別の3プロセスのnative fixtureでは、分岐の切替後もanchorが選択履歴に残る一方、深さ不足でfinalityが撤回され、holderの停止が再起動後も続くことを確認した（2.51秒）。後者は既定のテストスレッドスタックではoverflowし、64 MiBで成功した。通常ノードからholderへの配信と継続的な分岐監視は未検証。

```sh
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p misaka-palw-remote --lib tb1_exposure::tests -- --test-threads=1
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected -- --test-threads=1 --nocapture
```

holderの署名処理は、予約開始時のローカルpruning import epochを署名返却まで保持する。各finality観測の前後でepoch、UTXOの安定状態、取込セッションと進行フラグを確認する。取込中や世代の切替中に取得した分岐情報では署名を返さず、曝露済みroundの恒久停止も記録しない。

holder試験9件が4.18秒で成功した。追加fixtureでは、取込中の初回予約、予約後の署名中に同じ選択先端で完了した世代切替、分岐観測途中の世代切替を再現した。署名返却を保留し、元の予約は保持された。別途、上記の3プロセスのnative分岐試験が2.33秒で成功し、安定した世代で実際にfinalityが撤回された場合は再起動後も停止することを確認した。この結果は通常Nodeからholderへの配信や、別DBへのcold restore後の再開を検証するものではない。

## 追いつき同期時のpruning point更新

T12の短縮深度fixtureで、既知のpruning pointを検証してから履歴・virtual・UTXO importの状態を更新する経路を検査した。欠落したsinkヘッダー、現行pointより前への更新、検証と書き込みの間に現行pointが進んだ場合を拒否した。最後のケースでは更新ロック内でpointを再照合し、既存のimportセッションのトークンが保持された。正当な更新後、genesis側の合意検証とstagingへの履歴取り込みは成功した（0.62秒）。このfixtureはヘッダー同期とpruning履歴の検査用で、通常ノードのP2P同期完了は別途検証する。

```sh
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib t12_a_moved_pruning_point_s_proof_builds_validates_and_applies_at_225 -- --test-threads=1
```

## 保存済みConsensusV2台帳でのsnapshot世代復旧

Round 16の保存台帳ではpruning pointが第46世代（DAA 13798）に進んだ時点で、PALW snapshotは前のpointを指していた。Archival Nodeのretention rootと削除済み位置は一致しており、従来の起動条件ではsnapshot復旧が省略された。起動・再試行時にsnapshotの保存先ブロックも確認し、現行pointのsnapshotが必要なら既存の差分再計算・保存・読み戻しを通すよう修正した。削除は従来のretention境界に未処理の差がある場合だけ実行する。

保存台帳の私有コピーで通常のconsensus起動・停止を通し、再度読み取り専用で開いた。先端、pruning point、retention rootと削除済み位置を保持し、復旧した状態rootが選択チェーン上の直後のヘッダーのcommitmentと一致した。10.05秒で成功。原本118ファイルは試験の前後で同じSHA-256一覧に一致した。snapshotの保持・失敗・再試行など既存12件も1.17秒で成功。これは供給側の復旧確認で、空のConsensusV2ノードへのP2P転送・先端一致は未達。[世代・ブロック・rootの観測記録](../evidence/palw/pruned-ibd-2026-11-25/round16-archival-recovery.json)

[保存台帳の所在](../evidence/palw/mas1-qf1-native-2026-11-21/t12-handoff-2026-11-22.md)に記載した `snapshot` を使う。

```sh
MISAKA_PRIVATE_GENESIS_SUFFIX_DB="$snapshot/db" RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib verify_recovery_pruning_snapshot_at_selected_successor -- --ignored --test-threads=1 --nocapture
```

同じDAA 13798のコピーで、通常の供給APIからUTXOを32件ずつ取得した。計73件を合算したMuHashは当該ヘッダーのUTXO commitmentと一致した。EVM実行ヘッダーのcommitmentと、供給されたアカウントから再計算したEVM state rootも一致した。overlayの対象ブロックを照合し、PALW snapshotのクラス宣言2件が完全性検査を通過した。

このコピーをPruned Nodeのrecent履歴設定で再度開き、ブロックごとのEVM snapshot保存を停止した場合の復旧を検証した。最初の復旧で当該pointのEVM checkpointを確保し、再試行で再利用できた。コピー内のcheckpointのchecksumを1ビット変更すると復旧は保留となり、retention rootと削除済み位置は維持された。試験全体は10.21秒、保存原本は118ファイルすべてで事前・事後ハッシュが一致した。

UTXOの供給処理は各ページを読む間、UTXO集合の位置・安定フラグを確認する。移動中または再取込中の集合は供給しない。受信側のEVM有効判定にはローカルで読めた当該ヘッダーのDAAを使う。通常の履歴削除では必要なEVM checkpointの読出し・復元・検証・保存の失敗をpruning workerに返し、再試行する。snapshot関連12件と、追いつき同期で未準備のUTXOページを拒否するT12試験1件も通過した。[保存台帳の供給データ照合結果](../evidence/palw/pruned-ibd-2026-11-25/round16-archival-recovery.json)

### Round 16 保存台帳からの TCP 初期同期（2026-11-25）

非公開の Round 16 設定（fixture clock・PoW 検査省略）で、Archival 側には保存台帳のファイルコピー、Pruned 側には同じ genesis のみを与えた。通常の TCP プロトコル 108 で接続し、受信側は独立した staging を1回作成した。供給側の pruning point は DAA 13798、tip は DAA 51258。pruning 履歴のユニークヘッダー 27,601 件を含む検証を経て、受信側は trusted block 266 件を処理し、続くヘッダー同期に入った。

120秒の観測期限時点で後続ヘッダー 15,375 件（20%）まで進んだ。期限で接続を閉じ、試験結果は FAILED（122.57秒）。staging の公開・reset は0回。受信ノードの UTXO 取込、PALW root と tip の一致、報酬決済の検証は未達。S3とは別の保存台帳を使った。再現には保存原本の DB ディレクトリを環境変数 MISAKA_PRIVATE_GENESIS_SUFFIX_DB に指定し、次を実行する。

```sh
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib round16_archival_to_genesis_node_over_tcp -- --ignored --test-threads=1 --nocapture
```

### 復元時の経路選択と受信側のtip読み取り

Round 16 の復元では、checkpoint から600件の差分を適用する前に、比較のためだけに現tipから74,920件分の経路を展開していた。両端が選択チェーンの索引にあるときは、その位置の差で経路長を得る。索引が読めない場合は従来の経路走査を使う。選んだ経路の差分再生とstate root検査を通してからsnapshotを発行する。

同じ保存世代・同じ復旧試験の所要時間は前回10.21秒、今回1.07秒。各1回のローカル測定。DAA 13798のrootと後続ヘッダーのcommitmentが一致し、UTXO 73件、EVMとクラス宣言の検査も通過した。原本118ファイルは前後とも保存済みSHA-256一覧に一致した。関連するsnapshot試験は12件成功した。

受信側でpruning履歴の優劣を比較する際、ローカルのheaders selected tipが読めないときは、ローカルのpruning pointを伴う読み取りエラーを返す。`pruning_proof_comparison_requires_readable_local_tip` で実DBの行の欠損・破損の両方を検査し、復旧後は同じ累積workの候補を通常どおり拒否することを確認した。

### S3 の実受賞ブロックからの決済照合

停止中の2世代を読み取り専用で開いた。下記の診断は受賞ブロックだけを指定し、選択チェーンの子が作ったcoinbase、次のブロックの受理記録、現在のUTXOの金額・worker script・受理DAAを照合する。PALW保存先端とvirtualの選択先端が異なる場合は、journalをメモリ内で巻き戻し・再適用し、選択先端の保存rootを照合して決済を判定する。対象はDNS退役前の私設S3の単一blue報酬である。

```sh
"$reader" --cold-award-settlement "$db" "$policy" "$salt" "$receipt_block"
```

DAA 8602世代のpeer 1で、保存PALW先端は8602、UTXOの選択先端は8601だった。1件のdeltaを戻すと選択先端のrootと一致した。他の2ピアは8602、次の保存世代は3ピアとも8667で、巻き戻しは不要だった。従来のDAA指定の読み取りは保存PALW先端だけの観測であり、この世代の3ピアで選択先端が一致したことまでは示さない。

6個のDBすべてで同じworker出力 320,084,650,080 sompi が未使用で残り、受理DAA 7867、safe frontier 7866、settled=false だった。各世代で受理ブロックを受賞ブロックの代わりに入力すると拒否した。[世代・ピアごとの照合結果](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-cold-award-settlement.json)

### snapshot・restoreの空き容量下限と世代交代

`scripts/palw-devnet snapshot --home "$home" --rolling --min-free-bytes 12884901888` のように、コピー先に残す容量をバイト単位で指定できる。restoreも同じ引数を受け付ける。既定は512 MiB。コピー処理はmacOSのclonefileまたはLinuxのFICLONEを試し、非対応・異なるvolumeで通常コピーへ移るときはデータ区間を見積もる。疎ファイルでは空白区間を保持する。コピー中も空き容量を再取得し、読取不能・不足なら作成途中の世代を破棄する。外部プロセスが同時にディスクを使う場合の専有予約は行わない。

新世代のデータ・manifest・ディレクトリを同期し、世代名へのrenameと保存先ディレクトリのfsyncが成功してから古いrolling世代を回収する。保存を明示したmilestoneは回収対象外。容量不足のsnapshot・restoreで元の作業台帳と既存世代を保持し、世代名公開直後のfsync失敗でも旧世代を保持する試験を含め、devnet管理試験25件が1.25秒で成功した。

DAA 8602の保存世代のpeer 0から003366.sst（73,194,132 bytes）を一時ディレクトリへクローンした。コピーのSHA-256は原本と一致し、コピーを書き換えた後も原本は `3ecbfc3991398e51bbe9f337d7eac711d16a43b2c2fca51e85cfc35b63abf101` だった。別の疎ファイルで通常コピーを強制すると、論理134,217,728 bytes、割当32,768 bytesで前後のSHA-256が一致した。これらはmacOS上の測定。Linux FICLONEの実機試験と、長時間運転時の総ディスク消費は未検証。

```sh
python3 -m unittest discover -s scripts/drills/tests -p test_palw_devnet.py
```

### 復元プロセス停止後の台帳回復

従来のrestoreは、既存homeをbackupへ移してから復元台帳をhomeへ移す間にプロセスが終了すると、次のupが空のhomeと判断できた。現在はコピーとmanifest照合の後、元台帳・復元台帳のデバイス番号とinode、退避先を記録して同期し、その後に名前を切り替える。復元先のnative identityと、該当する場合は登録記録を照合してから復元完了を記録する。途中で停止した場合は、次にup・init・snapshot・restoreがlifecycle lockを取った時点で未完了の切替を戻す。記録と異なるディレクトリが置かれていれば停止する。

短時間fixtureは、元台帳を退避した直後、復元台帳を設置した直後、検証を終えた直後に子プロセスを終了させた。元homeが存在しなかった場合も照合した。復元台帳を戻す処理と元台帳を戻す処理の直後に再度終了しても、次回の起動前に未決済報酬を含む元台帳を回復した。検証済みの場合は復元台帳がhomeに残り、元台帳をbackupから読めた。保存snapshotは全ケースで維持された。管理試験26件が1.91秒で成功。対象はプロセス停止で、電源断実機試験は未実施。

### MOE2決済receiptを含むsnapshotの復元

Pruned NodeがPALW状態を復元するとき、永久LP receiptに保存された決済直前の市場からRFC-0064の整数quoteを再計算し、買付数量・LP投入量・burn・決済直後の市場全体を照合する。LPを発行しない少額決済では、決済後のreserve、buyback累計、Position在庫とretired累計を逆算してから同じ計算を適用する。delta取込でも同じquote検査を使い、同一ブロックの実際の市場差分と予算debitを引き続き照合する。

MOE2単体8件が成功した。隔離3プロセスの登録・決済・reorg fixtureは2.35秒で完了。3者とも買付4,800,000,000 sompi、永久LP投入3,200,000,000 sompi、Position burn 1単位、Miner 6,400,000,000 sompi、Panel 1,600,000,000 sompiを観測した。正常な保存状態の再読込と巻戻しに成功し、決済前市場のbuyback累計が変わった状態、およびreceiptとquoteのburn量が同時に1単位ずれた状態の復元は拒否された。fixtureのwork識別子は会計試験用に与えている。実LLM受賞またはEVM残高との一貫決済はこの試験の検証範囲外。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_model_market_v1::moe2
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --test-threads=1 --nocapture
```

### TFO3の後続attemptで使う配分照会

通常RPCの `model-allocation` は、配分を計算したPALW状態の `tipBlock` と、通常producerと同じ候補DAA・会計関数で求めたbase modelのブロック枠判定を返す。この判定は返却するモデル一覧の件数に依存しない。private決済ランナーは、照会の前後で選択親とvirtual DAAが安定し、応答が指定親を指す場合に待機枠を消費する。旧RPCでモデル一覧が省略された場合は、個別照会の親・候補DAA・base modelを照合する。旧ノードの応答に親hashがない場合は前後のDAG照合を使う。配分が不足した場合は待機枠を保持し、同じDAAの別の親や更新された候補DAAで再照会できる。epoch境界の比較には配分照会の候補DAAを使う。

`python3 -m unittest discover -s scripts/drills/tests -p 'test_protected*.py'` は100件成功。選択親とvirtual DAAそれぞれの移動、個別照会の親・DAA・モデルの不一致、epoch最終DAAでの不足、同一DAAの別親での枠回復を検査した。DAA 8602・8667の保存世代をそれぞれ3ピアで `--cold-award-settlement` により再照合し、同じ受賞outpoint、受付7867、frontier 7866、未決済を確認した。待機枠のfixtureは後続attemptの実受理を実証するものではない。

同じビルドで `palw_state_v2::tests::bond_budget_fold_v1::` の34件が0.11秒で成功した。devnet管理26件も成功。保存世代の `--cold-lead-room` を再実行し、候補DAA 8603、8604、8668の残量と判定が各3ピアで上表と一致した。

### Pruned IBD試験での保存台帳の扱い

保存済みのmaterial・FP台帳を使うnative importとTCP IBD試験、およびRound 16の復旧・TCP IBD試験は、RocksDBを開く前に作業用DBを作成する。macOSとLinuxではdevnet snapshotと同じファイル複製処理を使い、clone非対応時は疎ファイルのデータ区間をコピーする。コピー先には512 MiBの空きを残す。POSIXでは別プロセスがRocksDBの既存LOCKを共有ロックし、稼働中のwriterと競合した時はコピーを開始しない。LOCKがまだないcheckpointでは、コピー中に最初のwriterがLOCKを作成すると作業コピーを破棄する。

Round 16保存DBをmacOSの同一APFS volumeで複製した。元のLOCKを除く117ファイルのSHA-256が既存manifestと一致した。コピーは0.141秒、論理871,189,299バイト、コピー直後のファイル専有データは0バイト。ディレクトリ・ファイルシステムの管理領域はこの値に含まない。別の短時間 fixture ではRocksDBの同期WAL書込後、SST生成前にプロセスを終了した。コピー上で値を復旧して更新した後も、原本7ファイルすべてのSHA-256が一致した。

`python3 -m unittest discover -s scripts/drills/tests -p test_copy_closed_rocksdb.py` は4件、`test_palw_devnet.py` は26件成功。前者は同時writer、コピー中のwriter起動、未開封checkpoint、容量不足と既存のコピー先を検査する。この測定はmacOS上で行った。

S3でもDAA 8602・8667のpeer 0を各1回複製し、作業コピーから実受賞を照合した。それぞれ163ファイル、論理11,100,459,593バイトと11,256,999,196バイトで、複製直後のAPFS専有データはともに0バイト。元の世代ごとのsink、state root、報酬outpoint、受付7867、frontier 7866、未決済状態が一致した。[S3の作業コピー測定](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-closed-db-copies.json)

```sh
python3 scripts/drills/copy_closed_rocksdb.py "$db" "$scratch"
"$reader" --cold-award-settlement "$scratch" "$policy" "$salt" "$receipt_block"
```

新しいコピー経路を通る `verify_recovery_pruning_snapshot_at_selected_successor` は1.17秒で成功した。Round 16のDAA 13798でUTXO 73件、EVM commitment、クラス宣言2件、直後のヘッダーに対するPALW rootを照合した。復旧の前後で原本118ファイルのSHA-256が保存manifestに一致した。空の受信ノードへのTCP IBD完了は引き続き未確認。

### 停止ホームと保存世代を合わせた容量

`scripts/palw-devnet storage --home "$HOME/.local/share/misaka-tfo3-award/network-s3"` の `uniqueFiles` は、ホーム・snapshot・復元前バックアップを通じて同じdevice/inodeを一度だけ数える。各ツリーと全体の `scopePrivateBytes` は、その範囲に全ハードリンクがあるファイルの専有データ量。範囲外にもリンクがあるinodeは `externallyLinkedFiles` に数える。APFSの共有clone extentとディレクトリ管理領域は専有データ量の対象外。ファイル専有量を取得できない場合、対象ファイルを含む専有量はnullとなる。

停止中のS3ホームと隣接9ディレクトリ（復元前バックアップ1件を含む）で、123,128ファイル、論理230,866,673,401バイト、inode別の割当合計231,138,308,096バイト、APFSファイル専有40,603,078,656バイトを観測した。世代間のハードリンクによる重複計上はこの台帳では0件。DAA 8602から8667の保存世代で3ピア合計のconsensus SSTは61,044,057バイト増、blobは変化なし、WALは114,599,049バイト減、全区分の論理合計は56,986,846バイト減だった。この65 DAA間の差から長期増加率は算出していない。

`python3 -m unittest discover -s scripts/drills/tests -p test_palw_devnet.py` は27件成功。ハードリンクfixtureで、同じホーム内の別名、ホームと保存世代の共有、診断範囲外にあるリンクを区別した。

### TFO3決済待ちのFinalとProvisionalのanchor

通常RPCの `claim-lifecycle` は選択親のhash、Gate Cのcommit時点・現在・抽選に必要なanchor span、未抽選の期限DAAを返す。privateランナーはFinalとclaim IDと選択親を照合し、現在のspanが必要値に届いたらGate Cのための追加attempt待機を終了する。未認証の後続Finalがすべて認証失敗で終了し、進行中の後続claimも0件なら、新しい通常attemptの待機枠を使える。台帳で確認できた異なるclaimの件数とsettlementの未認証Final件数を突き合わせる。署名・抽選・受理・frontier更新は引き続き通常の合意処理が行う。

S3の保存2世代×3ピアで、後続Finalは受付DAA 8247、Gate Cのcommitは8527、必要spanは8403、現在spanは8402、未抽選の期限はDAA 10687だった。もう1件の後続Provisionalもbind基準8402の後のPanel seedを待っていた。6つのDBすべてで、通常RPCと共通のviewが保存行の独立したデコードと一致し、そのviewを使った決済ランナーは後続attemptが必要と判定した。これは停止DBを使った判定試験で、公開RPCへの接続や後続ブロック受理は実施していない。[世代別のanchorと選択親](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-covering-anchor-clocks.json)

```sh
cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-clock --example tfo3-consumed-receipt
"$reader" --cold-award-settlement "$db" "$policy" "$salt" "$receipt_block"
python3 -m unittest discover -s scripts/drills/tests -p 'test_protected*.py'
```

短時間試験100件成功。必要spanの直前・到達・通過、終了済みの拒否、重複claim、別の未確定claim、選択親の移動を含む。

### 再開・reorgをまたぐTFO3受賞の監査資料

DAA 8602と8667の保存世代を調べると、実受賞時のrunには観測JSON、native validatorのreport、その2ファイルのSHA-256を記録したvalidationファイルがあった。旧ランナーが決済待ちを再開したrunには後ろの2ファイルが欠けていた。再開処理は3ファイルを元のバイト列で複製し、reportとcheckpoint内の監査内容、validationに記録された2つのhashを照合する。新しいcheckpointはvalidation自身のSHA-256も保存する。新しい形式で資料が欠けるか変わっていれば再開を拒否する。旧形式で両方の付属ファイルが欠けていれば、同じ保存ホームに残る過去runから同一観測の資料を探す。見つからない旧形式は従来どおりの資料だけを引き継ぐ。

両保存世代の最新checkpointから一時ディレクトリへ2回続けて資料を引き継いだ。各世代で3ファイル193,715バイト、validationのSHA-256は `71b68e63c8937a415c90cad24d131d78bf4dbf99d9a26403cc8b51230196b50d`。当時のvalidator実行ファイルのhashを照合して再実行し、同じ受賞ブロックとquantum 0のreportがcheckpointと一致した。reorg時の個別受理ログは、退避済みの旧ログが欠けていても残るログを検索する。各ピアの受理イベントが必要な条件を維持する。`python3 -m unittest discover -s scripts/drills/tests -p 'test_protected*.py'` は102件成功。実受賞の決済は引き続き未確認。

### MODEL_STOPの適用先と元genesisの保護

非公開MAS1の案件受付と停止適用は、それぞれ対象classを指定してnativeの選択親でartifact rootを照合する。登録が見つからない場合、親またはrootが異なる場合は拒否する。ノードに固定されたgenesis登録（従来形式・TIR形式）にそのclass IDがあれば、BFTの管理対象から除く。停止の署名と包含を検査する間は、選択親とpruning import世代の安定も確認する。これらはローカルな適用条件で、既存の署名本文・wire IDは共通。

隔離3プロセスでartifact/profile停止を2.34秒、共有rule停止を2.29秒で検証した。各ピアで正規登録された追加モデルに4席中3署名の停止を適用でき、市場を保持した。元genesisへの案件と停止適用、追加classにgenesisのartifactを渡した組合せ、reorgで外れた停止recordは拒否された。これは既存のMODEL_STOPを通じた保護の確認。MAD2のYES/NO採用、再審・解除、通常ネットワークでの有効化は未達。

```sh
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --test-threads=1 --nocapture
MISAKA_TEST_MAS1_RULE_STOP=1 RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --test-threads=1 --nocapture
```

### Pruned IBDでUTXO供給元の世代が進んだ場合

供給側は要求されたpruning pointをチャンクごとに照合する。受信途中でその世代を供給できなくなると、通常のP2Pで `UnexpectedPruningPoint` を返す。従来の受信処理では、この通知がstaging切替後の一般的な合意エラーに入り、初回でノードが隔離されていた。

受信側は対象のpruning pointを記録して接続を閉じ、PALW snapshot欠落時と同じ再取得処理を使う。切替後の未完了importでは、現在のIBDが所有権を持つ場合に限り、参加を保留したまま別ピアとの再同期を許す。切替後の連続不足が3回に達した場合は隔離する。途中で受信した行は次回importでclearする。UTXOのMuHash照合とimport完了の条件を維持する。

短時間fixtureは同一のP2P受信routeへ正規形式のUTXO 1件と世代変更通知を順に送り、行の復号結果と再取得エラーの対象pointを照合する。実ピアを切り替えた後のTCP同期完了は未確認。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-p2p-flows --lib utxo_stream_reports_generation_loss_after_chunk
cargo +1.93.0 test --locked --offline -p kaspa-p2p-flows --lib a_sidecar_shortfall_is_retried_and_a_chain_failure_is_not
```

P2P flowsのlib試験は211件成功、1件ignored（35.25秒）。既存のconsensus試験バイナリで `rfc12_process_kill_matrix_preserves_import_epoch_money_and_publication` を再実行し、8か所の中断・再起動で旧epochの書込み拒否、再取得後のUTXO・金額・公開状態を確認した（4.61秒）。

### TFO3決済照会で使う選択ブロック

通常の `getPalwSettlement` は、照会時のsinkとPALW状態を取得したブロックのhash・DAAを両方照合する。以前は読取前後のsink・pruning point・import世代を照合していたが、間で取得したPALW状態そのものにはDAAの比較しかなかった。分岐先が同じDAAの場合でも食い違いを検出して再照会を求める。適用済みブロックがまだない状態は、そのネットワークのgenesisでのみ使用する。

DNS retirement前の `available` はPALW frontierのDAAを特定できたか、retirement後はnative snapshotを取得できたかを返す。後者でsafe blockが指定され、そのヘッダーを読めない場合はRPCエラーを返す。保存台帳の `--cold-award-settlement` も決済RPCと同じブロック照合を使う。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_settlement_v1::tests -- --test-threads=1
cargo +1.93.0 test --locked --offline -p kaspa-rpc-service --lib palw_settlement_read_tests -- --test-threads=1
cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-clock --example tfo3-consumed-receipt
"$reader" --cold-award-settlement "$db" "$policy" "$salt" "$receipt_block"
```

保存2世代×3ピアで選択ブロック、state root、巻戻し件数と実受賞のoutpoint・金額が[保存時の証跡](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-cold-award-settlement.json)と一致した。各ピアの選択先端DAAは8602世代で8602・8601・8602、8667世代で全て8667。全6件で320,084,650,080 sompiの報酬の受付DAAは7867、frontierは7866、未決済だった。

決済の短時間試験9件とRPCの試験2件が成功した。決済fixtureでは共通の親から同じDAAの子を2つfoldし、双方で自分のhashによる照会が通り、相手のhashによる照会が拒否された。実受賞の決済後reorgと後続Gate C成立の実証は未達。

### Pruned IBDの履歴構築・検証用の一時RocksDB

供給側のpruning履歴構築と受信側の検証は、従来OSの一時領域にRocksDBを作り、ノードの容量集計から漏れていた。容量制限付きノードでは両方を `storage-tmp` 以下に置き、開始前にWAL・compaction・manifest・ログ用に352 MiBを予約する。実ファイルも定期集計に含める。352 MiBは一時増幅の予算で、実データを含むDB全体のサイズは処理した履歴に依存する。

最後のDB参照を閉じると一時世代と予約を解放する。Unixの異常終了後は、専用マーカー、経過時間、所有ロックの解放を確認できた世代だけを回収する。所有者不明の旧一時ディレクトリは保持する。Unix以外では正常終了時の解放に対応する。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-database --lib -- --test-threads=1
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib processes::pruning_proof:: -- --test-threads=1
MISAKA_PRIVATE_GENESIS_SUFFIX_DB="$sealed_round16_db" cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib round16_build_validate_pruning_history_with_storage_cap -- --ignored --nocapture --test-threads=1
```

30,000,000,000バイト上限の短時間fixtureで、Round 16台帳の複製からpruning point DAA 13798の履歴27,826ヘッダー枠を34.41秒で構築し、独立検証が23.76秒で成功した。100 ms間隔で観測した一時DBの最大割当は138,063,872バイト。処理後の一時世代数と未解放予約はともに0。試験前後に原本118ファイルのSHA-256が保存manifestと一致した。DB層33件、pruning履歴の単体試験33件も成功。Round 16全体のTCP同期、S3の後続決済、定常運転時の最大使用量はこの測定の対象外。

同じ一時領域にはtokenizer workerが開いたまま使う名前付き資料もある。Unixの通常ファイル回収は、保存期間を過ぎたsnapshot spill専用の名前だけを対象にする。Windowsのsnapshot spillはOSの最終ハンドルcloseで消える。容量管理の短時間試験11件が成功し、期限を0にしたfixtureでもtokenizer資料が保持され、回収対象のspillだけが消えた。`cargo +1.93.0 check --locked --offline -p kaspad --lib` も成功した。

### TFO3の決済待機で照会するproducerとGate Cの履歴表示

S3の保存設定ではpeer 0がheartbeatを担当し、peer 1が `--palw-produce` で起動する。決済ランナーは全ピアで受賞を照合するが、後続attemptに割り当てる待機枠では、指定producerのRPCで選択親・virtual DAA・モデルの配分を読み、同じproducerの親が進むまで待つ。保存DAA 8602世代の実測では、peer 1の選択先端は他の2ピアより1 DAA遅れていた。短時間fixtureでは、観測ピアとproducerが同じDAAの別親にいる間は枠を使わず、producerが指定親を選択してから配分照会と待機を行った。

`claim-lifecycle` は旧 `public-check-certified` を履歴コードとして返す場合に `securityCertified=false` を返す。運用コマンド `t12check.py --g14` も当該行を認証済み件数から除外する。後続Finalがこの歴史的な終了状態に達した場合、決済ランナーは他の未決anchorを照合してから、新規attemptの要否を判断する。S3保存2世代の各3ピアを読み取り専用で再照合した。全6件で実受賞のoutpoint・金額と各先端のstate rootが保存証跡に一致し、後続Finalはpending、報酬受付7867、frontier 7866、未決済だった。各台帳の読取値を渡したランナーは後続attemptが必要と判定した。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib rfc18_a_receipts_wait_for_security_certification_and_count_from_it
python3 -m unittest discover -s scripts/drills/tests -p 'test_protected*.py'
```

短時間試験では旧履歴と抽選認証済みの2つの親で、claim-lifecycleの表示とreceipt受付が一致した。決済・登録ランナー等の試験は103件成功。運用レポートに両履歴を1件ずつ与えた場合の認証済み表示は2件中1件だった。

### 保存S3の登録後台帳での分岐切替と直接適用の比較

登録済み・Studio投入前の保存世代 `539994e02c9f40e6b134822d02a69cb5` の複製を使用。2ピアで同じ親から6ブロックを直接延長し、その先端のPALW rootを記録してDB複製を解放した。別の3ピアは同じ親から2ブロックを延長した後、1ピアだけに競合する6ブロックをRPCで提出した。3ピアすべてで選択履歴から2ブロックが外れ、直接延長した6ブロックが同じ順序で加わった。切替前のrootは直接延長側と異なり、切替後の3ピアのrootは直接延長側2ピアのrootと一致した。各ピアの読取前後で選択親とimport世代を照合した。

初回は120秒で3ピアの起動途中に終了。再試験は上限235秒、135.73秒で完走した。終了後に保存原本を全1253項目のmanifestと照合した。受賞前の台帳なので、実受賞の決済や決済後のreorgは引き続き未検証。[ブロックの切替・各ピアのroot・原本照合の記録](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-registered-reorg-roots.json)

```sh
python3 scripts/drills/palw_devnet_reorg.py --registered-preflight --home "$S3_HOME" --snapshot 539994e02c9f40e6b134822d02a69cb5 --port-base 48370 --output "$NEW_OUTPUT"
python3 -m unittest discover -s scripts/drills/tests -p 'test_protected*.py'
```

決済済みfixtureでは実受賞claimの独立Gate C、消費済みquantum、報酬outpointと決済anchorも照合する。ランナーの短時間試験104件が成功し、別親、読取途中のimport世代変更、必要なGate Cがpendingの応答を拒否した。

### 未決済のS3実受賞を保持した分岐切替

DAA 8667の保存世代 `72e6a8b458184ee6ab3b67ac939f918b` から、2ピアで6ブロックを直接延長した。別の3ピアでは2ブロックを延長した後、1ピアに競合する6ブロックを提出した。全3ピアで同じ2ブロックが外れ、同じ6ブロックが加わった。直接延長側と切替後の選択先端DAAは8670で、PALW rootが一致した。元の短い分岐でも、直接延長側でも、切替後でも、受賞時と同じoutpointの320,084,650,080 sompiが未使用、quantum 0が消費済み、受付DAA 7867、frontier 7866だった。

分岐切替後の各ピアが供給した選択ヘッダーのcarriageと、受賞当時の監査で保存した親・適用後のcarriageを使用した。native validatorは受賞と支払ヘッダーのcommitmentを照合し、同じ量子の再提出に `QuantumAlreadySpent` を返した。停止後に各ピアのDBで受賞ブロックのjournalを巻き戻して再適用し、3件とも元の状態を復元した。S3本体は停止したまま、Studioへの再投入は0件。後続Gate C成立と決済後のreorgは未達。[各ピアの受賞・分岐・journal証跡](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-pending-award-reorg.json)

保存世代の起動時には `private-palw` 圧縮を使用していた。これを引き継がない試行は12 GiBの空き容量下限で停止し、30 GBのノード上限と小さいキャッシュを指定した試行でも同じ下限に達した。reorg試験と通常のprivate再開で各ピアの保存ログからRocksDB presetを引き継ぐようにした。明示的な環境設定を優先する。圧縮を引き継いだ完走時は205秒、空き容量の2秒間隔最小観測値は16,435,978,240バイト。時間上限275秒と空き容量下限12 GiBは外側の試験監視で適用した。

```sh
KASPAD_RAM_SCALE=0.25 KASPAD_STORAGE_CAP_BYTES=30000000000 \
python3 scripts/drills/palw_devnet_reorg.py --pending-award --home "$S3_HOME" \
  --snapshot 72e6a8b458184ee6ab3b67ac939f918b --port-base 48370 --output "$NEW_OUTPUT" \
  --consumed-validator "$READER" --award-audit "$ORIGINAL_CONSUMED_AUDIT"
python3 -m unittest discover -s scripts/drills/tests -p 'test_protected*.py'
```

古い受賞状態をRPCで再取得した試行は、分岐切替後に6秒の接続用timeoutで終了した。接続後は保存設定の処理用timeoutを使用する。完走時は保存監査から2つのcarriageの長さとSHA-256を照合して読み込み、選択状態と受賞の検証を実行した。試験後に保存原本19,264項目をmanifestと再照合した。関連短時間試験107件とdevnet管理試験27件が成功した。

### S3実受賞の決済（DAA 8703）

DAA 8667の保存状態から既存S3を1回再開した。制限は300秒、空き容量下限は12 GiB。追加のStudio推論は0件。通常のproducerでモデル割当の余裕を確認した親を12.52秒保持し、生成したattemptがDAA 8667で選択履歴に収録された。3ピアの停止DBで生成ブロック、受理ブロック、claim、deltaを確認した。

受付DAA 8247の後続claimは、DAA 8527でFinalとGate Cのcommitに達していた。新しいattemptでanchor spanが8402から8667に進み、必要値8403を超えた。DAA 8667で75枠を抽選し、4件のattestationが44枠を承認した。閾値は41枠で、選択台帳の認証結果はDAA 8703でCertified。safe frontierが7866から8247に進み、元の受賞UTXO（受付7867、320,084,650,080 sompi）は未使用のまま決済済みとなった。

3ピアは同じ選択先端で同じ決済応答を返し、ランナーは正常終了した。決済済み世代は `bda15b77344d4277b7a1a63c690da3a1`。その3つの停止DBでroot、受賞outpoint、金額、後続attempt、Gate Cとfrontierを再照合した。DAA 8602・8667の保存世代も保持している。[実走と保存DBの照合結果](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-settled-8703.json)

決済後の保存世代を再起動し、2ピアで6ブロックを直接延長した。別の3ピアでは2ブロックを外して同じ6ブロックを採用した。切替後のDAA 8707で、3ピアのrootは直接延長した2ピアと一致した。元の受賞・決済は共通祖先にあり、分岐前後で同じ報酬outpoint、決済frontier 8247、量子0の消費を確認した。再提出は `QuantumAlreadySpent` で拒否され、3つの停止DBで受賞ブロックのjournalを巻き戻して再適用し、全状態を復元した。実行は218.1秒。終了後、保存世代の全19,394項目がmanifestと一致した。[決済後の分岐と再適用記録](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-settled-reorg.json)

再現時は上記reorgコマンドから `--pending-award` を外し、決済済み世代を指定する。使用したRocksDB presetは `private-palw`、外側の監視は300秒、空き容量下限12 GiBとした。

### Pruned NodeでのRound 16取込完了と後続ブロック

genesisだけを持つ受信側がTCPで全74,920ヘッダーを検証し、DAA 13798のoverlay・EVM・PALWと73件のUTXOを取得した。修正前はstaging採用後、UTXOをvirtualに適用する際に公開ビューからnative決済を再評価していた。取込中はこのビューが不完全として停止するため、検証済みのEVM latestが消え、同じ世代の完了検査で拒否されて参加は隔離された。

DNS retirement後の非genesis取込では、EVM sidecarで検証・設定したlatestをUTXO適用中も保持するよう修正した。取込の完了には、選択親・UTXO位置・PALW・overlay・EVM実行とlatest・import世代の従来の照合が必要。通常の先端選択では引き続きnative決済からheadを導出する。

修正後、受信側は安定状態になったDAA 13798で、Archival側と同じPALW rootと73件すべてのUTXOを返した。後続ブロック本体は最後の進捗出力が27,027件（36%）で、300秒の観測期限に達した。先端DAA 51258との一致を求める試験全体はFAILED。空き容量の最小観測値は15,189,344,256バイト。原本118ファイルを試験前後で保存manifestと照合し、関連の短時間試験9件が通過した。再現には上記のRound 16 TCPコマンドを使う。[取込前後と先端同期の観測結果](../evidence/palw/pruned-ibd-2026-11-25/round16-tcp-import.json)

### Round 16のブロック本体同期と報酬検証

DNS retirement後のinclusion配分は候補の選択親にあるclaimを参照する。通常のUTXO検証はこの状態を候補ごとに保持していたが、Manifest要求を含まない候補では報酬検証への引き渡しを止め、保存tipから差分をたどり直していた。Manifestの有無にかかわらず、同じ候補の検証で保持している選択親を報酬検証に渡すよう修正した。親状態の供給がない呼出しでは引き続き保存状態を照会する。

同じRound 16台帳・300秒上限のTCP fixtureで、取込点DAA 13798のrootとUTXO 73件が再び一致した。終了までのログに記録された親の再構築は27,564回から0回に減り、旧経路が再適用した差分は1,360,465件だった。試験の標準出力・標準エラーの合計は13,258,856バイトから99,731バイトになった。期限前の本体処理の最終出力は27,027件から43,857件（58%）に進んだ。両測定とも先端一致の試験は時間切れ。原本118ファイルを保存manifestと再照合した。RFC-0012の17試験と、pruning pointで支払待ちを引き継いだノードの発行額をArchivalと比較する1試験は成功した。[前後の測定値](../evidence/palw/pruned-ibd-2026-11-25/round16-validation-parent.json)

## 2026-11-25からの完了計画（今回を含め全9セッション予定）

Codex単独で下表の順に進める。第8回を通常経路の接続と全対象状態・容量予算の合格目標、第9回を最終総合判定とする。1回は終了条件と証跡を持つ作業単位。回数は見積りであり、試験失敗を理由に判定を通さない。各回の終了時にこの表に実績、commit、再現コマンド、結果、残件を追記する。見積りを超える場合は、その回で原因・追加回数・新しい到達予定を記録する。

| 回 | 担当範囲 | 終了条件 |
|---|---|---|
| 1 | 基準点と範囲を固定 | 保存S3の決済証跡、Pruned IBDの成功地点と失敗地点、5経路の不足と検証順を記録。完了 |
| 2 | Pruned IBD・容量 | 再起動後の再同期開始点を記録し、通常P2Pで同じ選択tipまで到達。root・全UTXO行をArchivalと照合。試験前に台数・保存世代・測定期間・ノード別と全体の物理容量上限を数値で固定し、初期同期、定常時、compaction、snapshot更新のピークと空き容量を測る。媒体の空き下限12 GiBを維持 |
| 3 | TB1 | 指摘1・3の単独パッチを候補ブランチと照合して取り込み、通常holderとNodeでEXEC/PANEL双方の資格・share配信・期限・停止・再起動を検証 |
| 4 | FOW1 | 実LLM内部Readoutから契約、commitment、Ticket、Court、Final、権利消費まで同じclaimで追跡。未認証と二重消費を拒否。採択仕様上の評価条件ごとに結果を記録 |
| 5 | QF1 | REVIEW_HOLD下の隔離設定で通常受付を通し、Qwen3 artifact・tokenizer・trace・Readout・裁定・Final後の権利を照合。反証期限内の資料取得、期限後の回収、reorgを確認 |
| 6 | MAD2 | 通常の登録・fork適用からkernel自動採用、BFT YES/NO、停止・再審・解除・履歴を追跡。genesis保護・権限・原子的反映を検証 |
| 7 | MOE2 | 独立修正を照合し、通常quoteから同一受賞権利の一回消費、native/EVM残高、支払・市場更新・巻戻しを照合。途中失敗で片側だけ更新されないことを確認 |
| 8 | 全対象状態・容量の合否 | 同じ選択tipでPrunedとArchivalのroot・UTXO・claim/Court・保持資料・Gate C/frontier・Ticket/消費・報酬/担保・モデル/市場・native/EVMおよび各経路の状態を比較。未決、決済済み、pruning境界前後、再起動、reorg復帰を含める。行・残高・取得可能資料まで照合し、事前予算に対するピーク、物理増加/DAA、未決claimあたりの容量、回収量、世代数を記録。5経路の通常入口からの短時間試験が全件成功 |
| 9 | 最終総合試験 | 第8回合格後に保存S3を活用して統合確認。実受賞・決済・3ピア一致・障害復旧を確認し、再現手順と制約を確定。公開候補の判定と公開有効化の承認を個別に記録 |

11月26日の進捗は第4回の途中。第8回の判定まで今回を含め残り5作業単位、第9回の総合判定まで6作業単位を予定する。第2回はRound 16の終端DAA 51258と切断終了まで確認済み。第3回の通常holderの自然成熟とshare配送を持ち越す。第4〜7回で使う同一claim・選択tip・保存世代の識別子を引き継ぎ、第8回で未決Courtなどを含む台帳・保持資料と物理容量予算を照合する。実ディスク空きが12 GiB下限を下回る期間は保存DBの実走を停止し、RAM上のfixture結果を物理容量予算の実測には算入しない。

11月26日の再計画時点からは、実際に作業したセッションも数える。現在のセッションを残り1本目とし、第8回判定まで5セッション、総合判定まで6セッションを割り当てる。途中で終了したセッションも消化数に含める。各終了時に完了条件、未達項目、次のセッションで実行する試験を記録する。予定した条件を満たせなかった場合は同じ回番号で消化数を据え置かず、最終判定までに必要な追加セッション数をその時点で再見積りする。

| 現在からのセッション | 引き渡す証跡 |
|---|---|
| 1（現在、第4回） | 第3回からのTB1自然成熟と通常holderによるEXEC・PANELのshare配送。FOW1は同じOriginalについて通常の成功Final、独立Gate Cの肯定票、価格付けと権利消費を追跡。 |
| 2（第5回） | QF1の隔離設定で通常受付から反証期限前後、資料回収、reorgまでの結果。 |
| 3（第6回） | MAD2の通常登録から採用・投票・停止・再審までの履歴と復帰後の状態。 |
| 4（第7回） | MOE2の独立修正を照合し、同一権利の決済、native/EVM両側の残高、二重消費拒否と巻戻しを記録。 |
| 5（第8回） | 5経路の通常入口からの試験結果、同一tipでの全対象状態の差分、物理容量ピークと事前予算の対照表。容量を実測できない場合は未判定を記録。 |
| 6（第9回） | 第8回合格を前提に、保存S3の実受賞・決済・3ピア一致・復旧の証跡を揃えて総合判定。 |

第1回の基準はT12の `54af90403`。既存S3の実受賞決済と3ピアreorgは `d6d48c8b0` に証跡がある。第1回終了時のRound 16はDAA 28004で停止し、cold reopen直後の選択tip・root・全73 UTXO行が一致した。保存元118ファイルのSHA-256照合も成功。その後の再同期範囲、終端までの検証、切断試験は下記の第2回の記録を参照。TB1の独立パッチは `33db510de`、custody試験9件成功。

DAA 8602の保存台帳を維持する。第8回までの検証は保存世代と短時間fixtureを使う。公開有効化、QF1のREVIEW_HOLD解除、VPS配備は別途承認を要する。

### 第2回の測定予算と再同期の開始位置

短時間のRound 16 fixtureはArchival供給元1台とPruned受信側1台。各ノードの作業ツリー（管理DB、WAL、staging、spillを含む）は各30,000,000,000 B、2台と試験作業領域を合わせて60,000,000,000 B以内とする。上限の基準は `kaspa_utils::storage_budget::RECOMMENDED_STORAGE_CAP_BYTES`。共有ボリュームで実際に増やせる量は開始時空き容量から12 GiBを差し引いた量にも制約される。初期同期300秒、再起動後240秒まで。保存世代は元台帳を固定し、ローリングは最新2世代。3ピアS3については各稼働ノード30,000,000,000 B、3台計90,000,000,000 Bを同じ基準で照合する。固定台帳、ローリング世代、非ノード資料、ボリューム実空きは別計上する。これらの予算は短時間検証用で、公開運用の容量値は第8・9回の計測で判定する。

Round 16では初回74,920件に対して、DAA 28004からの再接続時に要求対象とされたbodyは46,309件（28,611件減）。ヘッダは同期済みで、接続から不足body列挙の完了まで30.8秒を要した。ログには個々の要求hashがないため、件数差だけで全件の非重複までは判定できない。

停止中のS3台帳を読み取り、DAA 8602、8667、8703で3ピア合計の割当量はそれぞれ33,918,947,328 B、33,860,116,480 B、35,025,559,552 Bだった。最大の単一ピアは11,781,742,592 B。停止時の3標本は上限内。compaction中の最大値、世代をまたぐ共有extentの実占有量、未決claimあたりの増加はこの測定の対象外。[各ピアの割当バイト](../evidence/palw/pruned-ibd-2026-11-25/s3-closed-peer-allocation.json)

通常IBDの選択チェーン上の区間では、mergesetに対する開始点の祖先照会を省略した。各段のselected parentは開始点かその子孫で、残りのmergesetはそのparentのanticoneにあるため、開始点自身だけ除けばよい。分岐からの要求・EXEC補助列は従来の祖先検査を通す。body要求の先頭・末尾hashとDAAを既存の進捗ログに追加した。短時間試験3件が成功。対象は祖先述語との全列比較（選択枝・合流する枝・同一tip）、EXECを含むヘッダとbodyの同期、未決報酬のあるpruning pointを取り込んだノードとArchivalの発行一致。Round 16終端の再測定は未実施。

再現は各テスト名 `ibd_across_a_round_lane_whose_blocks_tie_on_blue_work`、`t12_exec_v2_ibd_carries_the_anchored_lane_blocks_through_both_sync_lists`、`p2_t49_a_node_that_imports` を順に次のコマンドに渡す。

```sh
RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib "$test_name" -- --test-threads=1
```

### 第2回の終端一致・容量測定

Round 16の保存供給元から通常TCP IBDを開始し、pruning point DAA 13798のPALW root・全73 UTXO行を照合した。DAA 28004で閉じた受信DBを別プロセスで照合して再開した。最初の240秒枠はDAA 36271で時間切れとなり、そこまでの状態を保存した。その次の枠では残り29,776 bodyを受信し、DAA 51258でIBD完了、同じ選択tip・pruning point・PALW root・全73 UTXO行をArchivalと照合した。受信側の新規staging作成数は0。終端合致直後、供給側で切断後も履歴要求を処理しており、テスト用終了待ち10秒が満了したため、このP2P試験自体の終了コードは101だった。

終端DBを独立プロセスでcold reopenし、Archivalの選択tip・root・全UTXOと再照合した。通常ワーカーを10秒動かした後も一致し、両ノードの終了と所有権解放を含めてこの試験は成功した。fixtureを受信側が閉じた時点で観測値を保存する順序にし、供給側の履歴処理には別の終了待ちを設定した。修正後に同じ全履歴を再生して供給側の終了待ちを再現する試験は未実施。[各区間の成否・終端root・容量標本](../evidence/palw/pruned-ibd-2026-11-25/round16-cold-terminal-capacity.json)

初期同期・再開・compactionを含む区間・終端アイドル状態を1秒間隔で採取した。S3の受賞・決済を含む3保存世代からpeer-0のPALW保持資料と停止記録を取り出した小型fixtureで、通常snapshotの固定1世代・ローリング3回を作成した。公開・削除時の実ディレクトリ同期後に採取した43標本の最大は56,303,616 B。最新2世代と固定世代が残り、DAA 8667と8602の復元資料は元世代と内容が一致した。上限は短時間fixtureの標本について判定した。公開運用の継続的増加量、ノード全体のsnapshot更新ピーク、全機能の資料保持・経済状態は第8・9回の対象。

再現にはEVM有効の `kaspa-consensus --lib` テストバイナリを使う。閉じた供給元と新規受信パスを指定し、各ログには新しいパスを使う。再開の時間切れ時は保存に成功したDAAと終了理由を確認し、同じ受信DBから次の枠を実行する。終端確認は独立して実行できる。

```sh
python3 scripts/drills/measure_round16_receiver.py prepare --binary "$TEST_BIN" --source "$DB" --receiver "$RECEIVER" --log "$LOG1"
python3 scripts/drills/measure_round16_receiver.py resume --binary "$TEST_BIN" --source "$DB" --receiver "$RECEIVER" --log "$LOG2"
python3 scripts/drills/measure_round16_receiver.py verify --binary "$TEST_BIN" --source "$DB" --receiver "$RECEIVER" --log "$COLD_LOG"
python3 scripts/drills/measure_s3_retained_snapshots.py "$S3_GENERATIONS"
```

第2回の残件を保存済みDAA 51258で再検証した。終端root・UTXO一致を確認した受信側から実TCPで履歴を要求し、供給側が履歴キャッシュのロックを保持して読み取りを開始した時点で切断した。切断から全flow・manager所有権の解放まで38.66秒、試験成功。元の10秒では足りなかった終了条件を再現し、90秒の供給側終了枠内で両ノードを閉じた。供給元118ファイルのハッシュも再照合した。再現は上の計測コマンドのphaseに `disconnect` と新しいログパスを指定する。第2回の短時間fixtureに対する終了条件は達成。全仕様の状態一致と公開運用予算は第8・9回で判定する。

### 第3回 TB1 custody

独立パッチ `33db510de` の停止・再試行判定をT12のholder署名経路に統合した。公開済みのRust呼出形式は `io::Result` を維持し、custody由来の停止理由をエラーに格納する。元のjournalのpath/inode検査、署名後のfinality再読込、pruning import世代検査を保持した。停止の記憶は署名、予約、観察の各入口でnative I/Oに優先する。停止マーカー同期後にもleased inodeを確認する。マーカーが一切残らない場合は別プロセスが元の予約を再署名できるため、永続化失敗時はholder・roundの独立した隔離を維持する必要がある。

`RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p misaka-palw-remote --lib tb1_exposure::tests -- --test-threads=1` は12件成功。実際の署名返却、A→B→A、マーカー書込み・同期障害、journalの置換、pruning import中・世代切替、3プロセスの中断・再起動を含む。通常NodeとholderのEXEC・PANEL配信および資料の端から端までの確認は継続中。

EXEC・PANELのholderは、署名したslot・round・資格root・freeze・shareを単一行の既存TB1REL01形式で渡す。受信側は資格表のholder鍵で各署名を照合し、同じ行列からブロックのpayloadとcoinbase commitmentを生成する。3プロセスfixtureでこの受渡しを通した。通常の常駐holder・Node間の配信と受付への組込みは第3回の残件。

上記custodyコマンドで13件成功（4.73秒）。改変署名、別freeze、複数行packet、batch件数上限超過の拒否を含む。T12で次の短時間fixtureも成功した。EXECは選択中のブロックがfinalityを失った場合の停止まで2.32秒。PANELは登録tokenizerだけを指定しFULL経路を起動せず、実時間8.57秒（上限100秒、空き容量下限12 GiB）。いずれも隔離したfixtureの結果で、通常サービスの完了判定は保留する。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected -- --test-threads=1
MISAKA_PRIVATE_QF1_TOKENIZER="$TOKENIZER" "$CONSENSUS_TEST_BINARY" private_tb1_stable_panel --ignored --nocapture --test-threads=1
```

上記の環境は `RUST_MIN_STACK=67108864`。CONSENSUS_TEST_BINARYは最初のコマンドで生成したテスト実行ファイル、TOKENIZERはRFC-0066の登録tokenizer.json。

ローカルholderはcloseのブロックhashと用途を指定し、選択tipのPALW台帳から準備・公開資格・freezeを取得して署名できる。EXECは受理済みreceiptと元ブロックのcarrierを照合し、PANELは受理済みPANEL closeから対応するEXEC準備をたどる。両方で元ブロックのcommitmentを確認する。既に曝露したroundでは、新たなcloseの検索より先に旧anchorを観察する。署名の前後でpruning import世代を検査する。

登録済みのcloseを使う隔離fixtureで、各用途のholderが復元した資格で署名し、双方のcertificateが通常ブロックとして受理され、PANEL replay承認まで進んだ。旧tip指定と別用途指定は拒否された。準備状態・frontier・成熟情報は試験入力であり、既存のnative writerで成熟後のheadを反映した。試験用のholder鍵とshareを使う単一受信Nodeでの結果。常駐サービス間のVSS配信・秘密shareの復元・再起動・自動受付は引き続き要検証。

```sh
MISAKA_PRIVATE_QF1_TOKENIZER="$TOKENIZER" MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_NATIVE_REPLAY=1 RUST_MIN_STACK=67108864 "$CONSENSUS_TEST_BINARY" private_registered_tb1_pending_freeze --ignored --nocapture --test-threads=1
```

登録済みfixtureは実時間12.46秒（試験本体12.33秒、上限100秒、空き容量下限12 GiB）。finality撤回の3プロセスfixtureも2.48秒で成功した。

`cargo +1.93.0 test --locked --offline -p misaka-palw-remote --features kaspa-consensus-core/evm --lib tb1_exposure::tests -- --test-threads=1` は13件成功（4.56秒、上記と同じstack設定）。曝露後に新しいcloseを要求し、置換分岐にcloseの読取機能がなくても、先に旧anchorのfinality喪失を検知して停止し、元分岐と再起動後にも停止を保持した。

### 第3回　VSS shareの保存と再開

隔離bootstrapのholderは元round・roster・holder鍵・VSS各期限を照合し、選択鎖の公開transcriptと私的cross-checkから得たshareを保存してからReadyを返す。苦情に対するdealerの訂正がある場合は訂正後の値を保存する。保存鍵はholderが別途保持する32バイト。公開journalには秘密値を書かず、holder・round・dealerごとのファイルに終了ブロック・transcript rootと認証付き暗号文を記録する。現形式は1件242 Bで、ファイルシステムによる実割当量は別。未公開の一時名はdealerあたり最大1件。同期・再読込・復号・同値照合に失敗すればReadyを返さない。

release時はローカルNodeの受理済みcloseから有資格dealer一覧を読み、各保存shareのVSS終了ブロックが元スケジュールの位置にありcloseの祖先であることを確認する。VSSの旧ブロック本文の再読込は不要。選択台帳、必要なヘッダー・祖先情報、holderの暗号化share・鍵・曝露journalが揃うことが前提。停止マーカーを永続化できなかった場合の再起動上の制限は従来と同じ。

検証にはRust 1.93.0、locked・offline、RUST_MIN_STACK=67108864を使用した。ビルドは一度だけ行い、次のfilterを指定して各テスト実行ファイルを順次起動した。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus -p misaka-palw-remote --features kaspa-consensus/evm --lib --no-run
"$REMOTE_TEST_BINARY" tb1_exposure:: --test-threads=1
"$CONSENSUS_TEST_BINARY" private_tb1_preparation_reads_ready_from_accepted_blocks --test-threads=1
"$CONSENSUS_TEST_BINARY" native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected --test-threads=1
MISAKA_PRIVATE_QF1_TOKENIZER="$TOKENIZER" MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_NATIVE_REPLAY=1 "$CONSENSUS_TEST_BINARY" private_registered_tb1_pending_freeze --ignored --test-threads=1
```

- holder関連22件成功（試験本体4.49秒）。新規回帰では子プロセスでの復号、鍵・round・slot・終了ブロック・transcriptの不一致、改変・切詰め、公開前と公開直後の一時ファイルからの再開を確認した。
- bootstrapの準備からEXEC certificateとcold reopenまで成功（試験本体6.92秒）。各3人のEXEC/PANEL Readyを元のVSS出力の保存後に生成し、PANELの苦情解決で得た訂正値を照合。EXECは再度開いた3つのjournalから有資格dealer分だけを復元し、誤鍵を拒否、元鍵での署名と通常ブロック受理に成功した。
- 既存の3プロセスfinality撤回と登録済みEXEC/PANEL replay試験も成功した。後者のPANEL releaseは別fixtureで供給するshareを使用。常駐holderの認証私信・自動受付、PANELの保存shareからのcertificate、Pruned Nodeの全対象照合は継続対象。
- ディスクビルド停止後の観測値は12,546,336 KiBで、12 GiB下限を36,576 KiB下回った。このビルドを容量合格に含めない。中断したビルドの未リンクobjectをRAM側へ移し、既存キャッシュと保存S3を保持。最終ビルドとfixtureを一時RAMボリュームへ移し、各fixtureは100秒上限で実行した。4回のfixture中に採取した作業ディスク空きの最小値は13,017,927,680 B。

### 第3回　bootstrap holder間のVSS私信

隔離bootstrapでは、各端点がローカルNodeのgenesisに固定したplan ID、network domain、用途、dealer・送信・受信slotと署名鍵を復元する。1接続で1個のdealer配布又はcross-checkを扱う。dealer配布はf=1のrowとcolumn計4 field、cross-checkは2 field。いずれも66 Bの正規fieldに限る。ストリームの時間制限・切断は呼出側が担当する。

- 受信者は接続ごとに新規のML-KEM-768鍵を作り、公開鍵とrecord contextをML-DSA-65で署名する。送信者は登録署名鍵・KEM公開鍵を検証し、1088 Bのencapsulationとその署名を返す。
- contextはRoundRef、dealer・送受信slot、phase、sequence、本文長、両署名鍵を含む。SHA3-512の各入力をu64 big-endian長で区切る。HKDF-SHA256でtranscriptに束縛した片方向AES-256-GCM鍵とHMAC-SHA256確認鍵を導出する。受信者が共有鍵の所持を示すまで送信者は私的fieldを送らない。
- 1回の握手でデータ暗号化は1回。再送・再起動は新しい受信鍵とencapsulationで開始する。固定長のbootstrapでは同じround・dealer・送受信slot・phaseの論理sequenceは0の1件だけ。両端は暗号化した正規本文をholderの台帳に保存してfile・directoryを同期し、再読込する。再送は同一値に限る。受信側の処理が成功した後に本文とtranscriptに束縛した確認を返す。確認が失われた送信者は同じ本文で再試行できる。
- dealer配布・cross-checkの保存量は本体につき312 B・180 B。journalとファイルシステム割当量は別。復旧には元の独立storage keyとjournalが必要。受理済み成果を失った端点が新規の暗号鍵で過去の順序状態を作り直す運用は検証対象外。

検証は次の順に行った（Rust 1.93.0、locked/offline、RUST_MIN_STACK=67108864、単一test thread）。

1. `cargo +1.93.0 test --locked --offline -p misaka-palw-tb1-channel --lib -- --test-threads=1` は5件成功（0.53秒）。別プロセスのdealer配布、context符号化の固定値、再接続時の新鍵、鍵・sequence不一致、鍵確認・暗号文・確認応答の改変、受信側処理失敗を試験した。最初の別プロセス試験ではmacOSのaccept済みソケットがnonblockingを継承したため失敗。試験のaccept後にblockingへ明示設定し、5件を再実行した。
2. 前節のremote試験 `tb1_exposure::` は22件成功（4.55秒）。
3. 前節のconsensus試験 `private_tb1_preparation_reads_ready_from_accepted_blocks` は成功（7.41秒）。4 holderのPANEL dealer 2について他の3 holderへの配布と12方向のcross-checkをループバックTCPで交換した。1方向で両journal再開後の再送、dealer値変更の拒否、受信記録から全actorの再構築、苦情・訂正・Ready・選択資格を検査し、既存のEXEC certificate受理とcold reopenまで到達した。鍵・多項式はfixture入力。常駐kaspadの起動時配線、全dealerと両用途の自動配信、保存shareからのPANEL certificate成立、公開profileの独立した暗号レビュー・相互運用は残件。

容量管理では初回の結合ビルドを空き下限で停止した。停止済み旧fixture実行ファイル1件だけを透過圧縮し、SHA-256 `3bdfe160b9a790347501e70eadf76233ede572a759fb2d6f445d43bb74b9f71f`、論理424,137,888 B、モード・所有者・実行可否を照合して同じパスへ原子的に置換した。物理使用量424,140,800 → 148,172,800 B。これは開発用バイナリでの回収量。S3台帳やPruned Nodeの容量削減実績には算入しない。最終remote・consensus試験の作業ボリューム空き最小標本は13,106,683,904 B。

### 第3回　EXEC・PANELのdealer秘密と再配布

隔離bootstrapの正常系dealerは用途・round・slotごとにOSの乱数源から4つの係数を独立に生成する。521ビットの候補のうち法そのものを棄却する。f=1の二変数多項式を受信slotの座標で評価し、相手に渡すのはrow・column各2係数。dealer自身のholder入力は同じ保存多項式からローカルに導出する。

初期化は暗号化された312 Bのレコードを排他journal下で作成・同期・再読込する。既存の正規レコードがあれば同じ秘密を使う。通常の再開・配布はレコードを読むだけで、消失・鍵不一致・認証失敗なら配布できない。台帳とストレージ鍵を両方失った場合、元の秘密を復元できない。過去の整合した保存世代への巻戻しはローカルの認証だけでは検出できないため、消失後に同一roundで初期化をやり直すことは復旧手順に含まない。

上記3つのRust試験を同条件で再実行し、私信5件、custody 22件、native fixture 1件が成功した。正常系の5多項式（EXEC 3、PANEL 2）について各3配布・12方向のcross-check、journal再開、再初期化時の同一値、誤鍵・未初期化拒否、保存済み通信からのReadyを検査。PANELの6つ目の配布は苦情訂正を試すため既知の不整合入力を使った。native fixtureはEXEC certificateの通常受理とcold reopenまで13.8秒（起動・終了込み）。3試験中の作業ディスク空き最小観測値は13,127,262,208 B。第3回の残件は常駐holderによる通信・期限内の自動進行と、保存shareを使ったPANEL certificateまでの通常接続。

### 第3回　保存shareからEXEC・PANELの公開用batchを組み立てる

publisherの `HolderBatch::from_selected_packets` は用途とcloseを受け取り、自分のNodeの選択tipから準備済みround、資格、元のfreeze、ブロックの容量制限を取得する。holderごとの署名を確認し、slot順でpayloadとcoinbase commitmentを作る。同一slotの重複や定足数不足を拒否する。返却前にpruning importの世代、選択tip、finality、元のDAA期限を再確認する。モデル監査roundはこのEXEC/PANEL受付の対象外。

初回のfixture実行ではEXEC certificateを受理した直後の次ブロックがblue score 54に達しており、旧予定値38のPANEL closeを作れなかった。隔離genesisにcommitするfixture日程をPANEL close blue 64、release DAA 78、排他的期限120に更新し、登録の排他的期限を145とした。EXECの日程と合意の期限検査は従来どおり。検証では既存RFC-0012 rigの試験用認証frontier・成熟work入力を使う。実運用での認証・成熟の成立はこのfixtureでは検証できない。また、このbootstrapではPANEL対象Originalが0件であり、実案件への割当や決済の検証は別途必要。

前節と同じlocked/offlineビルド後の再実行で、私信5件・custody 22件・native fixture 1件が成功した。native fixtureでは両用途につき3 holderが保存済みshareから署名し、publisherは不足・同一slot重複・署名改変を拒否、逆順受信でも同じbatchを生成した。Nodeは各batchの実ブロック本文からcertificateを構成・受理した。RocksDB checkpointの再オープン後に両用途のcertificate hash・seed、準備済み資格と状態rootが一致した。native fixtureは起動・終了込み26.27秒、3試験中の作業ディスクの空き最小観測値13,120,724,992 B。第3回の残件は常駐プロセスの起動、round通知からの期限内自動通信・受付、fixtureの成熟入力に依存しない起動からの確認。

### 第3回　kaspadのbootstrap release holder

`--palw-tb1-holder-dir=<directory>` は隔離genesisに固定したbootstrap planと、そのholderの既存のEXEC/PANEL台帳を読み込む。設定ディレクトリに `plan.borsh`（Tb1BootstrapPlanのBorsh）、`signing.seed`（登録鍵を生成した32 B）、`storage.key`（保存shareの独立した32 B鍵）、既存の `custody/`、書込み可能な `outbox/` を置く。鍵ファイルは所有者のみがアクセスできる通常ファイル。台帳は各roundの初回登録とReady/share保存を済ませたものを使う。両journalの排他leaseはworkerの生存期間中保持する。送信箱は単一holder所有とし、publisherに渡すのはそこに出る署名済みpacketだけとする。

サービスは実行中Nodeのconsensus sessionを取得し、500 msごとに選択tipを調べる。移行中IBDでは発行を待つ。各回の親探索は最大64ブロックで、次回はcursorの選択枝所属を再検査する。予定のblue scoreを飛び越えた枝から代理closeを取らない。署名前のbranch観測、資格・finality・期限検査と永続的な露出予約は既存custodyに従う。送信箱では一時ファイルに書き、file sync・rename・directory syncが成功してから配達済みとする。受信するpublisherは `ReleaseOutbox::read` で容量制限を適用し、`HolderBatch::from_selected_packets` で独立に検証する。再起動後の再配布は元のround・slotの同じファイル名を使う。署名乱数は配布試行ごとに新しく、署名のバイト一致を再送判定に使わない。

これはrelease段階の常駐処理である。VSS配信とReady自動投稿、別ホストのpublisherまでの転送、実運用の認証frontier・成熟を使った試験は別の終了条件。ポーリング間で発生して戻ったreorgの観測も保証しない。停止マーカーの永続化に失敗した後のプロセス再起動は既述の隔離・復旧制約に従う。

今回のnative fixtureは各署名者にclose hashを渡さず、既存の2つのjournalを開き直してrelease dutyを起動した。PANELで実際に複数tickの親探索が発生し、初回試験は「1 tickで配達済み」という試験側の仮定で失敗。再試験は最大8 tickまで配達を観測し、配達完了を必須とした。各用途で送信失敗後の再試行と成功後の重複抑止、署名者のslot・close一致、送信箱の実ファイル再読込・過小読込上限の拒否を確認。publisherは読込結果から両certificateを成立させ、checkpoint再開後の照合も成功した。試験本体22.53秒。前節と同じ私信5件、custody 22件も成功し、この3実行中のディスク空きの最小観測値は13,124,898,816 B。

`cargo +1.93.0 test --locked --offline -p kaspad --lib palw_tb1_holder:: -- --test-threads=1` は3件成功。CLIで指定したディレクトリからの正規2台帳の再開、二重起動拒否、元のplanとの不一致、秘密鍵の権限・長さ・symlink、通常T12での拒否を確認した。`cargo +1.93.0 build --locked --offline -p kaspad --bin kaspad` と実バイナリのhelpを確認。通常T12を新規RAM作業ディレクトリ・loopback設定で起動する拒否試験は、初回35秒の上限までに既存の起動時Base0検査が終わらず中断。スタック採取で検査を確認し、60秒上限の再実行では鍵やRocksDBを開く前に想定理由で終了した。正規の隔離genesisを持つ独立kaspadプロセスでの配達成功は未検証。既存のfeature限定probeはsaltに対応した特定genesisを検査しており、TB1用planとPANEL policyを固定する起動profileの接続が残る。

### 第3回　隔離ノードの起動とholderの初回登録

feature限定バイナリの `--private-probe-profile=tb1-v1` は、公開入力の `plan.borsh` と `panel.borsh` およびdrill saltからgenesisを導出する。T12の有効化スケジュールとcoinbaseの上限204 Bを引き継ぐ。初回のノード試験で、drillの既定ラベルに両commitmentを追記したgenesis（207 B）がUTXO初期化中に拒否された。drillラベルの領域をTB1の識別情報に使い、既存のsubsidy・script・salted UTXO commitmentを維持した。入力の検査時にも長さ制限を適用する。

両journalは初回登録時に同じ一時ディレクトリに作成し、同期後にまとめて配置する。初回登録は当該鍵・roundを初めて使う場合に限る。以前に署名した可能性があり台帳や停止マーカーを失ったholderは、空のディレクトリへの再登録で再開してはならない。ローカル媒体を失うと、初回登録コマンドには以前の使用を検出する手段がない。永続化に失敗した停止の効力は既述のプロセス内保証に限られる。

次の手順で公開入力のみのmanifest照合、holder登録、同一appdirからの起動と再起動を再現できる。public-dirには計画とPANEL方針、holder-dirには同じ計画・登録済みslotの秘密鍵seed・独立した保存鍵（各32 B、0600）・所有者用の空のoutboxを用意する。appdirは絶対パスで初回は空とする。

```sh
cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-config --bin misaka-rfc12-private-node
probe=( "$BIN" --testnet --netsuffix=12 --private-probe-profile=tb1-v1
        "--private-probe-salt=$SALT" "--private-probe-tb1-dir=$PUBLIC_DIR" )
"${probe[@]}" --print-private-probe-profile
"${probe[@]}" "--private-probe-enroll-tb1-holder=$HOLDER_DIR"
"${probe[@]}" "--appdir=$APPDIR" "--palw-tb1-holder-dir=$HOLDER_DIR" --nodnsseed --outpeers=0 --disable-upnp \
    --listen=127.0.0.1:53001 --rpclisten=127.0.0.1:53002 --nologfiles
```

saltを0x1fの32バイト反復にし、4 holder・3候補・2席の短時間fixtureを使った。別プロセスが同じmanifestを生成し、別saltで識別情報が変わった。holderの初回登録後、同じコマンドの再実行は既存台帳を理由に拒否され、両ファイルの内容が一致した。同一DBの新規起動50.2秒、再起動50.3秒でgRPCがlistenし、2本のjournalはいずれもプロセス外から排他取得できなかった。両プロセスが終了コード0で停止した後はleaseを取得できた。genesisは `e48d340100b0c5d66728faac1323a29325281493d1fc6339456449b4bd0ff6f141a884cc469817446a5cfee65beba3b3fa16caef8dc19e92ab7d6357a552ccae`。試験DBはRAMディスク上に作成し、保存S3は停止したまま保持した。4 holder間の自動VSS通信、Readyの実ブロック投稿、実際の認証・成熟条件からのreleaseとpublisher受付は第3回の残件。

別のholderと重なるprovider、交換終了と苦情締切が等しい日程は、いずれもmanifest生成時に拒否した。PANELの最大Original数だけを1から2に変更するとplan IDを保持したままgenesisとconsensusParamsIdが変わり、元のappdirでの起動はDBを開く前のfingerprint照合で拒否された。

`cargo +1.93.0 test --locked --offline -p kaspad --features private-integration-config --lib private_integration::tb1_bootstrap::tests:: -- --test-threads=1` は1件成功。同じバイナリで `palw_tb1_holder::` の3件も成功した。

### 第3回　初回登録と常駐holder間のVSS交換

隔離ノードの初回登録で、EXECとPANELの各journalと同じ一時ディレクトリに、そのround固有のdealer多項式を暗号化して用意する。両roundの同期が成功してからcustodyを配置する。既存の登録に対する再登録は拒否する。多項式を紛失したholderは、そのroundで新しい多項式を生成して継続できない。

4つのholderの各設定ディレクトリに同じ `vss-peers.json` を置くと常駐サービスが交換を行う。値はgenesisに固定したholderのslot順に並べた、異なるloopbackアドレスとポートのJSON配列（例 `["127.0.0.1:53100","127.0.0.1:53101","127.0.0.1:53102","127.0.0.1:53103"]`）。このファイルを省略した既存のrelease holderは以前の起動形式を維持する。

各holderは選択tipが交換終了のblue scoreに達する前、かつIBD移行中でないときに交換を開始する。接続待ちと認証はconsensus sessionを返却した後に行う。各接続では片方のdealer pair、またはあるdealerに関する2者間のcross-checkを1件運ぶ。接続を開始するのは小さいslotで、秘密の送信方向は元の署名対象に従う。受信側が認証済みの値を暗号化して同期した後に受領を通知する。再起動後の再配送では元の予約と照合する。全交換の完了はReadyや合意上の資格を成立させない。

2つの通常登録で両roundの暗号化多項式の再オープン、異なる保存鍵、二重登録拒否、紛失時の再生成拒否を確認した。別の4-holder fixtureではholderを個別に開き、実TCPで2 round × 4 dealerのペアを合計24件、cross-checkを96件配送した。全台帳を開き直し、各dealer・各holderについて保存済みの全cross-checkを復元し、honest fixtureの苦情が0件であることを照合した。次の2つのフィルタを別々に実行し、1件と4件が成功した。

```sh
cargo +1.93.0 test --locked --offline -p kaspad --features private-integration-config --lib private_integration::tb1_bootstrap::tests:: -- --test-threads=1
cargo +1.93.0 test --locked --offline -p kaspad --features private-integration-config --lib palw_tb1_holder:: -- --test-threads=1
```

4つの独立した隔離ノードはすべて同じ公開plan・PANEL manifestを検証し、各holderは独立の鍵・custody・DBを使用した。最初のホストメモリ割当6 GiBは、1台1.5 GiBとなり既存の2 GiB下限で起動を拒否された。4台合計10 GiBを割り当てて順に起動し、最後のRPC起動後4.65秒で全員が60件ずつ完了した。slot 0の正常停止・再起動後に4.91秒で再配送を確認。全240保存ファイルの内容は前後で一致し、全プロセスは終了コード0で停止した。この試験は採掘を行わないgenesisでの私信交換で、Readyの実ブロック投稿とその先のreleaseは対象外。

遅く起動するholderがいる間、未接続と未受信のエラーが交互に発生し、slot 2のログが起動時出力を含めて1,314行になった。交換失敗の警告を1 holderあたり30秒に1件へ制限し、失敗後と配送完了後のポーリング間隔を100 msにした。変更後の実バイナリで同じ4台のDB・台帳をcold reopenし、最後のRPC起動後6.98秒で再配送が完了。全240ファイルのSHA-256が元の値と一致した。起動待ち中のslot 0の交換警告は2件、ほかの3台は0件。実ディスクの観測最小空きは12,987,084,800 B。これは隔離fixtureの実行時の値で、第8回のPruned容量判定は未実施。[4プロセスの検証結果](../evidence/palw/tb1-four-holder-vss-2026-11-25.json)

### 第3回　holderの署名済みReadyと手数料入力

隔離bootstrapの常駐holderは、交換終了とconsistent終了の選択チェーン上のブロックを読み、Ready期限内に各dealerの受領済みペア・cross-checkと公開transcriptを照合する。holderは対応するshareを同期してからReadyを署名する。同じround・slot・dealerについて再起動後にも最初の署名済みpacketを再使用する。別のendを示す既存packetや改変署名の上書きは拒否する。

手数料を負担するholderはenrollmentディレクトリに `ready-fees.json` と所有者だけが読める32バイトの `ready-fee.seed` を置く。JSONの `outpoints` はEXEC・PANEL各4列の配列で、各列はdealer順の `"txid:index"` または `null`。指定した各入力は重複不可で、釣銭は同じ支払鍵に戻す。`maximum_fee_sompi` は1件あたりの実手数料上限。kaspadは選択tip・pruning import世代を再確認し、指定UTXOを使ったcarrierを通常mempoolの検証・配信に渡す。手数料設定を省くと署名済みpacketまでを保存する。

`private_tb1_preparation_reads_ready_from_accepted_blocks` は23.52秒で成功した。正直なdealerのEXEC・PANELで、1 dealerの資料欠落、outbox書込先の拒否、再試行、再起動後の署名bytes一致、改変拒否、選択ブロックの資格・certificateを照合した。苦情訂正用のPANEL dealer 1名は既存の不整合入力を使用。ブロック組立は既存の短時間fixtureの手数料入力と時計を使う。別試験で所有鍵・専用入力・実手数料上限・釣銭を確認した。4常駐プロセスでの手数料付きReadyのmempool受付から実ブロック選出・成熟・releaseまでを通す試験は未完了。

4-holder TCP試験で接続中の `setsockopt` がmacOSのEINVALを返し、通信が中断する事象を再現した。socketをnonblockingにして絶対締切まで `poll` で読み書きを待つように修正。修正後はholder 5件が6.33秒、私信5件が0.50秒で成功した。隔離設定1件とcustody 22件も成功。再現用の試験フィルタは前節の2本に加え、`misaka-palw-tb1-channel --lib` の全件、`misaka-palw-remote --lib tb1_exposure::`、`kaspa-consensus --features evm --lib private_tb1_preparation_reads_ready_from_accepted_blocks`。全件をlocked/offline、1 test thread、64 MiB stackで実行した。

### 第3回・TB1の4台での有料Ready検証（11月25日）

既存の `tb1-v1` にはreleaseが参照するnative settlement policyがない。隔離用の `tb1-native-v1` は同じ手数料用UTXO、plan・panelのcommitmentを維持し、別のgenesisと合意fingerprintでRule E、ヘッダーcommitmentとnative settlementをDAA 1に設定する。genesisに紐づけて4 holderを新規enrollする。両profileの識別・登録試験1件と履歴時計の試験9件が成功した。

再現は空の作業先で次の順に実行する。`TARGET` はbuild先、`FRESH` はまだ存在しないディレクトリ。Metalを使ったmacOSでの試験で、4台の通常ノードがPoWを検証する。

```sh
CARGO_TARGET_DIR="$TARGET" cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-clock --bin misaka-rfc12-private-node --example tb1-ready-fixture
python3 scripts/drills/tb1_ready_peers.py --node "$TARGET/debug/misaka-rfc12-private-node" --generator "$TARGET/debug/examples/tb1-ready-fixture" --work "$FRESH" --profile tb1-native-v1 --metal
```

4台でVSS交換を各60件完了し、EXEC・PANELの署名済みReadyが各holderのoutboxに8件、計32件揃った。各holderの専用入力で1件ずつ、計4件をそれぞれ通常mempoolで確認した。各手数料は271,525 sompi。静止中の採掘ノードでは30秒待っても4件が揃わない場合があり、試験は各送信元での受付を確認してから選択ブロックを進める。同一DAAでH2が告知を抑制した場合は、保持済みの直後の子ブロックだけを通常RPCで再提示する。

成功した実行では4件すべてがblue score 7の同一ブロックに入り、署名済みbytesと一致した。次のブロックで4台すべてのRPCが同じ4件の受理を返し、4プロセスは終了コード0で停止した。資格を満たす数のReady、準備commitment、両roundのclose、native finality到達後のrelease、そこでの再起動・reorgは第3回の残件。[4件のtxid・手数料・収録ブロック・受理ブロック](../evidence/palw/tb1-funded-ready-peers-2026-11-25.json)

### 第3回・両roundの資格に必要なReadyと準備ブロック

4台の署名者が各自のfee floatを通常のnative送金で分割した。EXECとPANELそれぞれで、dealer 0・1・2について異なる3 holderが署名したReadyを1件ずつ通常mempoolに提出した。4台で受理を確認した18件はblue score 7に5件、8に11件、9に2件。各dealerの3署名にあるtranscript rootと終了ブロックが一致し、手数料の総額は4,887,450 sompiだった。4プロセスの終了コードは0、所要102.23秒、ホストの観測最小空きは12,976,336,896 B。[取引別の収録先と受理ブロック、6組の署名者とtranscript](../evidence/palw/tb1-quorum-ready-peers-2026-11-25.json)

準備の手数料を負担するholderは、`ready-fees.json` の任意の `preparation` に `{"outpoint":"txid:index","dealer_masks":[7,7]}` を指定する。順にEXEC、PANELのdealer集合を表し、bit 0はdealer 0。準備専用UTXOは各Readyの入力から独立させる。holderはReady期間が閉じた選択親で両方の資格を計算し、入力の所有権、手数料上限、釣銭を検査して準備取引を提出する。隔離profileのheartbeat minerは、選択した取引に含まれるgenesis固定のplanと親の履歴から同じcommitmentを計算し、空のminer extra dataへ格納する。再現は前節の4台コマンドのprofileを `--profile tb1-short-v1` に変更し、`--quorum` を追加する。

`cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_preparation_reads_ready_from_accepted_blocks -- --test-threads=1` は64 MiB stackで1件成功（23.73秒）。独立に計算した資格と自動生成したcoinbase、および受付後の両roundの資格rootを照合した。異なるdealer集合を指定した場合は、明示的にmarkerを渡しても準備が成立せず、空のminer dataからもmarkerを生成しなかった。次のEXEC closeで使う `ExecCloseCarrier::encode` の呼出元は現時点では合意fixture内にあり、署名済み登録からの常駐経路が残る。

既存の `tb1-native-v1` は登録開始がDAA 18400で、短いfixtureでは準備取引を4台が受理しても、復元した合意状態に資格が残らなかった。この結果を受け、holderの支出とminerの準備マーカー生成にも登録開始DAAを適用した。`tb1-short-v1` は別genesisに開始DAA 1を固定する短時間試験専用設定。元の2設定と区別し、起動時の既存のV1・V2・private Rule Eの検証を通す。

この設定で通常PoWの4ノードがReady 18件を受理し、準備取引（手数料327,137 sompi）をblue score 14に収録、15で受理した。全ノードの状態45,016 BのSHA-256・root・親が一致した。Rustの状態復元でEXEC・PANELの資格root、両roundのdealer集合（各0・1・2）、coinbaseと同じ準備マーカーを確認した。所要108.72秒、ホスト空き最小13,010,636,800 B、終了コードは全台0。EXEC/PANELのclose、成熟後のrelease、その状態での再起動とreorgはこの4台試験の対象外。[準備取引と復元した両資格の証跡](../evidence/palw/tb1-native-preparation-peers-2026-11-25.json)

### 第3回・署名済み登録からEXEC closeを生成（11月25日）

`ExecInputBatch::close_carrier` が、選択枝で検証した登録のpreimage、所有者署名、資金参照、任意の資金提供者同意をslot順に書き出し、同じ集合から導いたconsumer rootと両roundの資格に結び付ける。隔離heartbeatのminerは選択したEXEC取引と親の保存資格を照合する。closeの確定予定scoreで合意側と同じ署名・資金・期限の検査を通過したときに限り、自動で選択親に対応するfreezeをcoinbaseに入れる。受付時は実際のcoinbaseの値を検証する。

`RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_preparation_reads_ready_from_accepted_blocks -- --test-threads=1` は1件成功（23.94秒）。異なるpolicy、容量上限1バイトを生成時に拒否し、所有者署名1バイトを変更した取引にはminerがfreezeを付与しなかった。正常な取引を入れたブロックでは選択親から独立計算したfreeze・保存receiptのpreparationが一致し、deltaを戻すとclose前のstate rootに一致した。単一の合意fixtureによる結果。4実プロセスの検証端点は引き続き準備取引までで、EXEC取引の手数料支出・RPC提出からPANEL close・成熟後releaseまでの実証が残る。

### 第3回・4ノードのEXEC closeと再起動（11月25日）

`tb1-short-v1 --quorum` の4台試験を延長した。登録所有者が準備の両資格に結び付けたTIR入力1件に署名し、準備取引の実際の釣銭を別のUTXO署名で支出した。最初の試験は1,000,000 sompiの手数料付き取引がblue score 17に収録されても、coinbaseのfreezeが一致せず不合格。close検証が、準備親とclose親の内側の状態rootを直接ヘッダーと比較していた。Rule Eのヘッダーはfork-choice leafを含むcommitmentを記録する。両状態をそれぞれのDAAの規則で開き直し、同じ検証をminerのテンプレートと合意受付に適用した。

修正後は登録に結び付くfreezeが17で一致し、18で4台が同じ取引を受理。全台のclose後の状態45,346 Bが一致し、復元したreceiptが準備ブロックとfreezeを指した。4プロセスを終了して同じDB・custodyを再オープンし、採掘停止のまま全台でtipと状態を再照合した。再起動前後の全8終了コードは0、release期間前のoutboxのreleaseは0件。所要165.86秒、ホスト空き最小12,980,371,456 B。保存carriageに別ブロックを指定した場合と、内側のrootに代えてヘッダーcommitmentを指定した場合はいずれも復元を拒否した。[取引・準備とcloseのroot・受理・再起動の証跡](../evidence/palw/tb1-native-exec-close-peers-2026-11-25.json)

再現は4台用コマンドに `--profile tb1-short-v1 --quorum` を指定する。公開前の残件はnative finalizedが実際にcloseを覆う枝でのshare配信、EXEC certificate、PANEL closeとcertificate、それらのreorg・復旧。今回の署名済み入力はquoteのないTIR登録で、モデル実行・受賞・決済はこの試験の検証範囲に含まれない。

同じgenesisの生成器から読み出した設定値は `pruningDepth=74920`、`finalityDepth=600`、`registrationFromDaa=1`。EXECのrelease期間はDAA 25以上48未満。native finalizedは認証済みsafeの下のpruning pointを使うため、今回のblue score 18までの履歴ではcloseを覆えない。成熟試験には、対象closeを含む選択履歴、pruning pointの進行、safeを支える成熟済みの実workの3点が必要となる。上記の短時間合意fixtureも修正後に再実行し、フラットな状態rootを使う経路、署名改変拒否、delta巻戻しを含む1件が23.90秒で成功した。

### 第3回・PANEL close、両certificateの生成と4台の成熟状態（11月25日）

heartbeat minerが、署名済みrelease batchの本文からcoinbaseのcommitmentを生成する。PANEL closeではgenesisに固定したpolicy、受理済みEXEC certificate、準備資格、選択親の全Originalと資料割当を照合してfreezeを作る。EXEC/PANELのcertificateでは、batchの直前と候補の選択親の両方でnative settlementを取得し、署名・quorum・期限・再構成を確認した結果をcoinbaseに入れる。検証ノードは実際のcoinbaseを同じ規則と照合する。batchの収録とcertificateの受理は別々に判定する。

`RUST_MIN_STACK=67108864 cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_preparation_reads_ready_from_accepted_blocks -- --test-threads=1` が24.02秒で成功。両roundとも末尾を切ったbatch、存在しないbatchのcertificate参照では自動markerが付かず、genesisから変更したpolicyではPANEL closeのfreezeが付かなかった。正常系では両certificateと異なる2つのseedが保存され、checkpoint再オープン後にも一致した。この合意fixtureは成熟済みfrontier・workをセットし、PANELのOriginalは0件である。

同じ変更の4実プロセスで実際の成熟状態を測った。`tb1-short-v1 --quorum` はEXEC closeを受理し、blue score 18・DAA 8時点で全台の `getPalwSettlement` のsnapshotとreadinessが一致した。停止理由は `frontierNotCovered`、成熟workは0（必要1）、深さ0（必要1）、safe・finalizedは未成立、pruning pointはgenesisだった。成熟待ちのworkも0件なので、既存入力に対する待機だけでは進まない。再起動後の全4台でも同一結果でreleaseパケットは0件。所要163.91秒、ホスト空き最小12,967,501,824 B。実際のshare配送・集約・手数料支出からcertificateに至る4台経路、および成熟後のreorgは未検証。[選択親・状態root・成熟条件・再起動の実測](../evidence/palw/tb1-exec-close-maturity-2026-11-25.json)

### 第3回・独立したholderの公開資料から手数料付きbatchを作成

隔離bootstrapの常駐publisherは、`ready-fees.json` の任意フィールド `publication.outboxes` に指定した4ディレクトリをrosterのslot順に読む。相対パスは設定ファイルのディレクトリ基準。`publication.batch_outpoints` はEXEC・PANEL順で、それぞれ専用入力またはnullを指定する。publisherは各公開packetの署名・slot・freezeを自分の選択枝で検証する。4者中3者に達した場合に、現行tipのfinalityと期限を再確認して取引を作り、通常のmempool受付に渡す。手数料入力はReady・準備・他のbatchとの重複を拒否し、既存の支払者・釣銭・手数料上限検査を通る。

次の短時間試験を順番に実施した。

- `cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_preparation_reads_ready_from_accepted_blocks -- --test-threads=1` が1件成功（24.31秒）。EXEC・PANEL双方で、3者の署名があれば4番目が欠落しても、別slotの署名を置いても、厳密に検証したbatchとpayloadが一致する。有効な署名が2者に減るとbatchを生成しない。両certificateの受理とDB再開を含む。このfixtureは成熟状態を設定する。
- `cargo +1.93.0 test --locked --offline -p kaspad --features private-integration-clock --lib palw_tb1_holder::ready_fees::tests:: -- --test-threads=1` が1件成功（0.07秒）。batchとReady・準備・他round間の重複、専用入力なし、支払者・上限を検査した。
- `tb1-ready-fixture <新規ディレクトリ> tb1-short-v1 --quorum` の生成物で、実際のsplit取引の出力21件と、Ready 18件・準備1件・batch 2件の設定上の入力が一対一に対応することを照合した。4つの公開ディレクトリ参照も各holderの出力先と一致した。

実4プロセスでの確認地点は前節のEXEC close・cold reopenまで。通常の仕事から成熟条件を満たした4台でのbatch受理、常駐publisherのcertificate投稿、PANELへの進行とreorg復帰は第3回の残件。

### 第3回・常駐publisherからのEXEC・PANEL certificate投稿

`publication.certificate_outpoints` にEXEC・PANEL順で任意の手数料入力を指定する。各certificateには同じroundのbatch入力が必要。同一UTXOの重複指定を設定読込時に拒否する。常駐publisherはローカルの選択枝を1回64ブロック以内で遡り、再起動時もそのround専用の入力を使ったbatchを特定する。sinkではbatchの釣銭、後続ブロックがある場合は選択された子のacceptance dataを確認する。支払者が署名したcertificate取引を同じ親の通常のheartbeatテンプレートに通し、取引の収録とcoinbase commitmentを確認した上で、既存のmempool投稿経路に渡す。採掘側は元のbatch直前と候補親の両時点で選択枝の成熟を検証する。

前節と同じRust試験2件を再実行した。native fixtureは24.21秒、手数料設定試験は0.07秒で成功。EXEC・PANEL双方で、未登録batchの参照を拒否し、正しい参照からcertificateを受理した。holderのleaseを閉じて再開後、同じ専用入力から選択枝の受理記録を使って元の参照を復元できた。native fixtureの成熟は設定値による。

`scripts/drills/tb1_ready_peers.py --node <private-node> --generator <tb1-ready-fixture> --work <新規ディレクトリ> --metal --quorum --profile tb1-short-v1` が163.09秒で成功した。実4ノードがReady 18件・準備・EXEC closeを受理した。blue 18までの選択枝18ブロックと停止前・再開後の全4台のmempoolで、batch・certificate用の4入力の消費は0件。4台の選択tip・状態・成熟判定が再起動後も一致した。ホスト実空き最小値は12,965,171,200 B。[4台の検証記録](../evidence/palw/tb1-certificate-fees-2026-11-25.json)

この実プロセス試験の終点はrelease窓の手前で、成熟仕事は0、finalized headは未成立。通常の仕事で成熟・pruning境界がcloseに到達した後の手数料付きbatchとcertificateの実受理、PANEL closeの常駐投稿、reorg復帰が次の確認対象。

### 第3回・署名済みの資料割当を使うPANEL closeの常駐投稿

隔離publisherに `ready-fees.json` の `panel_close` を設定する。`outpoint` は他用途と重複しない手数料入力、`assignments` はproviderが公開した割当のディレクトリ。holderの `panel.borsh` のIDを起動時にgenesisと照合する。providerはEXEC closeごとに受付順の資料IDと署名を `<EXEC close hash>.tb1panel` として保存する。publisherは選択枝からEXEC closeを探索し、予定位置のPANEL候補について、ローカルの通常テンプレートでEXEC certificate、全Original、資料署名と期限を照合できた取引を投稿する。空の割当で投稿できるのは、候補親の認証済み受付索引が空の場合だけ。

- `private_tb1_preparation_reads_ready_from_accepted_blocks` が24.22秒で成功。再開したholderが過去のEXEC closeを見つけ、空の受付からPANEL closeを構成した。両certificate受理と状態のcold reopenも通過した。
- `private_registered_tb1_pending_freeze` を本書第3回の登録tokenizer・native certificate・native replay設定で再実行し、12.22秒で成功。Originalが1件ある場合は、割当の欠落と署名に対応しない資料IDを投稿前に拒否した。正しい割当で採掘テンプレートがcommitmentを生成し、通常受理後のPANEL closeの記録とdelta巻戻しを照合した。二つの合意fixtureは成熟状態を設定している。手数料設定試験は0.07秒で成功した。
- 前節の実4ノードコマンドでPANEL close入力を含む5入力を検査し、164.28秒で成功した。選択枝18ブロックと各ノードの停止前・再起動後のmempoolで公開用入力の消費は0件。実空き最小は12,993,249,280 B。[4台の状態と専用入力](../evidence/palw/tb1-panel-close-publisher-2026-11-25.json)

実4ノードの確認範囲はEXEC closeと再起動まで。実workで成熟した選択枝でのEXEC・PANEL両方の手数料支出、PANEL closeの常駐投稿から受理まで、成熟後のreorg復帰は未検証。providerが実際に保管する資料と割当の対応はQF1資料供給経路でも検証する。

### 第3回・同じEXEC closeでrelease窓の開始と失効を実測

保存済みの4ピアのDBを別ディレクトリに復元し、元のholderの鍵・custody・outboxをそのまま使った。前回の選択tipと状態を全員で照合してから、blue 18・DAA 8から通常のPoW heartbeatを79ブロック受理した。窓開始のDAA 25（blue 51）と失効のDAA 48（blue 97）で、4台のsnapshotとreadinessが一致し、全台で `frontierNotCovered`、finalizedなし。全79ブロックで公開用の5入力の消費は0、開始前・窓開始・失効時の各4台のmempoolにも該当支出なし。8本の曝露journalの内容は一致し、release packetは0件。4台とも正常終了。元DBの92ファイルをSHA-256で再照合した。113.48秒、ホスト空き最小12,989,865,984 B。[実測値・状態commitment・照合対象ネットワーク](../evidence/palw/tb1-exec-window-expiry-2026-11-25.json)

EXEC closeの保存状態を元のRule E commitmentに照合した。8件の登録済みbondのうち、Gate Cの検証者年齢条件を最初に満たせるのはDAA 5040。抽選75・承認閾値41の設定で、期限48より前に年齢条件を満たすbondは0件。このroundでは、新規仕事を投入した場合も通常のGate C承認を期限内に得られない。年齢を満たした時点での実際の資格は稼働状態・担保・operatorの条件にも依存する。pruning pointも全測定点でgenesisだった。保存S3のDAA 8602はTFO3専用の別genesis・別ruleset。成熟成功側のTB1試験には、対象ネットワーク上で年齢条件を満たすverifier、認証された仕事、closeを覆うpruning point、残存release期間を併せ持つ履歴が必要。今回の実績は未成熟時の失効までで、成熟した両certificateの実4ノード受理は残件。

再現は前節の終了済みfixtureを `SAVED`、別の新規ディレクトリを `FRESH` として実行する。

```sh
python3 scripts/drills/tb1_ready_peers.py --node "$NODE" --generator "$GENERATOR" --profile tb1-short-v1 --metal --quorum --resume-exec-window "$SAVED" --work "$FRESH"
```

RocksDB複製時、RAMボリュームがextent照会にENOTTYを返すため、snapshotのコピー処理を修正した。extentを取得できない場合は残り全バイト分を予算に計上してコピーする。コピー関連の回帰4件が成功した。

### 第3回・保存EXEC closeからの通常producer起動

同じ4台の閉鎖DBの複製で、既存の5番目のseatを通常のbonded producerとして起動した。保存commitmentと登録公開鍵を照合し、起動後のRPCでも鍵、bondと手数料UTXOを確認した。隔離ネットワークのproducerにだけUTXO indexと未同期採掘オプションを指定した。4台の共通tipを12回観測し、最終観測は41 draws、生成0、対象bondの共通claim 0。全台正常終了、81.03秒、ホストの最小空き12,990,128,128 B。通常の仕事抽選に入れることを確認した。仕事の受理、Gate C、成熟したEXEC/PANEL releaseは未達。この観測コマンドの正常終了はclaim成立を意味しない。[4ピアの観測値](../evidence/palw/tb1-producer-observation-2026-11-25.json)

再現は前節のコマンドで `--resume-exec-window` を `--resume-producer` に置き換え、新しい作業ディレクトリを使う。

### 第3回・検証者の年齢条件を満たせるEXECとPANELの原期限

従来の隔離roundはgenesisに固定される。既存台帳の期限を上書きせず、新規の4ピアfixtureを `--maturity-schedule` で起動した。EXECはclose blue 17、release DAA [25, 200000)、PANELはclose blue 240000、release DAA [300000, 600000)。これらは隔離試験用の選択値。仕事登録者が署名する元期限もこのPANEL期限に合わせて確定する。pruning depth 74920、finality depth 600、検証者年齢5040、抽選75・承認閾値41を維持した。4台で18件の資金付きReady、preparation、署名済みEXEC closeを受理し、blue 18・DAA 8で停止。closeの状態を開いて確認した8件のbondは、期限までに年齢条件に到達できる。4台のcold reopenで状態が一致。実行時間161.11秒。年齢到達、独立承認、finalized、成熟後のshare配送はこの試験の観測範囲外。[4台のclose・期限・再起動](../evidence/palw/tb1-maturity-schedule-2026-11-25.json)

```sh
python3 scripts/drills/tb1_ready_peers.py --node "$NODE" --generator "$GENERATOR" --profile tb1-short-v1 --metal --quorum --maturity-schedule --work "$FRESH"
```

### 第3回・実際の保存先を測定して通常attemptを受理

macOSでマウント一覧に現れない10 GiBの試験ボリュームを使うと、従来の最長prefix照合が親の494,384,795,648 Bを選び、保持資料の書き込みを誤って拒否した。Unixでは書き込み先の既存祖先に `statvfs` を発行し、ストレージ予算とretentionの双方が同じ空き・総容量を参照する。新しい仕事を引き受ける前に空き下限を確認し、書き込み直前にも実際のcaptureサイズで判定する。既存のreceipt・redemptionの処理は引受判定より先。8 GiBボリュームの空き不足側では、4ピア接続後もproducerがholdingを返し、抽選0回・生成0件を確認した。容量予算の単体試験12件が成功した。

期限を延ばした隔離genesisの閉鎖DBを復元し、拒否時の選択枝に含まれる実heartbeat 4件を通常の `submitBlock` で再検証した。producerが自ら仕事を計算してalgo 6のブロックを生成。blue 23・DAA 11で4ピアのclaim、bond、出力root、carrying blockが一致した。producerに1,599,245 Bのmaterialが残り、全台正常終了。68.68秒。元の閉鎖DB92ファイルのSHA-256が一致した。claimはprovisional、Gate C pending、受賞決済・両certificateは未達。[受理・保持量・容量拒否の対照測定](../evidence/palw/tb1-retained-attempt-2026-11-25.json)

同じ閉鎖DBを `SAVED` に指定する。次のprefixはこのDBの選択tipを起点とする。新規にenrollしたfixtureの場合は `--producer-prefix` を省略し、新規claimの成立を同じ条件で判定する。時間内に成立しなければ結果を保存して失敗終了する。[再投入した4ブロック](../evidence/palw/tb1-retention-replay-prefix-2026-11-25.json)

```sh
python3 scripts/drills/tb1_ready_peers.py --node "$NODE" --generator "$GENERATOR" --profile tb1-short-v1 --metal --quorum --resume-producer "$SAVED" --require-producer-claim --producer-prefix docs/evidence/palw/tb1-retention-replay-prefix-2026-11-25.json --work "$FRESH"
cargo test --locked --offline -p kaspa-utils --lib storage_budget::tests::
```

実claim受理後の閉鎖DB4台分とcapture、公開plan、時計、選択prefixを4,173,263 Bに圧縮した。120ファイルを原本と照合。署名seedとholderのcustodyはアーカイブの対象外。別ディレクトリにproducerのDBを復元し、採掘を停止した状態でcold起動した。元のtip、claim全行、algo 6のcarrier、出力root、captureの長さ・SHA-256が一致し、正常終了した。

第3回の追加実測。上の4ピアfixtureに `--independent-panels` を加え、同一ホストの3検証ノードで別々の登録bond、operator、未使用fee UTXO、panel submitterの起動を確認した。4台がDAA 11で同一の実claimを受理し、4台とも正常終了した。閉鎖DBの複製をcold起動し、DAA 43で同じclaimがprovisional、認証pending、割当0、G14 seed未設定であることを確認した。最早anchorはDAA 31、bind backstopは611。割当、検証者の署名、決済は引き続き未検証。[実claimと再起動時のRPC記録](../evidence/palw/tb1-independent-panel-claim-2026-11-25.json)

第3回のanchor調査。上の `--independent-panels` fixtureで、最早anchorのDAA 31から4つの選択親について各10秒間heartbeatを保留した。通常producerの抽選回数はそれぞれ23、23、23、22回増え、観測中に選択tipは進まなかった。4台の終了後、1台の閉鎖DBを複製してcold起動し、DAA 43のtipから実claimの受理ブロックまで64ブロックとそのmergesetをRPCで逆走査した。slot以後の25ブロックはすべてheartbeat（algo 8）。候補となるattempt（algo 6・9）は0件で、この履歴にはR-core+のpanel割当を起動するanchorが存在しない。未割当のままDAAだけを進めても、この条件を満たした検証にはならない。Gate Cの認証・報酬・成熟後のshare配送は未達。[抽選回数・cold起動後の履歴と割当](../evidence/palw/tb1-anchor-history-2026-11-25.json)

第3回の実claim再実行。前項のDAA 43の閉鎖DBを4台分複製し、同じbondでproducer・panelを起動した。150秒の観測中に338回抽選し、新規受賞0、共通tipはDAA 43、claimはprovisional・認証pending。4台とも正常終了した。受理ブロックのRPC写しからヘッダhashを再計算し、元のclaim ID、署名、challenge、genesis domainを検査した。ヘッダのnonceとpre-PoW hashから仕事を独立に復元してBASE0を実行し、署名済みのtrace・output・execution・manifestの4根とchunk数がすべて一致、再生成した資料は保存済み1,599,245 Bと一致した。資料の末尾1 bit変更、ヘッダ時刻変更、別claim IDの3ケースを拒否。Gate Cの検証者承認、決済、TB1のshare配送は未達。[4台の結果・再実行・拒否理由・保存入力のSHA-256](../evidence/palw/tb1-native-attempt-replay-2026-11-25.json)

再現手順。四台を再開する場合、SOURCEは直前の `--independent-panels` 試験の閉鎖ディレクトリ、CLOSEは同じgenesisのEXEC close、FRESHは新規作業先。単独再実行の2入力は `~/.local/share/misaka-tb1-fixtures/replay-8abe70c93edb5ebe.tar.gz` に保存した。BLOCK_ID・CLAIM_IDは上の実測JSONのreplay.blockとreplay.claimを指定する。

```sh
python3 scripts/drills/tb1_resume_claim.py --node "$NODE" --generator "$GENERATOR" --close "$CLOSE" --source "$SOURCE" --work "$FRESH"
"$GENERATOR" --replay-attempt "$CLOSE/config" "$INPUT/accepted-block.json" "$BLOCK_ID" "$CLAIM_ID" "$INPUT/accepted.material"
```

第3回の資料保持。DAA 43で閉鎖した4台の `palw-retention/` と `foreign/` を実claim IDで照合した。producerだけにcapture 1,599,245 Bとanswer 112 Bが残り、検証3台の該当ファイルは0件だった。未割当時のディスク検査からは、メモリ上のgossip到着有無は判定できない。通常のpanelは到着資料を有界のメモリプールで受け、dutyで検査した資料を保持する。[ピア別ファイル](../evidence/palw/tb1-peer-retained-files-2026-11-25.json)

他ノードの検証済みcapture・answerの初回保存は、共有容量予算が設定されているとき `PalwRetained` の予約を取得し、stagingから公開まで保持する。既存の通常ファイルは容量予約なしで再利用する。変更した実装を既存依存ライブラリとRAM上で単独コンパイルし、予約拒否時のstaging未作成・既存コピー保持と、8書き手が競合しても完全な1コピーのみ残ることの2試験が成功した。kaspad全体の再ビルドと実ノードでの新規foreign保存は未検証。

追加の4ノード試験はホスト空きが12 GiB未満になり中止した。試験後の空きは約11.2 GiBで、4プロセスの終了を確認した。再開fixtureは複製前にも下限を検査し、作業先を作らずに拒否することを確認した。次回は `--producer-peer` で同じ4台のうち別の登録bondを1件選択できる。成熟したEXEC/PANELと報酬決済は引き続き未証明。

第3回の検証者による回答照合。通常panelのattempt出力ID読取りは、メモリ上の候補を借用し、ディスクでは自ノード・他ノード由来の短いanswerをcaptureより先に検査する。anchor、prompt、出力rootの照合に失敗したときは次の候補を試す。DAA 11で受理した実claimの112 BのATA1を使い、ヘッダ由来のanchor・promptと署名済みoutput rootの一致を確認した。anchor、prompt ID、出力ID、magicをそれぞれ変更した4ケースは拒否。1,599,245 Bのcaptureはそのまま独立再実行と資料保持に使える。実ノードのディスクから削減されたバイト数やP2P受信はこの検査では測定していない。[実入力・署名済みroot・試験結果](../evidence/palw/tb1-retained-answer-2026-11-25.json)

Rust 1.93.0と既存の依存ビルドを使い、RAM上で現作業ツリーのkaspadライブラリ、通常のkaspad実行ファイル、ライブラリ全体のテストバイナリを再コンパイル・リンクした。private-integration-clockを含む構成でkaspadのhelp正常終了。保持・回答優先順位・不一致の反証25件と、TB1 holderの再起動・VSS等5件が成功した。4ノード実走は再開していない。

実claimの照合を再現するには、直前の再実行用アーカイブを展開し、今回の実測JSONの `answerHex` をバイナリに戻したファイルを `--replay-attempt` のCAPTUREの次に渡す。

### 第4回 FOW1の仕事識別とReadout裁定

第3回の成熟・share配送を未達として記録し、ホストの12 GiB下限で4ノード試験を止めた状態から、第4回の独立して実行できる接続作業に着手した。従来はnativeの受理側だけが `TB1QFW01` を解釈し、候補側では試験コードが手順を複製していた。private remoteの `read_panel_approved_qf1_publication` で、選択親、受理済みOriginal、目標Ticket、Panel承認、登録class・tokenizer・Readout descriptor、通常inventory openingを既存の検査に通し、そのEXEC登録のjob tokenから開示payloadを返す。候補の送信・Final・決済はこのAPIの範囲外。

保存したQwen3の3ピアログ2件を `check_real_lifecycle.py` で再照合した。各記録内で試行、署名対象のOriginal、4根、Ticket、Panel、End Courtを追跡し、保存済みJSONとの全項目一致を確認した。各試行は入力111 token、出力9 token、trace 2,086,566葉。正しいReadoutのOriginalでは承認が収録され、Endの異議は棄却された。改変したReadoutのOriginalもTicket目標を満たしたが、承認は収録されずEndで計算が反証された。これらは過去の実行記録の再検査であり、変更後の候補が同一claimでFinal・Gate C・権利消費まで進んだ証跡はまだない。G-COLL・G-COST・G-USEの公開条件も別途判定する。[照合したOriginal・Ticket・裁定と元ログのSHA-256](../evidence/palw/fow1-signed-readout-recheck-2026-11-25.json)

`cargo +1.93.0 check --locked --offline -p misaka-palw-remote --features private-tb1-tir --lib` は最終版で成功。`cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --test qf1_work_disclosure` は1件成功。開示全体の上限判定を調整した後、最終ソースからencoder・reader・定数を取り出し、同じ回帰試験1件を再実行して成功した。変更した2件のconsensus fixtureの実行は未検証。

### 第4回の保存DBからのPanel復元と適格性評価

EVM有効の現行consensus試験バイナリで、保存した3ピア分のDBをRAM上の複製から1ピアずつ開き直した。各ピアで同じOriginal、PANEL qualification・seed、抽選2席の承認署名、受付commitment、期限DAA 60の選択tipが一致した。別の複製で受付資料の予約IDを変更するとintake rootの照合で拒否され、署名の末尾1 bitを反転すると署名検査で拒否された。再現には `recovered-panel-intake-databases.tar.zst` を展開して下記を使う。新しい開示を含む候補の投稿・Final・権利消費は未検証。[保存DBと復元値](../evidence/palw/fow1-retained-assessment-2026-11-25.json)

```sh
for peer in 0 1 2; do
  MISAKA_PRIVATE_COLD_PANEL_DIR="$out/peers/$peer" MISAKA_PRIVATE_COLD_PANEL_SEED=1 MISAKA_PRIVATE_COLD_PANEL_INTAKE=1 RUST_MIN_STACK=67108864 "$CONSENSUS_TEST_BINARY" private_read_cold_panel_receipt --ignored --test-threads=1 || exit 1
done
```

G-COSTでは固定4入力・2経路・順序を逆転した2回の記録と、同一仕事の6 Ticketの記録を、それぞれ登録tokenizer・事前計画・元ログから再照合した。G-USEではRound 15の算術提出の2問に対する回答・Readout・Ticket・2台の再実行記録を照合した。3件のレポートはいずれも保存済みJSONとバイト一致した。算術履歴の照合は圧縮アーカイブから直接読み、展開コピー105,687,544 Bを省ける。切り詰め、余分な行、異なる延長、正しいフレームの後ろの破損を拒否した。これは監査用コピーの容量で、Pruned Nodeの実測は第8回で扱う。[元資料と再現手順](../evidence/palw/mas1-qf1-native-2026-11-21/README.md)

G-COLLはQF1に適用できる規則が未確定。G-COSTは費用閾値・誤差範囲が未採択でUNKNOWN。G-USEは提出2問の正答までを確認し、Round 16の選出Panelによる評価はpending。QF1はREVIEW_HOLDを維持する。

### 第4回の通常合意fixtureでの開示・Court期限

前節のEVM有効テストバイナリで、登録済みEXECとPANELを通る短時間fixtureをRAM上で順次実行した。目標を満たすTicketと落選Ticketを別々のOriginalで確認し、それぞれ23.11秒と23.00秒で成功。どちらも選出席が再実行し、署名を受理した後、DAA 61でCourtが `ReplayVerified` となった。期限ブロックのdeltaを戻すと `Pending` と親rootが復元され、DBを開き直すと期限時のrootと結果が一致した。当選側の数値work keyは通常の開示読取りで復元でき、改変したjobとinventoryを拒否した。Gate C有効下で同じ当選Originalの独立認証は未成立だった。[両Original・受付ブロック・期限ブロック](../evidence/palw/fow1-native-court-and-publication-2026-11-25.json)

別の3プロセスfixtureで `read_panel_approved_qf1_publication` を実行し、同じ選択親のPanel承認と署名済み登録から3者が同一work keyを復元した。class・Readout・inventory openingの変更と承認より古い親を拒否。親プロセスの試験は11.35秒で成功した。これらは短い成熟条件と小型TIRを使う隔離fixtureで、実Qwenの採掘Final・権利消費は未達。現作業ツリーでは `Qf1TerminalUpdate::SuccessFinal` を構築するのはテストだけであり、採掘適格性を選択親の状態で検査して成功Finalを生成する通常入口が残る。Court期限とGate Cの間をこの入口で接続する必要がある。

```sh
export MISAKA_PRIVATE_QF1_TOKENIZER="$TOKENIZER" RUST_MIN_STACK=67108864
MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear "$CONSENSUS_TEST_BINARY" private_registered_tb1_pending_freeze --ignored --nocapture --test-threads=1
MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_TB1_LOSING_TICKET=1 "$CONSENSUS_TEST_BINARY" private_registered_tb1_pending_freeze --ignored --nocapture --test-threads=1
MISAKA_PRIVATE_QF1_NO_DISPUTE=1 "$CONSENSUS_TEST_BINARY" private_tb1_stable_panel --ignored --nocapture --test-threads=1
```

### 第4回の候補検査と採用basis

登録EXECのwork開示を読む通常の合意処理で、Courtと同じ計算により、登録TIRのcommit tile、入力・出力長、Readout chunk数から必要なtrace葉数を求める。申告数がその値または登録上限に適合しない場合は、候補のwork key・Gate Cの認証対象を返す前に拒否する。Original自体は従来どおりCourtの反証対象として取り込める。

上記と同じ登録tokenizer・小型TIR・RAM上のDBで `private_registered_tb1_pending_freeze` を3回実行した。全て `MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1` で、各行に記載した環境変数だけを追加した。

| 追加設定 | 照合結果 | 実行時間 |
|---|---|---|
| `MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear` | 正規仕事のkeyを復元、Court期限の巻戻しとcold reopenが一致。独立Gate C認証は未成立 | 23.28秒 |
| `MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_TB1_WORK_EXTRA_TRACE=1` | 正規実行からtrace葉数だけを1増やしTicketを再計算して所有者が署名。隔離目標を満たし、受理済みOriginalとして読み出せる状態で、仕事検査が葉数不一致を拒否 | 6.54秒 |
| `MISAKA_PRIVATE_TB1_TRACE_GEOMETRY=extra` | 別のCourt fixtureで余分な葉を反証し、受賞を停止。裁定deltaを巻き戻すと未決状態と親rootが復元 | 5.12秒 |

RFC-0067 §5（採択commit `7ad72a9e0`）では、この種の技術互換と個別claim検証を両方の採用basisに要求する。G-COLL・G-COST・G-USEの自動審査はKERNEL_AUTOの条件。BFT_DECISIONには対象と世代に一致する正規の採用YESが必要で、UNKNOWN評価はそのまま保持する。現ツリーの `palw_mas1::new_mining_allowed` は旧自動審査の述語であり、これを両経路共通の成功Final判定に使えない。現行台帳から採用basis・共通互換・停止集合を認証してFinalへ渡す接続と、その後の権利消費が残る。

### 第4回で確認したgenesis登録とモデル停止の境界

選択親のMODEL_STOP対象判定で、起動時に検証したgenesis登録と台帳のclass、artifact、PWU規則、slash単価を照合する。genesis登録として扱うには登録DAAがネットワークのgenesis DAAに一致し、登録者bondがないことも必要。親・登録内容に矛盾があれば、停止対象の判定をエラーで終了する。

EVM有効の合意テストバイナリで以下を順次実行した。第1件は実genesis、異なる親・artifact、矛盾するDAA・PWU規則・単価を確認し、0.22秒で成功。後続の2件はRAM上の独立した3プロセスで追加登録モデルを検査した。artifact単位は3.26秒で全台が同一の停止決定と `BftSafetyStop` を観測。rule単位は3.20秒で全台が同一の停止決定と `RuleDisabled` を観測。いずれも分岐変更後に元の停止決定を全台で拒否した。

```sh
"$CONSENSUS_TEST_BINARY" genesis_control_exemption_checks_immutable_registration --test-threads=1
"$CONSENSUS_TEST_BINARY" private_moe2_three_process_registration --nocapture --test-threads=1
MISAKA_TEST_MAS1_RULE_STOP=1 "$CONSENSUS_TEST_BINARY" private_moe2_three_process_registration --nocapture --test-threads=1
```

3署名で実証したのはMODEL_STOP。採用投票のYES/NOからFOW1の成功Final・権利消費への接続は引き続き未達。

### 第4回で照合した登録履歴と初期契約

genesis時点で登録履歴の保存が有効なネットワークでは、モデル停止対象の判定に同じ選択状態の登録履歴（aux 141）を用いる。登録ブロックが当該ネットワークのgenesis hashであり、履歴の版、class、登録DAA、登録形式、登録時のcarriage digestが起動時の登録内容と一致することを確認する。履歴が必要な設定で行が欠落した場合はエラーにする。登録履歴の保存開始がgenesisより後の場合は、当時の行を要求せず既存の台帳・起動時登録の照合を使う。

EVM有効の合意テストバイナリで次の2件をRAM上で順次実行した。1件目（0.29秒）は実受理された追加TIR登録のcarriageを元genesisのclass・artifact・PWU規則・単価と組み合わせた入力を拒否した。同じDBを開き直した後も、元genesisだけが免除され、同じDAAで別のgenesis hashを指定すると拒否された。2件目（0.17秒）は既存のgenesis構成と、必要な履歴行が欠ける場合の拒否を確認した。Pruned IBDで取得したsnapshot上の再検証と、FOW1の採用basisに基づく成功Finalは未実施。

```sh
"$CONSENSUS_TEST_BINARY" isolated_wide_carrier_origin_matches_actual_accepting_block_and_survives_same_db_restore --test-threads=1
"$CONSENSUS_TEST_BINARY" genesis_control_exemption_checks_immutable_registration --test-threads=1
```

### 第4回・FOW1が参照するMAD2採用票の読取り

RFC-0067 §4の隔離署名domainで、MODEL_ADMISSIONの専用roster、提案、YES又はNOの定足数を順に別々のnativeブロックへcommitする読取りを実装した。rosterはローカルの隔離設定が指定する。署名はchain/genesis、roster・epoch、提案・開票anchor、descriptor・kernel契約、要求分類・origin、policy・世代・期限、採否に束縛する。ローカル署名は同一投票枠の本文をディスクへ保存し、ファイルとディレクトリの同期後に署名を返す。鍵と同じ予約ディレクトリを保持する前提であり、コピーからの巻戻しや喪失後の二重署名を防ぐ保証はない。

RAM上のEVM有効fixtureで4鍵中3署名のYESとNOを別の提案で検査した。反対票への署名流用、2票、重複slot、要求分類の変更、古い選択親、期限切れの開票を拒否。投票ファイルを開き直すと同じ票の再署名は成功し、反対票は拒否された。重い分岐へのreorg後に旧採否の参照が失敗し、同一提案を新しいanchorで開き直しても元の署名は使えず、同じ投票者による反対票への変更は拒否された。1件成功、0.59秒。

```sh
"$CONSENSUS_TEST_BINARY" mad2_reads_yes_and_no_from_separate_native_proposals --test-threads=1
```

このfixtureの提案digestとrosterは隔離用入力。公開用の権限割当、登録済み契約の技術互換との照合、採用履歴の正規台帳への原子的反映、FOW1の成功Final・権利消費との接続は未達。実プロセス再起動や電源断も今回の試験には含まない。

### 第4回・当選Originalと登録済み追加モデルの採用票

隔離設定のMAD2追加モデル投票で、投票を開始する時、署名する時、定足数を利用する時に、選択親の同じ状態から追加分類、市場、登録artifact、TIR、tokenizer、layout、Readoutを照合する。kernel契約の識別値にはTIRのprimitive集合、Court版・root、上限とfence2の適用状態を含めた。FOW1の仕事に票を対応させる読取りでは、通常の仕事検査が復元した登録情報・Readoutの開示バイトと現在の親を要求する。

短時間fixtureで登録済み小型TIRを使い、正規Ticketの当選Originalの仕事を復元した。4鍵中3署名のYESが含まれた親で同じ仕事を再度読み、その追加モデルの採用票と一致した（7.19秒）。異なるartifact、改変Readout、誤ったkernel契約の提案、登録の照合を経ていない採否、古い親の仕事を拒否。採否の包含ブロックは `351cb0a58ab850c568c5f642fb80d4d89d5d3c11474cd59d096ffdd1f141038fbe3ed99f110f16dfc84b6d95b2858d977364dfc30cf13f6b979a2e78db118bc7`。初回のfixtureはpruning point保存前のテンプレート作成でHistoryUnavailableになったため、投票ブロックごとに既存の保存完了確認を入れて再実行した。

同じバイナリの別fixtureでCourt期限まで進め、ReplayVerifiedの仕事に独立Gate C認証が未成立であることを確認した（23.07秒）。採用票の読取り後に採用履歴を原子的に更新する処理、現行policy・世代・停止集合から成功Finalを生成して権利を一回消費する処理、実Qwen3の検証は未達。QF1はREVIEW_HOLDを維持している。

```sh
export MISAKA_PRIVATE_QF1_TOKENIZER="$TOKENIZER" RUST_MIN_STACK=67108864
MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=1 "$CONSENSUS_TEST_BINARY" private_registered_tb1_pending_freeze --ignored --nocapture --test-threads=1
MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear "$CONSENSUS_TEST_BINARY" private_registered_tb1_pending_freeze --ignored --nocapture --test-threads=1
"$CONSENSUS_TEST_BINARY" mad2_reads_yes_and_no_from_separate_native_proposals --test-threads=1
```

### 第4回・採用の提案本文と署名をブロックから再検証

隔離MAD2の投票では、開票ブロックに提案本文、採否の包含ブロックに開票anchor・YES/NO・slot順の署名を、手数料付きのlifecycle transactionで収録した。受信側は指定した採否ブロックから開票ブロックをたどり、両方の本文、coinbase commitment、選択親、登録済みQF1契約、期限、定足数を照合する。rosterの鍵と権限起点は隔離設定から与える。

前節と同じ3コマンドを実行した。採用票fixtureは7.48秒、Court期限fixtureは23.18秒、別提案のYES/NOとreorgは0.64秒で成功。提案payloadは290 B、4鍵中3署名の採否payloadは10,012 B。これは取引のpayload長であり、ブロック全体やRocksDBの物理増加量は別途測定が必要。採否ブロックは `643fa891b0a9e4144da947e5be27a207cd5ef2e320f47476d794ab859d7149c0f9ec3b3edbeb4d54f68eb3e67d4bb26d9ed02b39d1648d6ccbab02658e1f23b8`。

同じcoinbase commitmentを置いて署名本文の末尾を1 B欠落させた別ブロックを、投票の読取りで拒否。正常な採否ブロックのRocksDBチェックポイントを開き直すと、state rootが一致し、同じ当選仕事・提案・採否を復元した。復元側に渡したのは権限設定、採否ブロックhash、Originalの参照と仕事開示で、投票者の署名配列や提案構造体は渡していない。その試験コピーだけで開票ブロック本文を欠落させると再読取りは失敗した。pruning境界を越えて採用状態を復元するための台帳への反映とPruned IBDでの検証は未達。

### 第4回・採用投票の権限を選択親の台帳へ保存

隔離genesisが指定するTB1計画のisolation欄に、投票者の順序付き公開鍵、epoch、障害許容数、policyと版を束縛した。通常の候補生成と検証で登録内容を照合し、最初の正当な登録について選択親の台帳に200 Bの行を保存する。権限登録payloadは15,860 B。最初の試作ではgenesis coinbaseが273 Bで204 Bの上限を超えたため、既存の計画IDを介して照合する方式に修正した。合格したfixtureでのgenesis coinbaseは201 B。

前節の3コマンドで、別提案のYES/NO・reorg（0.59秒）、登録モデルの投票（7.84秒）、Court期限・独立認証未成立（22.72秒）が成功した。登録モデルの試験では、鍵の変更と異なるpolicy版を拒否し、登録deltaの巻戻しで元のrootに戻ること、チェックポイントを開き直して権限登録本文を除いた試験コピーでも同じ採用票を読めることを確認した。開票・採否の本文は引き続き必要。200 Bは行の符号化長で、物理容量の削減量ではない。採用結果の正規台帳への反映、成功Finalと一回消費、全対象状態のP2P同期・容量予算は未検証。

### 第4回・通常テンプレートでMAD2の開票を生成

隔離genesisの投票権限を登録した後、minerのextra dataを空にし、提案本文を含む手数料付き取引だけを候補に渡した。Nodeは選択親の権限行、policyと版、候補のDAA期限を照合して開票commitmentを生成する。異なるpolicy版と期限切れの候補にはmarkerが付かなかった。署名側とNodeは同じ提案codecとcommitment計算を使用する。

前掲の3コマンドで、別提案のYES/NOとreorg、登録済みQF1の開票から3署名の採否再検証（7.92秒）、Court期限時の独立認証未成立（23.02秒）を確認した。採否ブロックのmarkerは引き続き署名側が供給する。開票はモデルの採用決定ではなく、正規台帳の採用履歴、成功Finalと権利消費は未達。

### 第4回・通常テンプレートでMAD2の採否を生成

登録済みQF1の採否取引にepoch・故障許容数・順序付き公開鍵を収録する隔離形式を追加した。Nodeは選択親の権限行に対して公開鍵を認証し、開票の祖先関係、policy、期限、採否に対する署名と定足数を照合して、空のminer extra dataに採否マーカーを設定する。以前の採否形式の読取りも維持する。

前掲の3コマンドが成功した。登録済みQF1（8.09秒）では、4席中3署名で採否が生成され、鍵又は署名の1バイト変更、YESからNOへの改変、2署名ではマーカーが付かなかった。チェックポイントを開き直した試験コピーでは、外部から投票者鍵を指定せず、権限登録ブロックの本文を除いても採否を復元した。別提案のYES/NO・reorgは0.58秒、Court期限・独立認証未成立は23.15秒。鍵を含む採否payloadは17,832 B（旧形式10,012 B）。開票本文の削除後は復元できず、採用履歴の正規台帳への反映、成功Final・権利消費、Pruned IBDの全状態比較は残る。

### 第4回・過去本文の削除後に採否と当選仕事を照合

採否を保持する隔離genesisを追加した。Nodeが開票commitmentとpolicyを照合すると282 Bの未決行を置く。署名・権限・期限・coinbaseを照合した最初の定足数で、同じ遷移内で未決行を消し、開票anchor・提案・採否・署名集合のcommitmentを含む411 Bの行に置き換える。以後の通常読取りは、指定親の行と同じ親で検証した登録モデル・kernel・Readoutから票を復元する。既存の隔離genesisは従来の本文読取りを使用する。

前掲の3コマンドが成功。登録済みQF1（8.13秒）では、採否マーカーを手動指定して署名を改変したブロックから採否行が作られず、正規採否後の同じ票の再提出にもマーカーが付かなかった。採否deltaの巻戻しで元のrootと未決行が戻った。採否の前後でPALW状態をsnapshot carriageに符号化し、期待rootを指定してそれぞれ復元した。チェックポイントを開き直した試験コピーから権限登録・開票・採否の3本文を削除しても、元の仕事に対して同じ票が得られた。別提案のYES/NO・reorg（0.60秒）と、Court期限・独立認証未成立（23.14秒）も成功。

行サイズは値の符号化長で、キー、delta、RocksDBの物理容量を含まない。未決期限切れの整理と、複数提案の世代・停止履歴を踏まえた実効採用判定は残る。これらの短時間fixtureでは、ネットワーク越しの全状態同期、成功Final、権利消費を検証していない。

### 第4回・投票が成立しなかった開票の期限と巻戻し

隔離MAD2台帳の読取りで、開票の排他的期限と選択親のDAAを比較し、未決の提案を投票中・期限切れに分類する。期限切れでも282 Bの元の提案行を保持する。期限ごとの全行走査・タイマー・追加の履歴行は不要。期限到達時に物理容量が回収されるわけではない。

前掲の3試験が成功した。登録済みQF1のfixtureでは期限前に3署名を用意し、通常の候補がDAA 16に達した時点で採否マーカーを生成しなかった。手動で正しいマーカーを付けても採否行は作られず、選択親の状態は期限切れとなり、新たな署名も拒否した。以前に確定した別提案の採否行は一致した。期限切れの状態を期待root付きsnapshotから復元し、期限ブロックのdeltaを戻すと期限直前のrootと投票中の状態に戻った（8.66秒）。別提案のYES/NO・reorgは0.62秒、Court期限と独立認証未成立は23.28秒。最初の試験で連続する2ブロックが同じDAAになることを確認し、実際の候補DAAを観測して期限境界を検査した。採用世代と停止集合を使う実効採用判定は残る。

### 第4回・登録モデルの採否を決定世代から選ぶ

世代参照を持つ隔離genesisで、署名検証済みの採否を履歴に書くのと同じ遷移で、対象ごとに最新の決定ブロックを指す64 Bの値を更新する。対象はdescriptor、kernel契約、要求分類、origin、policyと版で識別する。現行の読取りは、指定された選択親から登録済みの追加モデル、Readout、kernel、権限policyを照合してその対象を求める。呼出側が採否ブロックを指定する履歴読取りも維持する。

この隔離設定では、同じ対象の同じ世代は最初に正規包含された定足数を保持し、より大きい決定世代で参照を進める。旧世代や同世代の別案件は個別の署名済み履歴として残る。これは定足数が決定した世代の索引であり、再審を開始できる者や次の世代の割当、停止集合、fork指定の原子的反映を確定する処理は残る。

前掲の3試験が成功した。登録済みQF1の仕事で、世代1のYES、世代2のNO、遅れて到着した世代1のYESと世代2の別案件のYESを順に包含した。最新参照は世代2の最初のNOを返し、個別履歴は各票を返した。各更新でsnapshot復元とdelta巻戻しを照合し、異なるkernel・要求分類・policy版で同じ参照を引けないことを確認した（9.84秒）。Court期限・独立認証未成立は23.07秒。旧隔離fixtureのreorg判定は経済指標同点時のハッシュ順で一度失敗し、既存の浅い分岐の累積work判定を使う構成に切り替えた。修正後0.58秒で成功し、同じバイナリでさらに6回連続成功。直接実行は前掲のRUST_MIN_STACK=67108864が必要で、これを省いた初回はスタック上限で中断した。

### 第4回・追加モデルのburn停止／解除の署名と資金を事前検証

RFC-0053用の隔離取引本文にnetwork、policy、class・artifact、action、期待sequence、直前操作、burn額・上限、排他的期限を束縛した。直前操作は初回がnetwork・class固有で、次回以降は取引IDと受理位置を含む。事前検証では第1入力のP2PKH ML-DSA-87とSIGHASH_ALLを照合し、解除時は選択親の登録bondの鍵で専用domainの署名も検査する。入力UTXO・出力から実手数料を照合して、先行控除後に残る額から必要額を差し引く。

登録済みQF1の短時間fixtureで実UTXOを使って署名したcarrierをNodeのmempool検証に通し、そのUTXOと算出手数料を使った。試験用に残余100 sompi、停止10、解除20を指定した。停止後の候補はsequence 1・徴収可能額90、登録者署名による解除後はsequence 2・80となった。不足額、上限超過、誤network、期限切れ、署名改変、第三者解除、異なる前操作、同じ停止の再提出を拒否した。試験中、登録市場とMAD2採否行を含む選択親rootは一致した（10.19秒）。別提案のYES/NO・reorgは0.62秒、Court期限と独立認証未成立は23.34秒。

既存plannerの対象判定・世代・genesisと年次fork免除も次のコマンドで検査した。
```sh
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --features evm --lib model_pause_v1_ -- --test-threads=1
```
今回の支払は事前計算で、停止と解除はそれぞれ同じ未使用UTXOから構築した独立の候補である。実際に連続して資金を消却する試験は未実施。正規の受理位置における状態行とcoinbase手数料控除の原子的な確定、各claim効果直前の停止照合、reorg時の両者の復元が残る。試験額は公開パラメーターに割り当てていない。

### 第4回・停止取引のお釣りを使った解除（11月26日）

登録済みQF1の隔離fixtureで、通常のテンプレートに停止carrierを載せ、後続ブロックのacceptance dataから元ブロック、取引IDとブロック内位置を取得した。停止取引の使用済み入力がvirtual UTXOから消え、お釣りを次の解除取引の署名済み入力として使えることを確認した。解除には登録者署名と最初の取引から導出した操作参照を使った。解除後には停止のお釣りも消え、最後のお釣りと最初の入力の差がNodeの算出した2件の手数料の合計と一致した。2件の受理位置と実入力を使った事前計算で停止・解除の順序、世代2、試験額10・20 sompiの控除後の合計を照合した。

登録済み投票・連続支払fixtureは10.78秒、独立認証待ちのCourt fixtureは23.41秒で成功。通常経路で確定したのはcarrierのUTXO支払まで。burn分をcoinbaseの徴収可能額から控除して停止行と原子的に確定する処理、およびそのreorg復元は未達。第8回の全経路・容量判定は保留。

### 第4回・停止と解除が参照する直前操作の位置（11月26日）

旧事前計算では、同じ署名済み停止取引が別ブロックの同じindexに入ると操作IDが一致した。隔離用操作IDのドメインをv2とし、network、取引ID、収録元ブロックhash、ブロック内indexを束縛した。呼出元が選択枝のacceptance dataを供給し、事前計算ではそのentryの取引IDと入力の取引IDも照合する。同じ収録元の同一entryを再採用した場合には同じ操作を参照できる。

通常の受理記録から停止・解除の2件を取り出し、同一位置での再計算、収録元だけを差し替えた場合の操作IDの相違、旧参照による解除と他取引のentryの拒否を短時間fixtureで確認した（10.77秒）。別提案のYES/NOとCourt期限の回帰も成功。収録元差替えは事前計算への入力を変えた試験。停止行と供給量の更新を伴う実分岐の切替・復帰は未検証。

### 第4回・年次カタログでcontainer rootを指定されたモデル（11月26日）

QF1の登録済み仕事からMAD2の追加モデル用kernel契約を読む入口は、登録時のinventory rootだけでカタログを照合していた。報酬配分側と同じ `palw_model_tier_at_v1` を使い、選択状態のDAAで有効なPALWART3 container bindingも照合する。これにより、年次指定に該当する仕事について追加モデル用の最新採用票を返す前に処理が止まる。

TIR登録者のcontainer bindingを適用するfixtureで、同じ登録・market・カタログを使って前後を比較した。binding前には追加モデルとしてReadout開示検査に到達し、binding後には追加モデル対象外になった。対象fixture 1件、burn停止の対象・世代2件、モデルtier 11件が成功。再現用のkaspa-consensus-coreのlibテストフィルターは順に `tir_catalog_tier_reaches_a_class_through_its_bound_container_root_only`、`model_pause_v1_`、`palw_model_tier_v1::tests`（features evm）。指定モデルの成功Final、claim認証、報酬決済までの連続動作は未検証。

### 第4回・成功Finalの期限境界とOriginal受理記録（11月26日）

FOW1認証対象に、既にそのIDが束縛していたOriginal収録ブロックを保持した。Gate Cの認証済み読取りでは、選択状態の受理記録にある収録ブロックと原期限が両方とも一致することを必要とする。成功Final用のCourt判定は、正しい選択親のDAAが原期限より小さく、候補DAAが原期限以上で、同一OriginalのCourtがReplayVerifiedである場合に成立する。

coreの `fow1_` 試験2件が0.02秒で成功した。親DAA 11・期限12で候補11を拒否、12と15での最初の跨ぎを許可し、親が既に12の場合を拒否した。収録元だけ、期限だけ、選択親だけを変えた入力も拒否した。作業ツリーにあった成功Final handoffからこの判定を呼び、独立抽選・投票・認証撤回の既存fixtureも実行した。handoff側の未コミット差分は今回のcommitに含めていない。通常ノードによる採掘適格性の確定と成功Final生成、実Qwenの決済は未検証。

### 第4回・成功FinalからGate Cへ渡す状態と差分の往復（11月26日）

前節で未コミットだったFOW1の引き渡しを統合した。成功Finalを受けるfoldはCourt閉鎖と原期限を照合し、独立認証の待機行、classの件数、数値workとattemptの帰属を同じtransitionに記録する。既に帰属を持つキーへの再書込みとclass件数のoverflowを拒否する。認証済み読取りはworkとattemptの双方が同じ認証対象に属することを要求する。

隔離fixtureでこのfoldが実際に書いた範囲のdeltaを逆適用し、両方の帰属行と認証待機行が消え、元のrootに戻ることを確認した。順適用でFinal直後のrootを再現した。独立抽選・投票後の認証とCourt承認消失後の撤回も成功。kaspa-consensus-core（features evm）の `palw_security_certification_fold_v1::` は6件成功（FOW1の2件を含む）。入力となる成功Final、producer、exposureはfixtureが与えている。通常ノードによるこれらの決定、ブロック単位のreorg、報酬支払は未検証。

### 第4回・独立Gate Cの責任額をEXEC予約から確定（11月26日）

成功Finalの入力から責任額を取り除いた。認証対象のIDに既に束縛されたOriginalの予約キー・EXEC close・slotを保持し、Finalを処理する時点の台帳で、有効・未消費・同一classの予約を要求する。認証者の担保計算にはその行の `liability_sompi` を使用する。0または既存の認証行のu64に収まらない額は拒否する。予約の読取りによる報酬消費はない。

隔離fixtureで資金提供者bond 4、producer bond 1、報酬予約1,000 sompi、責任額1,200 sompiを指定し、認証行のproducerと責任額を照合した。予約なし、異なるclass、Finalと同じDAAで失効する予約は、いずれも状態rootを維持して拒否された。Gate Cの6試験、MOE2決済エンジンの4試験が成功。通常producerの選出と受取先の登録署名への照合、実際の報酬支払はこの試験の対象外。

### 第4回・成功Finalで照合するproducerと受取先（11月26日）

隔離用のbond付き成功Finalでは、EXEC登録のbeneficiaryにproducerのnative bond IDと台帳上の受取payloadのcommitmentを入れる。EXEC ownerが署名した登録からその値を認証対象へ引き継ぐ。Finalのfoldでは、提示された有効なbondとその受取先からcommitmentを再計算し、そのbondの登録済みML-DSA-87鍵による同意署名も検証する。署名対象は専用domain、network/genesis、EXEC登録キー、割当slotを束縛する。登録キーとslotが決まれば抽選結果に先立ち署名できる。

Gate C foldの6試験が成功。資金提供者bond 4とは別のproducer bond 1の正しい署名を受け、bond 4への入替えと、同じproducerが別slotについて作った署名を拒否した。拒否後のstate rootも元の値と一致した。MOE2決済エンジン4試験も成功。fixtureがbondと同意署名を引き渡しており、実ノードでの配布、通常候補からの成功Final生成、実支払は未検証。既存の評価用登録をそのまま支払資格とは扱わない。

### 第4回・受理済みOriginalのブロックからproducer同意を読む（11月26日）

署名済み登録の受取人に対応するbondと同意署名を固定長のlifecycle payload（FOW1PR01）で運ぶ。ノードの仕事読取りが、選択親の受理記録にあるOriginalブロックの取引を調べる。該当取引が1件なら登録受取人・bond鍵・network・登録キー・slotを検証し、認証対象にproducerを保持する。成功Finalの入力から任意のproducerと署名を指定する欄を取り除いた。該当取引0件では既存の評価用仕事を復元できるが、この対象で成功Finalを確定することはできない。

料金付きの3件目の取引として同意をOriginalに同梱し、通常テンプレートからブロックを受理して、Nodeが当選仕事と同じ登録のproducer bondを復元した。前掲の登録済み投票fixture（10.78秒）とCourt期限fixture（23.26秒）に環境変数 `MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1` を加えた。同じバイナリで変数を外し、producer未設定の従来の評価経路も確認した（10.58秒、23.16秒）。CoreのGate C fold 6件が成功し、重複・短い署名・別slot・受取人違い・同意なしでの成功Finalを拒否した。Original本文を取得できる隔離Nodeでの確認。pruning後に同意を復元する経路、通常の採掘資格に基づく成功Final生成、実支払は未検証。

### 第4回・Original本文回収後のproducer帰属（11月26日）

認証済みEXEC closeからOriginalを受理する際、同意署名に成功したproducerのbond outpointだけを受理記録と同じdeltaに保存する。値は取引ID 64 Bと出力index 4 B。署名4,627 Bは追加行に含めない。読取りはclaimと収録元ブロックの双方を照合する。同意なし・不正な同意から帰属行を作らず、OriginalのCourt受付を維持する。旧状態に帰属行がない場合、仕事の読取りでは既存の本文検証を使用する。

実ノードの隔離fixtureで、正しい同意を持つOriginalの帰属を期待root付きsnapshotから復元し、受理deltaの逆適用で行が消え、順適用で元のrootとbondが戻った。再度開いた試験用checkpointからOriginal本文を削除して取得不能であることを確認し、その状態でも同じbondを読めた。登録投票・Court期限fixtureは同意ありで10.84秒・23.24秒、同意なしで10.55秒・23.07秒。別提案のYES/NO試験は0.62秒。68 Bは値の論理長。空のPruned NodeのIBD、仕事全体の本文なし再構成、全対象状態の物理容量予算は別の検証を要する。

この隔離fixtureのQF1登録はlocal inference用で、EXEC closeの `into_mining_reservation` は当該profileからの報酬予約を明示的に拒否する。通常の成功Finalに必要な予約を得るには、採用根拠と技術条件を検査する採掘用の登録入口が必要である。

第4回・署名済みEXEC登録からの採用票照合。remoteの `read_exec_registration_vote` は当選後のworkを要求せず、所有者署名を検査して登録のclass・artifact・tokenizer・Readout descriptorから現在の選択親に対応する追加モデルの採用票を読む。抽選後の照合と同じnative状態を使う。未決では票なし、対象親が変わった場合は再試行。採用票の読取りだけでは採掘登録・報酬予約は成立しない。

既存の短時間 `private_registered_tb1_pending_freeze` を登録tokenizer・native certificate・producer consentで実行。登録済みballotを使う設定で1件成功（10.76秒）。同じ署名済み登録で未決、cold reopenした第1世代YES、第2世代NOを照合し、誤署名・壊れた登録・改変した開示を拒否した。Court側の設定でも1件成功（23.26秒）。fixtureは既存登録を再利用しており、新しい採掘用登録からの当選・予約・成功Finalは未検証。

第4回・EXEC閉鎖前の採用票。認証済み入力batchの `pre_draw_registration_vote` は所有者が署名したslotから登録anchorとモデルを取り出し、選択親の履歴と読取中のpruning import世代を照合する。親がEXEC closeのblue scoreかrelease DAAに達したら閉鎖済みとして拒否する。公開layoutとReadout metadataはEXEC準備前にエンコードでき、仕事の開示payloadも同じエンコーダを使う。

登録tokenizerを指定し、`MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1 MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=pre-draw` で `cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --nocapture --test-threads=1` を実行した。隔離genesisの4鍵のうち3鍵がYESに署名し、通常テンプレートで権限登録・提案・quorumを順に受理した。その後にEXECを準備し、所有者と資金提供者の署名を検査したslot 9で、閉鎖・releaseの前に同じ受理ブロックのYESを読んだ。選択親DBをcold reopenして同じ結果を得て、Readout開示の1ビット変更を拒否。3.42秒で成功。VSSの資格は既存fixtureの入力で、この実行は抽選前で終わる。

同じ試験のballot設定 `1` は10.89秒で成功。EXEC閉鎖前の権限未登録、閉鎖後の照会拒否、古い選択親での再試行、最新世代のNOと履歴上のYESを確認した。ballot設定を外したCourt試験は23.38秒、通常のYES/NO分離試験は0.58秒で成功。現時点の所有者署名は評価用登録。採掘用の意思・採用根拠を署名に含めてclose受理時に検証する経路、報酬予約、成功Final・決済は残件。

第4回・採掘意思を束縛するEXEC署名。隔離genesis `MD2PNL4` に、採用票の受理ブロックを含む専用の所有者署名preimageを追加した。資金提供者は、このpreimageの登録keyとslotから作る既存のMOE2同意に署名する。通常のEXEC close候補検査は、準備ブロックがコミットした親状態とcloseの選択親状態の両方で、追加モデルの登録、公開layout・Readout、kernel、現行policy、最新世代のYES、署名した受理ブロックを照合する。公開programは同じdescriptorのslot間で共有する。既存の隔離genesisではこの新しいcloseを採用しない。

前節と同じ `pre-draw` 短時間fixtureが3.69秒で成功した。通常テンプレートで作ったclose候補を合意の候補検査関数に渡し、新しい二重署名を受理した。評価用の所有者署名、評価用の資金提供者署名、別ブロックを参照する二重署名、1ビット変更したReadout開示を拒否した。正しい候補も報酬予約への変換は `REVIEW_HOLD` で停止した。通常のYES/NO分離、従来genesisでの票の履歴、Courtの3件も成功した。今回の確認は候補検査まで。VSS・Readyから通常受理で保持された資格にこの候補をつないだブロック取込、採掘権利の確定、報酬決済は未検証。

第4回・VSSからの通常ブロック取込。隔離genesisの予定でモデル登録と3者のYESを先に受理し、4 holderのVSSと署名済みReadyからEXEC/PANEL資格を再構成した。通常受理で保存した資格を読み、既存の登録tokenizerと短いReadout用TIRで、所有者・資金提供者が署名したslot 9を閉鎖した。minerのmarkerは空から通常テンプレートが生成した。ブロック受理後に選択状態のclose記録が同じ準備ブロックとfreezeを参照し、delta巻戻しで親rootに戻った。別DBとしてcold reopenした際のtip・root・freezeが一致し、当該slotの報酬予約は0件。資格とclose記録は通常ブロック処理で生成した。

登録済みtokenizerのパスを `MISAKA_PRIVATE_QF1_TOKENIZER` に設定し、`cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_mad2_intent_from_native_ready -- --ignored --nocapture --test-threads=1` が11.06秒で成功。従来のVSS/Ready試験26.37秒、候補の改変拒否3.70秒、票履歴10.78秒、Court試験23.31秒も成功した。単一プロセスの隔離fixtureであり、このslotの抽選後の実受賞・成功Final・決済、複数ピアのreorg、物理容量予算は未検証。

| 今回の受理記録 | hash |
|---|---|
| 準備ブロック | `c5a555e1c236af4d00304f51ba6217a86305e5d56b9b661e8613dd572e2cd17be3cb1a686ba9c7457a05d2f61260d8540b5464ab0a3feb9c6019f91027de0d42` |
| YES受理ブロック | `a8c1610ea97a5e8e285b64270471d78841799a38fd45a43b19ae825e8ee0b8fa7853d9f5a99aa633ee74df7681e510d8d240534aa5071e18e903f10f954aa4b1` |
| EXEC close | `67e4ed90786e3c284838c622dfa867ea32f14fb29df7de79ecc4c6db50ffa0a928b21928ee83d58f3ff1bf3459a0834c320342f53a1068c4ea4d987197e35abe` |
| cold reopen後の状態root | `cdc7d3a8c4bd47c482c76616241d007beabeaf9da0d44f280423f9c272149de24ba519079bb0fced8b5992d399b7f2cb6ce77e25fa307d87defcb565bee388a5` |

第4回・pruning後のholder release。holderが署名するfreeze IDは通常受理済みのEXEC close記録にあり、PANELでも受理記録から復元できる。holderの署名とpublisherのパケット収集で、選択台帳の記録・準備資格・closeヘッダー・選択履歴からこれを解決するようにした。close本文とcoinbaseの再取得はこの読取りでは不要。資格・署名窓・finality・pruning import世代・曝露journalの検査は署名経路で実施する。保存する台帳行やsnapshotの形式は変更していない。

前節の `private_mad2_intent_from_native_ready` が16.70秒で成功。成熟を注入していない通常分岐を元のrelease窓のDAA 35まで進め、cold checkpointのclose取引本文だけを取り除いた。保存台帳から同じfreeze・資格を読み、別purposeを拒否した。元のノード、本文を欠くノードともに `FrontierNotCovered`、frontier・finalizedなし、unique work 0。署名時の理由はともに `NativeUnavailable("native finalized head unavailable")` で、shareは公開されなかった。

`private_tb1_preparation_reads_ready_from_accepted_blocks` は26.46秒で成功。この既存fixtureは成熟済みの決済状態を用意する。別checkpointでclose本文が取得不能でもEXEC・PANEL両方の保存shareから署名を返し、元ノードで資格とfreezeを使い各packetを認証した。この陽性試験は独立Gate Cからの自然な成熟を実証しない。本文欠落の再現は隔離コピーで行った。Pruned IBD全体、証明書候補を検査する側の履歴依存、物理容量予算は別途検証する。

第4回・証明書を受理する側の履歴依存。通常テンプレートと合意の証明書検査が、選択台帳のclose記録と準備資格からfreezeを復元する。署名済みrelease batch本文、元の観測点と候補親でのfinality、期限、certificate markerを照合する。PANEL用の受理記録では元のconsumer set・policy・資格とfreezeを照合する情報も保持する。今回の変更による新しい台帳行はない。

`cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_preparation_reads_ready_from_accepted_blocks -- --nocapture --test-threads=1` が26.62秒で成功。隔離DBで同じ成熟条件のままEXEC close本文を除去し、未知batchの拒否、正規batchの通常テンプレート生成、証明書ブロックの通常受理、証明書hashとseedの一致を確認した。PANELも本文欠落時の未知batch拒否と正規テンプレート生成に成功した。PANEL受理後のduty抽選は人口・資料割当をclose本文から読むため、この試験ではPANEL本文を受理前に戻した。旧MAD2履歴10.81秒、Court経路23.37秒の回帰試験も成功した。成熟したworkは既存fixtureのメモリ入力で、別DBへのcold reopenでは引き継がれない。自然なGate C成熟、PANEL本文欠落下でのduty生成、全体のPruned IBDは未検証。

第4回・PANEL本文をpruneした後の担当者抽選。隔離genesis `MD2PNL5` では、PANEL closeの受理時にprovider署名と全Originalの割当を検査し、ポリシーID、候補3名と重み、配席数、割当件数をcloseに紐づけて選択状態に保存する。行の値は285 B。証明書受理時は、この行、保持済みclose、選択枝のintake、認証されたseedから担当者を抽選する。同じ遷移で行を削除し、deltaで巻き戻すと復元する。旧genesisの抽選は従来の本文を読む。

短時間fixtureで新旧各2件が成功した。新genesisの `private_tb1_certifies_pruned_panel_from_native_ready` は受理済みVSS・Readyから、Originalが0件の場合にPANEL本文を取り除いて証明書を受理した（26.91秒）。登録済みモデルのOriginalを含む `private_registered_tb1_pending_freeze` では、証明書テンプレート生成前に試験DBからPANEL本文を取り除き、通常受理による担当者とdutyが独立計算と一致した（23.92秒）。pending行のwire検査・1 B切詰め拒否、snapshot再構成、受理時の行削除、deltaでの親root・行の復元を確認。旧genesisでもReady試験26.44秒、Originalを含むCourt試験23.76秒で成功。

再現は `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_certifies_pruned_panel_from_native_ready -- --test-threads=1`。非空の試験は同じcargoオプションでフィルタを `private_registered_tb1_pending_freeze`、引数を `--ignored --nocapture --test-threads=1` にし、実tokenizerのパスを `MISAKA_PRIVATE_QF1_TOKENIZER` に指定。`MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1`、`MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear`、`MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1`、`MISAKA_PRIVATE_TB1_RETAIN_DRAW=1` を設定する。最後の変数を外すと旧genesis。非空fixtureは準備資格をfixtureから渡し、両方の成熟試験は仕事量をfixtureから供給した。自然成熟、Pruned IBDの終端一致、未認証で期限切れのcloseの回収量、物理ディスク予算は未検証。PANEL closeを組み立てる時点には先行EXEC close本文への依存が残る。

### 第4回・EXEC本文回収後のPANEL close（11月26日）

隔離genesis `MD2PNL6` は、通常受理で署名・登録・freezeを検査したEXEC closeのconsumer setとpolicyのrootを128 Bの値として保持する。PANEL closeのテンプレートと受理側は、選択親台帳からこの2項目、資格、roundを復元し、既存のEXEC受理記録のfreeze commitmentを再照合する。正しいPANEL closeの受理と同じdeltaで128 Bの行を削除する。

短時間試験5件が成功。新genesisで受理済みVSS・Readyから空のintakeを閉じる試験では、使い捨てDBのEXEC本文を削除してholderのpayload・通常テンプレート・PANEL closeの受理・証明書・cold reopenまで検証した（26.68秒）。署名付きOriginalを含む試験では、資料の欠落、署名と異なる資料を拒否し、正しい資料だけで期待したPANEL commitmentを受理した（23.46秒）。EXECの2つのrootが保存元と一致し、snapshotは1 bit変更を拒否。PANEL closeのdeltaの逆適用でEXEC側の行と親rootが復元された。後続の履歴照合用にその試験DBのEXEC本文を戻し、別途PANEL本文を削除して証明書・担当者抽選も検証した。v5で空のintakeとOriginalありを再実行（26.50秒、23.47秒）、旧genesisでOriginalありを再実行（23.41秒）。

再現は `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_panel_close_after_exec_body_pruning -- --nocapture --test-threads=1`。非空の試験は直前節の `private_registered_tb1_pending_freeze` と同じ引数・tokenizerで、genesis選択の環境変数を `MISAKA_PRIVATE_TB1_RETAIN_EXEC_ROOTS=1` にする。成熟仕事量はfixture入力、非空試験のOriginal受付時にはEXEC本文が存在した。原本が失われた後の新規Original受付、成功Finalから独立Gate C・報酬への通常接続、自然成熟、Pruned IBD終端、物理容量予算は残件。128 Bは値の論理長で、DBの物理回収量は未測定。

### Round 16の保存済みPruned・Archival台帳の全PALW状態比較（11月26日）

既にP2P同期を終えた受信DBとArchival供給元をRocksDBの読取り専用モードで開いた。両DBのvirtual selected parentはDAA 51258の同一tipで、PALWの全状態と再構成した索引を比較して一致した。primary carriageの論理長は68,452 B、bond 8件、class 5件、モデル市場2件、kernel補助行5件。終端rootは `964c48a7a7f6c29d17a45972f388a451dc66db898a1897b1f0877503ce77a08d6b06dd3ce528e91163124a9ef980a88aaa5ad2d615befa134911c9627401b2c1`。

両DBの直近32ブロックのjournalと適用後rootを照合し、それぞれのjournalをメモリ上で逆適用してDAA 51242まで検証した。各段の親rootは子ヘッダーのcommitmentと一致。順適用でも各段の両状態が一致し、元のtip・root・全primary carriageに復帰した。1件成功、0.40秒。終了後に供給元118ファイルが保存manifestに一致し、受信側108ファイルも試験前のSHA-256と全件一致した。

再現には `MISAKA_PRIVATE_GENESIS_SUFFIX_DB` を閉鎖済みのRound 16供給元DB、`MISAKA_ROUND16_TERMINAL_DB` を終端受信側のconsensus DBに指定し、`RUST_MIN_STACK=67108864 cargo test --locked --offline -p kaspa-consensus --features evm --lib round16_cold_primary_state_and_suffix_match -- --ignored --nocapture --test-threads=1` を実行する。終端にはclaim、Court session、pending payout、MOE2決済が各0件だった。これらが存在する場合のPruned状態一致、外部資料の取得、競合枝の実選択、物理容量予算は個別の検証対象。今回の読取りでノードやS3実走は起動していない。

### 保存S3のclaimを含むsnapshot入力と宣言読取り障害（11月26日）

受賞UTXOを受理したDAA 7867を追跡している3保存世代で、停止DBからPALWの永続化済みtip recordと当時のクラス宣言を読み、Pruned IBDのnative snapshot入力を組み立てた。各世代の3ピアで、保存ブロック・journalに記録したroot・フレーム全バイトが一致した。受賞claimと後続claimをIDで確認し、宣言2件のクラス・artifact・登録DAA・内容を照合。通常のnative snapshot preflightは全9入力を受理し、各入力の末尾を1バイト切ると拒否した。

| 保存DAA | claim数 | 宣言数 | nativeフレーム長 |
|---|---:|---:|---:|
| 8602 | 23 | 2 | 6,530,308 B |
| 8667 | 23 | 2 | 6,530,382 B |
| 8703 | 24 | 2 | 6,540,938 B |

通常snapshotの宣言収集は、保存索引に行がない場合と読取り・復号に失敗した場合を区別する。疎な形式は存在しない宣言を省略できる。行の破損時は疎な形式・完全形式のどちらも生成を中止する。短時間fixtureで実RocksDBの宣言行を破損させ、両形式の失敗と行を取り除いた後の疎な形式の復帰を確認した。

再現は `MISAKA_S3_SNAPSHOTS` を保存世代の親ディレクトリに設定し、`cargo test --locked --offline -p kaspa-consensus --features evm --lib s3_cold_pruning_frames_preserve_awarded_and_covering_claims -- --ignored --nocapture --test-threads=1`（1件、10.77秒）。同じビルドの `pruning_snapshot_readiness_v1` は13件成功（1.37秒）。S3の試験範囲は入力の生成・事前検査まで。保存tip recordから各ピアのvirtualへの差分適用、S3由来フレームの新規Pruned Nodeへの取込と決済判定、challenge資料の取得、ディスク物理回収量はこの試験では検証していない。

### 保存S3の受賞・後続claimの保持資料（11月26日）

前節と同じ短時間試験を拡張し、DAA 8602・8667・8703の各停止世代で実際の保持資料を開いた。peer-1の受賞claimのcaptureとanswer、peer-0のforeign replayが同じjob資料を指し、prompt・生成token列が一致した。保存logitsからtrace rootを、85,938件のstep leafからMerkle rootを再計算して照合。後続claimではpeer-1のBASE0資料（5,560 leaves）のexecution・traceとanswerのoutput rootを台帳に照合した。前節と同じ再現コマンドで1件成功、11.63秒。

別途、保存済みの各peer-1 DBを `tfo3-consumed-receipt --cold-retention` で読み、両claimとも3世代でcaptureOwed=trueを確認した。受賞claimのGate Cは全世代でcertified、後続claimは順にpending、pending、certified。いずれもpruning pointはDAA 0。ローカル資料が実際に開ける範囲まで検証した。新規Pruned Nodeへの資料配送、pruning境界通過後の保持と物理容量の判定は残る。

### 第4回・全captureを持たないCourtで使う回答の照合（11月26日）

kaspadのfree-prompt区間経路では、回答のjob資料を復号できるだけで、そのtoken列を別途選んだjobと組み合わせていた。選択台帳のCourt dutyにclaimのoutput rootとjob identityを渡し、seatとCourtで共通の照合を行う。FPA1の回答またはFPC1のcaptureから得たtoken列の実行長でjob contextを構成し、class、output root、記録済みのjob pinを照合する。台帳にpinがない旧claimでもoutput rootを検査する。Courtは照合できた回答からpromptとanchorを取り、区間と対象leafを選ぶ。区間終端で全captureがない場合も必要な資料を要求できる。取得には既存の署名付き要求と25 DAAの再要求間隔を使う。

短時間fixtureで別job、同じ長さで内容が異なる回答、長さ違い、別pinを拒否し、その後の正しい早期停止回答を採用した。BASE0の実行済みcaptureでもoutput rootの一致と不一致を検査。A16の小型モデルでは回答に対応する区間資料を開き、leaf 0の反証が全capture経路と等しかった（0.18秒）。既存seatの待機検査はFP側の現在の3要素の分岐とattempt側の2要素の分岐をそれぞれ検査するよう修正。seat試験21件（12.23秒）、Court配線の検査10件（2.05秒）、別のTIRモデルを受理・Final・裁定まで歩かせる既存fixture 1件（60.00秒）が成功した。

再現は `cargo +1.93.0 test --locked --offline -p kaspad --lib served_court_answer_ -- --test-threads=1`。回帰試験のフィルターは `seat_s_tests::`、`court_responder_coverage_pin::`、`an_ir_class_walks_to_active_on_testnet_12`。A16の照合では取得済みの区間をメモリに供給した。実P2Pでの要求・再配送、同一FOW1 claimの成功Final・独立Gate C・決済、Pruned Nodeの物理容量予算はこの検証の対象外。

### 保存S3の決済をまたぐ1ブロックをPruned用snapshotから復元（11月26日）

実受賞の決済が切り替わった親と子は、ともにDAA 8703だった。親のブロックは `2c76bf28…`、子は `9c4b4324…`。親のfrontierは7866で、DAA 7867受付の報酬は未決済。子で後続claimのGate Cがpendingからcertified、frontierが8247、決済depthが0から1になった。3ピアすべてで子のヘッダーは巻き戻した親のstate rootにコミットしていた。受賞の署名、selected chain上の支払・受付、virtual UTXOの320,084,650,080 sompiを原本DBで照合し、親と子の両方で同じ受賞quantum 0が消費済みであることを確認した。

各ピアで親のPALW状態と子のdeltaだけを空の一時RocksDBに保存した。writerを閉じ、read-onlyで開き直し、tip行が存在しない状態で通常のpruning snapshot読取りとjournal順適用を実行。復元した親・子のPALW全状態がそれぞれ原本の逆適用結果・決済済みの選択状態と一致した。親のprimary carriageは6,530,741 B、deltaは24,624 B（いずれもBorsh長）。private-palw設定でflush後のSST割当は3ピアとも1,175,552 B。測定先はRAMディスクで、対象はこのsnapshot・deltaと同時に書く索引だけ。ノード全体の物理容量予算は別途測定する。

DAA 8602・8667の2世代×3ピアでは同じ診断が未決済を返して終了し、受賞の代わりに支払受付ブロックを渡すと拒否した。保存DBの各ファイル名・サイズ・mtimeは試験前後で一致。原本には書き込んでいない。UTXO集合を一時DBに転送する試験、実際のpruning pointで新規ノードが参加するP2P IBD、競合枝の選択はこのfixtureの対象外。[親子のroot、署名済みreceiptの識別子、ピア別の実測](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-cold-settlement-transition.json)

再現は次の2コマンド。`S3_SNAPSHOTS` は保存世代の親、`SCRATCH` は短時間fixture用の既存一時ディレクトリを指定する。

```sh
cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-clock --example tfo3-consumed-receipt
python3 scripts/drills/check_s3_award_transition.py "${CARGO_TARGET_DIR:-target}/debug/examples/tfo3-consumed-receipt" "$S3_SNAPSHOTS" "$SCRATCH"
```

### 保存S3の決済前後における全UTXOの照合

DAA 8703の停止DBを3ピア分読み取り専用で開き、virtualの全UTXOからMuHashを再計算した。各出力の削除・復元時にキーと値の整合を検査しながらvirtual差分と選択枝の差分を逆適用した。決済直前と直後はともに2,259行。各集合のcommitmentは対応する選択ヘッダーに一致し、前後で異なる両方の値が3ピアで一致した。保存世代8602・8667の未決済拒否6件、8703の正常3件、receipt以外の拒否1件を66.63秒で順次確認した。元DBのファイルサイズ・更新時刻も照合した。[決済前後の行数とcommitment](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-cold-settlement-transition.json)

再現は前節と同じ `scripts/drills/check_s3_award_transition.py`。今回の照合範囲は保存S3の決済ブロックとその親。未決Courtなどを含む新規Pruned IBDと、実ディスク上の継続運用予算は第8回の判定に残る。

### 第4回・Courtが裁定に使うV4 captureの選択

free-promptの通常Courtは、受け取ったcaptureの受付時にはジョブ固有の復号規則を検査していたが、裁定に使うcaptureを選ぶ段階では外側のジョブを外して既定の規則で検査し直していた。正しいV4回答でも非貪欲な選択があるとここで候補から落ちる。受付と裁定が同じ検証関数を使い、裁定側はその判定に対応する元のcaptureスライスを使うよう修正した。

FPC1を保存する際は既存Borsh形式のままジョブ、prompt、captureを出力先へ書く。読取りではpromptの束縛、非空、宣言長と残りバイト数の一致を確かめ、Court・seatが元のバッファを参照できる。保存形式とのバイト一致、末尾追加、切詰め、過大長の拒否、元バッファ内を参照することを検査した。ディスク使用量の削減率やプロセス全体のピークRAMは今回測定していない。

短時間試験はRust 1.93.0のlocked/offlineで逐次実行。kaspadのlibフィルタ `a_v4_capture_verifies_under_its_jobs_rule` でBASE0実行結果のV3・V4を検査した（1件、0.82秒）。V4 fixtureでは既定規則が同じcaptureを不一致と判定することも確認。`served_court_answer_` は2件、`court_responder_coverage_pin::` は10件、`seat_s_tests::` は21件成功（配信回答試験の一部はseat試験と重複）。`cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib fp_material_tests:: -- --test-threads=1` は5件成功。このfixtureはV4のcapture選択までを検査する。同一FOW1 claimの成功Final・独立Gate C・権利消費と、Pruned Nodeの全対象状態・容量予算は未検証。

### 第4回・FOW1の成功Finalから報酬支払への照合

隔離用MOE2決済foldでは、Courtが `ReplayVerified` を返す分岐で常に拒否していた。このため、既に独立Gate Cが成立した仕事も、その認証を読む前に停止していた。支払位置でのCourt判定を決済foldから呼び、選択親と支払直前の状態で同じFOW1認証が有効であることを要求する。登録時の署名に束縛したproducer bondと受取payloadを、coinbase出力の組立てに使用するminerと照合する。この確認を通ってから、別途、候補のmarker・価格anchor・予約・消費・出力を検査する。

独立抽選と投票を行うcore fixtureから、決済foldと同じCourt判定を呼ぶ。親で未決・支払時だけ認証済み、逆に支払時に未決、認証対象欠落、資金提供者への受取人差替え、同じbondの受取payload変更、Court承認撤回直後を検査する。成功Finalの入力はfixtureが与える。通常ノードのaward marker検査は価格復元後も `PrivateMoe2AwardUnresolved` で終了する。通常の支払計画の導出とブロック受理、当該FOW1の実支払は引き続き未達。

再現は `cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_security_certification_fold_v1:: -- --test-threads=1`（6件成功、0.08秒）。同じコマンドでフィルタを `moe2::settlement::` にした独立の決済回帰試験は4件成功、0.01秒。両試験を順次実行した。

### Gate Cの認証履歴が読めない場合のpruning境界

RFC-0018のchain claim用認証行（S）を復号できない場合、従来の境界計算はその行を読み飛ばしていた。未決認証から参照されたclaimが消失した場合も同じだった。Pruned Nodeは認証行のkeyをclaim IDから再計算し、値と照合する。未決行から参照するclaimが存在し、Gate Cの適用を受けるFinalには対応する認証行が存在することも確認する。いずれかが不明なら候補pruning pointを拒否し、理由をログに記録する。健全な状態では、未解決claimと未決認証の最古の受理blue scoreから従来の上限を求める。

`cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib pruning_progress -- --test-threads=1` は2件成功（0.27秒）。切詰め、別claimの決定済み行への差替え、未決行の参照先消失、適用対象Finalの認証行消失を検査した。正規の行を戻すと元の境界を再計算でき、決定後はその認証による保持が終了した。適用前のFinalは認証行を要求しない。

停止済みS3を `tfo3-consumed-receipt --cold-retention "$DB" "$POLICY" "$SALT" "$COVERING_CLAIM"` で読んだ。DAA 8602・8667では対象claimのGate Cはpendingで、pruning上限はどちらもblue score 16529。DAA 8703ではcertifiedとなり、上限は16840だった。この上限は台帳全体の保持義務から計算する。各世代の3ピアでblock・状態root・上限が一致し、全9 DBで実際のpruning pointはDAA 0、captureは保持対象だった。逐次照合は55.99秒。読取前後で各DB直下のファイル名・サイズ・更新時刻が一致した。新規Pruned IBDと物理ピーク容量の検証は残る。

### 第4回・FOW1の認証行から決済を受けるminerを特定

隔離用の通常ノードは、受賞候補のOriginalに対応する独立Gate Cを確認する際に、署名済みのproducerとFinalの認証行に保存したbond・支払先を照合する。決済foldは選択親と支払直前の両方で同じminerを導出し、支払計画に指定したbondとの一致を要求する。CourtはPANELの承認ブロックを保存行から復号する。承認行の切詰め、Finalに保存したbondの差替え、保存した支払先の差替えでは決済を許可しない。

通常経路で残る経済入力は、RFC-0064 §2.1が定める価格付け後のworker報酬Rと、受理時のplanに基づくPANEL配分・受取先。EXECの max_reward は予約上限であり、独立Gate Cで保持する額は責任額である。隔離PANELの draw は ML-DSA-65 の actor ID を記録し、決済出力には native bond が必要となる。受賞候補の最終受理と実支払は未達。

再現は `cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_security_certification_fold_v1:: -- --test-threads=1`（6件成功、0.08秒）と `cargo +1.93.0 build --locked --offline -p kaspa-consensus --lib`（成功）。両方ともRAM上でconsensusとconsensus-coreのdev最適化を0として逐次実行した。

### 第4回・PANELの抽選席から支払bondを復元する

隔離用EXEC closeの補助取引 `TB1PAY01` に、PANELのML-DSA-65鍵とnative bondのML-DSA-87鍵が同じchain、EXEC freeze、actor、outpoint、支払先に署名した指示を載せる。受賞候補の読取りでは、選択親に保存されたcloseとヘッダーを照合し、本文の取引Merkle rootを再計算する。登録DAAがcloseのDAAより大きいbondや重複したactor・bondは受け付けない。PANELの当選やGate C認証は、それぞれ既存の抽選・承認処理が決める。

追加モデルの20% PANEL planについて、通常ノードは提示された指示から、承認済みの席を抽選順にnative bondへ対応付ける。隔離用決済foldは正のPANEL即時支払を行う場合、選択親と支払直前の両方で全席のbond、順序、均等な即時額が支払計画と一致することを要求する。席割り後の端数とvestingを控除した即時総額の導出は別に必要となる。従来の署名指示がないcloseから正のPANEL支払を導出することはできない。

現在はclose本文から支払指示を読むため、本文回収後のPruned Nodeでの復元は未検証。PANEL署名指示を含む通常の採掘・EXEC close、同一claimの価格付け後worker報酬、PANEL即時総額、実coinbase支払の一連の検証も残る。

Rust 1.93.0、locked/offline、RAM上で逐次検証した。`cargo test -p kaspa-consensus-core --lib palw_security_certification_fold_v1:: -- --test-threads=1` は7件成功（0.19秒）。2つの実署名と2席のfixtureで、異なるfreeze、各署名の破損、本文差替え、重複、承認欠落、minerとの重複、支払先変更を拒否した。2席の抽選順に100を50ずつ割り当て、即時総額99を拒否した。`moe2::settlement::` は4件成功（0.01秒）、`cargo build -p kaspa-consensus --lib` も成功した。

### 第4回・署名済み決済期限と認証済みFinalの責任期間

受賞候補の検査で、独立Gate Cが認証したFinalのDAAとCourt窓から、署名済みquoteが最低限カバーすべき責任期限を計算する。決済foldは選択親と支払直前の両方からminerと期限を導出し、短すぎる責任期間を予算引落し前に拒否する。追加モデルの市場への正の支出と、vesting対象のworkerへの即時支払は、指定された責任期限の次のDAA以降に限る。G15対象のOriginalは受理ヘッダーのhashとDAAを検査し、Final DAA＋Court窓に達した時点でも正の報酬を保留する。

境界fixtureではFinalはDAA 12、独立認証は21、Court窓は500。責任期限の最小値511とG15の保留最終値512を個別に検査する。登録fixtureのslot 10はslot 9と同じOriginal締切で、より長い経済期限にownerとfunderが抽選前に署名する。保存S3のquoteや予約は変更しない。実際の支払には価格付け後のworker報酬、即時額、settled anchorの成熟、受賞ブロックの受理と権利消費の検証が残る。

Rust 1.93.0、locked/offline、RAM上の逐次検証。consensus-coreのフィルタ `palw_security_certification_fold_v1::` は7件、`palw_settlement_liability_v1::` は4件、`moe2::settlement::` は4件成功。通常の `cargo build -p kaspa-consensus --lib` も成功した。登録tokenizerと `MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1` で `cargo test -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --nocapture --test-threads=1` が1件成功（23.62秒）。署名済みslot 10からOriginal締切61、経済期限10142を復号した。ReplayVerifiedのOriginalは独立FOW1認証を欠くため支払を拒否した。

### 第4回・FOW1 Final成立後に確定したanchorの計数

隔離FOW1の成功Finalで、数値workの帰属行にその時点のsettled attempt finalsカウンタを記録する。既存行への追加は8バイトで、値は72バイトとなる。読取りとPruned用のwire検査は従来の64バイトの帰属記録も扱う。旧行からFinal成立時点の件数を復元できないため、anchor数が必要な支払は保留する。決済foldが追加モデルの市場への支出またはvesting対象のworker即時支払を評価するとき、選択親と支払直前の両方で既存の二つの時計によるロック判定を使う。DAA到達後も規定のanchor数に達していない場合と、licence停止中は支払を保留する。

この検査は支払候補時点の成熟を評価する。成熟の初回成立をラッチする処理、通常の採掘候補からの成功Final生成、FOW1専用のGate C担当発見・独立再実行、価格付け後の実支払が残る。既存のノード用 `palw_certification_duties_v1` が返す対象はchain claimとowning39 mining claimである。今回の成功Finalと投票はcore fixtureから渡した。

再現はRust 1.93.0で `cargo test --locked --offline -p kaspa-consensus-core --lib FILTER -- --test-threads=1`。RAM上で `palw_security_certification_fold_v1::` は7件、`vesting_fold_v1::` は25件、`moe2::settlement::` は4件成功した。Final時1件・必要な後続anchorが2件のケースで、1件追加時は保留し、2件追加時は成熟し、その状態からlicenceが停止すると未ラッチの支払は保留した。Finalのdelta巻戻しで件数が消え、再適用で件数1と同一rootに戻った。wire検査は64・72バイトを受理し、63・71バイトを拒否した。`cargo build --locked --offline -p kaspa-consensus --lib` も成功。新規Pruned IBDと実支払は未検証。

### 第4回・独立Gate C検証者によるFOW1担当の読取りと再実行

隔離用ノードのConsensusApiに、指定した選択sinkとbondの `fow1_certification_duties_v1` を接続した。成功Finalの認証行から、現役bondに割り当てられた未投票の抽選席数、exam、元のOriginalと収録ブロック、投票期限を返す。PANEL担当と独立認証の担当は異なる抽選で決まる。読取りは期限に到達した時点で新たな仕事の開始を止める。既存の合意側の投票受理期限は従来どおり。

private remoteの `BranchExecInput::replay_fow1_certification` は、native EXEC証明書、指定examの担当、署名済みOriginalと選択履歴を照合し、登録済みモデルで入力、回答、Readout、traceを再計算する。再実行結果の全件数・根・Ticket、署名済み目標、選択sink、pruning import世代、未投票の担当を確認した場合にだけ、既存のGate C署名対象を返す。PANELの返信署名を転用する処理はない。通常サービスでの継続的な担当取得・署名・配送、通常の採掘Finalから認証・支払までの実証は残る。

Rust 1.93.0、locked/offline、RAM上で逐次検証。前節と同じcoreフィルタ `palw_security_certification_fold_v1::` は7件成功（0.20秒）。fixtureの成功Finalと独立抽選から、当選bondのexam、Original収録ブロック、重複抽選の席数を照合し、未抽選・非当選・期限到達・投票済みを除外した。登録tokenizer、`MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1` で `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --nocapture --test-threads=1` が1件成功（23.56秒）。実ノードは異なるsinkとPANELのdutyを拒否し、通常のPANEL再実行を完了した。このOriginalには独立認証がなく、Gate Cの肯定票生成・送信・決済はこの試験では未検証。

### 第4回・EXEC本文回収後のFOW1入力復元

通常ノードは指定された選択tipで同じEXEC closeに帰属する受理記録と証明書を読み、両方のfreeze IDを照合する。private remoteは、検査済みの登録batchからround・資格・consumer set・policyを復元し、closeヘッダーのhash、実際の選択親に対するfreeze commitment、証明書の選択履歴、個々の登録anchorと期限を照合する。入力のattemptとtokenは照合済みのseedから導出する。carrierの上限は署名・contextを含む正確なwire長で検査する。署名済みbatchを既に保持している場合は、PANEL closeが一時的なEXEC rootsを消費した後にも、追加の台帳行を保存せず復元できる。

この経路には署名済みbatch、closeヘッダー、受理記録・証明書と選択履歴が必要。新規Pruned Nodeによるbatchの取得、Originalの初回受付、独立Gate C肯定票の配送、同一権利の実支払は別途検証する。

Rust 1.93.0、RAM上で前節の `private_registered_tb1_pending_freeze` を逐次実行。追加設定 `MISAKA_PRIVATE_TB1_RETAIN_EXEC_ROOTS=1` で1件成功（23.49秒）。PANEL close後、EXEC本文と一時root行がない状態で、29,748 Bの上限から元の登録key・attemptを復元し、29,747 Bと古いtipの読取りを拒否した。同じDBのcheckpointをcold reopenし、tip・状態root、復元したattempt・token列が一致した。追加設定を外した既存genesisでも1件成功（23.53秒）。29,748 Bは署名済みcarrierのwire長。物理ディスク回収量は未計測。

### 第4回・未決済EXEC予約の価格状態を保持するpruning境界

通常のpruning point更新時、隔離経済ルールの選択台帳でopenのEXEC予約を読み、各closeの準備ブロックの選択親（価格anchor）を解決する。候補のblue scoreは、最も古い価格anchorの一つ前までに制限する。同一closeの複数slotは履歴を一度だけ読む。closeの受理記録がない履歴では保存本文から予約keyに対応する登録を読み、準備ブロックとcloseの選択履歴を照合する。読取り失敗と、参照を持たないopenの旧形式予約は境界更新を保留する。

予約の終了または期限処理によりopenの予約がなくなると、その予約による上限が外れる。Court・独立認証・safe frontierによる上限は引き続き個別に評価する。追加の永続行はない。

Rust 1.93.0、locked/offline、RAM上で逐次検証。`palw_security_certification_fold_v1::` は7件成功（0.21秒）。3予約・2 closeのfixtureは履歴を2回読み、先に閉じた予約の上限を外し、残る予約はDAA 29で保持、30で解除した。参照不能の履歴も拒否した。前節の `private_registered_tb1_pending_freeze` は1件成功（23.77秒）。EXEC本文がないDBのcold reopen後に価格anchorのblue score 3を復元し、closeより前のtipを拒否した。このノードのQF1登録には有償予約がない。`cargo test --locked --offline -p kaspa-consensus --features evm --lib palw_v2_the_pruning_ceiling_fails_closed_on_a_tip_it_cannot_read -- --test-threads=1` は1件成功（0.30秒）。有償予約を持つPruned IBDでの実際の境界更新、物理容量ピーク、予約解除後の回収量は未検証。

### 第4回・再実行と受賞判定が参照する保存本文の照合

受賞判定は受理済みOriginalとEXEC closeのhashを選択台帳から取り、保存ヘッダーを再ハッシュし、取引列からMerkle rootを再計算する。保存本文の不一致はノードの履歴読取り障害として扱う。候補の開示値が登録内容と異なる場合は、従来どおり候補を拒否する。PANEL・Gate Cの再実行で読むOriginalと、private remoteがCourtの期限窓で読むcoinbase・裁定本文にも同じ検査を適用した。Merkle計算用メモリは既存のnative予算で予約する。永続行の追加はない。

RAM上でRust 1.93.0の `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --nocapture --test-threads=1` を逐次実行した。登録tokenizer、native certificate、producer consentを指定し、Court expiryを `clear`、`MISAKA_PRIVATE_TB1_RETAIN_EXEC_ROOTS=1` としたケースは1件成功（23.53秒）。正規の数値work keyを復元し、同じヘッダー・署名のまま先頭2取引を交換したOriginalとEXEC closeの双方で履歴不一致を検出した。CourtはReplayVerified、独立認証は未成立。Court expiryを `case` に変更してretain設定を外したケースは1件成功（18.80秒）。未決事件の期限時はUnverifiedだった。

11月26日の再計画後の実装セッションを1本消化した。第4回の終了条件は未達。現行の隔離登録は3種とも有償の採掘予約を許さず、通常受賞側でも価格付け済みworker報酬が確定していない。TB1自然成熟・通常配送の検証と、FOW1の適格な登録から独立認証・価格付け・権利消費までの接続に追加2本を見込む。その後のQF1、MAD2、MOE2、全状態と容量の照合は各1本。再計画起点から全対象判定は通算7本目（残り6本）、合格後の総合試験は8本目（残り7本）の見積り。次のTB1試験は対象genesis、検証者年齢、pruning point、release残存期間を保存履歴から確認して選ぶ。

### 再計画後2本目・保存claimを通常TB1 holder付きで再開する入口

4ピアのclaim再開スクリプトはproducerとpanelを起動していたが、holderの起動引数を渡していなかった。`scripts/drills/tb1_resume_claim.py --with-holders` は登録時の4組のVSS経路と元のcustodyを使用する。genesis・rules・元のEXEC checkpoint、closeの状態root・verifier cut、保存時計を照合する。各holderの既存のEXEC/PANEL journalを確認し、起動時に各ノードで同じEXEC closeが選択枝にあることをヘッダーで検査する。共通tipごとにcloseのDAAで4台のsettlementを照合し、pruning pointと一緒に記録する。開始前からのreleaseファイルと新規ファイル名を分けて記録する。ファイル名だけではshareやcertificateの受理は判定できない。

既存の `scheduled-native-close/close.borsh` を `tb1-ready-fixture --read-verifier-cut` で読み直した。EXEC closeはDAA 8、年齢条件を満たせる最初のDAAは5040、期限は200000（exclusive）。保存した実claim枝の最終観測はDAA 43。4つの登録済みloopbackポートを順に確保・解放でき、journalは各2本、releaseファイルは各0件。異なるgenesis・rules・開始checkpoint・不一致のVSS経路を拒否した。空き6.9 GiBの実媒体で `--with-holders` の再開入口は作業ディレクトリ作成前に容量下限で停止した。ノード起動、自然成熟、新規配送の観測は未実施。

RAMで `cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_certifies_pruned_panel_from_native_ready -- --test-threads=1` が1件成功（26.41秒）。この短時間試験は成熟仕事量をfixtureから供給する。実成熟の再開コマンドは第3回の `tb1_resume_claim.py` に `--with-holders` を付ける。共通tip・settlement・releaseファイルを観測する150秒の枠が満了しても、自然成熟の合格を意味しない。

再計画後の消化は2本。通算7本目の全対象判定まで残り5本の計画枠がある。自然成熟の実証は、同一genesisの成熟履歴と物理容量下限が揃った時点に持ち越す。この依存が未解決の間は通算7本目に全件合格できるか確定できない。

### 再計画後3本目・通常Gate Cのattempt再実行と枝の変更

通常ノードのGate Cワーカーは、再実行終了時にnative routingのclaim・import世代・宣言を読み直していたが、その照合に失敗しても計算結果で投票を判定していた。照合失敗時はUnjudgedに戻し、期限に余裕があれば30 DAA後に再実行できる。終了済みのworker結果を再試行時の同じclaim・jobに流用しない。実行中に抽選行が置換された場合はその事実を完了まで覚え、A→B→Aでも元のworker結果を捨てる。通常の期限直前のUnavailable判定は既存規則に従う。

Rust 1.93.0で `cargo test --locked --offline -p kaspad --features private-integration-clock --lib palw_cert_duty::tests:: -- --test-threads=1` をRAM上で実行し、8件成功（0.99秒）。参照先の照合失敗、計算結果の破棄、再試行の境界、新しく照合できた結果の採用、同一DAAの抽選行往復を検査した。これはchain claimの通常Gate Cワーカーの検証。FOW1専用drawの常駐取得・投票配送、採掘適格な登録からの成功Final・価格付け・権利消費は未達。

Issue #129の単独commit `33db510de` のパッチを配布用に出力し、一時Git indexで基点 `f839ef0dc` に適用した。変更後のtreeは `6a2dbaa69ee820e663123ce17cbaaa61fdfd9a3c` で単独commitと一致した。変更はTB1 custodyとその回帰試験の2ファイル。再計画後は3セッション消化。通算7本目まで4枠が残るが、自然成熟・有償の成功Final・全容量予算の合格は確定していない。

### 再計画後4本目・保存S3のWAL増加とCourt行のクラッシュ復旧

DAA 8602・8667・8703の停止世代を既存の `palw_devnet_storage.measure_tree` で再計測した。DAA 8667→8703で3ピアの通常ファイルの割当は合計1,165,443,072 B増えた（各終点の差は32,373,419 B/DAA）。WALが738,643,968 B、consensus SSTが424,648,704 B増え、blobは15,431,081,984 Bで一定だった。先行する8602→8667ではWALが114,589,696 B減り、通常ファイル全体も58,830,848 B減った。36 DAAの増加を長期増加率には外挿できない。各世代の割当は共有extentを含み得る。[3世代のファイル種別と区間差](../evidence/palw/pruned-ibd-2026-11-25/s3-closed-peer-allocation.json)

RocksDBのnative Zstandard WAL圧縮を容量制限付き・default presetのPALW V2 Pruned consensus DBと隔離用private-palw presetに接続した。DAA 8602の保存Court delta 2行を同じWriteBatchに入れ、RAM上の新規DBに書き、WAL同期直後に子プロセスをDBの終了処理を通さず終了させた。各DBにSSTとblobがないことを確認。非圧縮WALは容量制限付きPruned設定で、圧縮WALはデフォルト設定で再起動し、全key・valueが元データに一致した。

| 新規WALの設定 | 論理長（B） | 割当（B） |
|---|---:|---:|
| 圧縮なし | 24,835,492 | 24,838,144 |
| 容量制限付きPruned | 9,287,079 | 9,289,728 |
| private-palw | 9,287,079 | 9,289,728 |

2行のfixtureでWAL割当を62.6%削減。16行の既存fixtureはbottommost SST割当152,129,536 B→22,609,920 B、旧blobのcompaction後はDB割当71,872,512 B→22,765,568 Bで再読取り一致。前節までの第8回で必要な全ノードのピークと継続的な物理増加量、混合負荷でのCPUと書込みレイテンシは未計測。

DAA 8602の閉鎖peer-0 consensus DBを `MISAKA_S3_COLD_CONSENSUS` に指定し、RAM上で `cargo +1.93.0 test --locked --offline -p kaspa-database --test private_palw_journal -- --ignored --nocapture --test-threads=1` が3件成功（56.87秒）。クラッシュ復旧の単独再実行も成功（4.34秒）、`--lib storage_bounds::tests::` は2件成功。保存元の164ファイルのSHA-256を固定manifestと照合した。再計画後4本を消化。通算7本目まで3枠が残り、5経路の通常接続・全状態と容量の合否は未確定。

### 再計画後5本目からの実施順序（11月26日）

通常5経路とPruned全状態・容量の合格目標を、今回終了後 **残り7セッション、再計画起点から通算12本目** に更新する。従来の通算7本目という見積りは、TB1の自然成熟とFOW1の有償登録からの独立認証・決済が持ち越され、残り2枠では各経路の接続と容量実測を収められないため改める。前掲「回・担当範囲・終了条件」表の全項目を引き継ぎ、Codex単独で次の順に進める。

| 通算 | 通常入口からの完了条件 |
|---|---|
| 6 | TB1。適合する保存履歴で成熟したEXEC/PANELからholderのshare配送、再起動と枝変更後のrelease・停止理由を照合する。 |
| 7 | FOW1。採掘適格な署名済み登録からOriginal、独立Gate Cの担当取得・投票、成功Final、価格付け、支払と一回消費を同じ仕事で追跡する。 |
| 8 | QF1。隔離設定の通常受付から反証期限までの資料供給、回収、reorg時の保持・復元を検査する。公開REVIEW_HOLDを維持する。 |
| 9 | MAD2。登録、採用票、burn停止、権限者による解除を選択台帳で追い、再起動・reorg後に世代と残高を照合する。 |
| 10 | MOE2。別作業の差分を照合し、通常決済でnative/EVMの両側に同じ権利を一度だけ反映し、失敗・二重消費・巻戻しを検証する。 |
| 11 | 新規Pruned Nodeを正規P2P IBDで同一tipへ同期し、PALW root、全UTXO、未決claim・Court資料・Gate C・Ticket・消費済み権利・未決済報酬・担保・モデル・市場・native/EVM残高と5経路固有の行をArchivalと比較する。pruning境界の前後、reorg・再起動でも照合する。 |
| 12 | 実媒体で初期同期、定常時、pruning・compaction、snapshot世代更新の割当ピーク、B/DAA、未決claim当たりの量、回収量を記録する。既定の各台30 GB・2台と作業領域計60 GB・空き下限12 GiBで判定する。 |
| 13 | 通算12本目までの全項目合格後に、保存S3実受賞を含む最終総合試験を実施する。公開・VPS配備は別途承認。 |

各セッション終了時にcommit、実行コマンド、合否、残り本数を記録する。通常入口の成功をfixtureで代用した項目、実媒体不足、同一genesisの成熟履歴不足は未達として明記し、その時点で追加工数を再見積りする。12本目は到達目標であり、媒体空きや成熟履歴が揃う日は未確定。

今回の検証では、停止済みS3各ピアのvirtual UTXO全行から選択枝を逆算し、各ヘッダのUTXO commitmentを確認した。決済直前のPALW snapshotとUTXO全行、決済ブロックのPALW・UTXO差分を同一WriteBatchで空のDBに保存し、flush・closeした。読み取り専用で開き直して親の全行を照合し、保存した差分を適用して子の全行とcommitment、PALW状態とrootを照合した。親・子とも2,259 UTXO、3ピア一致。DAA 8602・8667の全6読取りは未決済として拒否、非receiptも拒否した。元DB全ファイルの長さと更新時刻は試験前後で一致した。

Rust 1.93.0のlocked/offlineビルド（kaspad、private-integration-clock、example tfo3-consumed-receipt）後、既存の `scripts/drills/check_s3_award_transition.py` にビルドしたバイナリ、停止世代ディレクトリ、RAM上の一時領域を順に渡して成功。各一時DBのSST割当は1,363,968 B。対象はDAA 8703の実決済1遷移。新規ノードのP2P同期と実媒体ピークは通算11・12本目で検証する。[3ピアの決済・UTXO照合結果](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-cold-settlement-transition.json)

### 通算6回目・TB1 holder再起動後の同一release配送

通常holderは再開時に同じshareへ新たな乱数で署名する。従来はそのバイト列で既存のoutboxファイルを置換しており、受取側が既に組み立てたバッチと再配送後のバッチでcommitmentが変わり得た。常駐holderの配送時に、round、slot、登録鍵による署名を検査し、既存ファイルがある場合は署名対象全体の一致を確認して最初のファイルを保持する。既存ファイルが破損している場合、または異なるfreeze・shareを含む場合は配送を失敗として返す。custodyのbranch観測、停止マーカー、期限と署名資格の判定は従来の入口を通る。

RAM上、Rust 1.93.0で `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_tb1_certifies_pruned_panel_from_native_ready -- --test-threads=1` が1件成功（26.91秒）。EXEC・PANELそれぞれの3 holderでjournalを閉じて開き直し、再署名された異なるpacketの配送後も元の全バイトが一致した。各roundのcertificateとseedをnative stateから確認した。この試験では各roundの成熟仕事量をfixtureが供給し、holderの再開は同一プロセス内で行った。

保存した同一genesisの観測を再確認した。scheduled-native-closeのEXEC closeはDAA 8、登録bondの最初の年齢適格DAAは5040。2つの試験ボリューム内の同じgenesisのresult.jsonで、観測配列に残る最大DAAは43。ホスト空きは6.9 GiB。通常4ノードの自然成熟と新規share配送は未達。第7回のFOW1から第12回の容量判定まで当初割当は6枠。TB1の自然成熟を同一試験内で実測できない場合、条件成立後に専用1枠を要し、残り7枠・通算13回目の全対象判定、14回目の総合試験を見込む。

custody回帰試験の初回はRAMディスクでarchive作成中にENOSPCとなった。コンパイル出力だけをホストの一時ディレクトリに移し、試験用DBはRAM上に置いた。`cargo test --locked --offline -p misaka-palw-remote --lib tb1_exposure:: -- --test-threads=1` は15件成功（4.42秒）。異なるfreeze、異なるslot・round、既存署名破損を拒否し、既存ファイルの全バイトを保持した。Issue #129の分類、停止マーカー障害、A→B→A、子プロセスを用いる再開もこの試験群で再確認した。停止マーカーが一切永続化できなかった場合の再起動後の隔離は、引き続き独立した永続記録に依存する。

常駐holderの呼出箇所を含む `cargo build --locked --offline -p kaspad --features private-integration-clock --lib` も成功した。検証終了後に今回作成したホスト上の一時ビルド領域を解放した。

### 通算7回目・FOW1独立検証者の署名とclaim単位の不一致票

private-tb1-tirの `ExecInputBatch::signed_fow1_certification` は、検証鍵から選択台帳のactive bondを引き、指定examに対する独立Gate Cの担当を取得する。担当のOriginal収録ブロックを認証し、その取引のclose・slotからnative EXEC certificateに対応する入力を復元する。署名済みOriginalと登録instanceを照合して完全再実行し、ML-DSA-87で通常のinner 75に署名する。署名返却前にbond、選択tip、import世代、担当を再確認する。担当の比較にはdrawのanchor spanとexecution keyを含めた。

完全再実行できた場合、4つのroot・件数・Ticketのいずれかが主張と異なれば、署名対象のclaim_mismatchesに対象index 0を入れる。合意foldでは不一致席となり、肯定票の担保をロックしない。資料や登録instanceの読取りに失敗した場合は署名を生成できない。通常PANELの承認には従来どおり全項目の一致が必要。

Rust 1.93.0、locked/offline、devのkaspa-consensusとkaspa-consensus-coreの最適化0、RUST_MIN_STACK=67108864で逐次実行。

- `cargo test -p kaspa-consensus-core --features evm --lib fow1_committed_final -- --test-threads=1` は1件成功（0.08秒）。既存のFinal fixtureから独立drawのanchor、正票と不一致票の担保・席数の違い、重複拒否を検査した。
- `cargo test -p misaka-palw-remote --features private-tb1-tir,kaspa-consensus-core/evm --lib signed_fow1_ballots -- --test-threads=1` は1件成功（0.08秒）。一致・不一致の完了結果を与えて署名・検証し、別networkでの検証を拒否した。通常のlifecycle取引を組み立て、payloadから同じ署名済みobjectを取り出した。fee入力の署名・mempool送信はこの試験の対象外。
- 登録済みtokenizer（SHA-256 `aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4`）を `MISAKA_PRIVATE_QF1_TOKENIZER` に指定し、`MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_NATIVE_REPLAY=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1` で `cargo test -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --test-threads=1` が1件成功（23.47秒）。native bondの検索、旧sinkとPANELのdrawを使った独立署名の拒否、PANEL側の通常再実行、期限61のReplayVerified、そのOriginalで独立認証が欠けるため支払を拒否したことを確認した。

通算7回目のFOW1完了条件は持ち越す。常駐 `cert_duty_tick_v1` からFOW1専用drawに対応する登録instanceを取得し、署名objectの手数料を支払って配送する部分が未接続。`into_mining_reservation` の現行3登録は有償予約を作れず、実仕事から成功Final、独立draw、価格付け、権利消費までを通した証跡は得られていない。次は第8回のQF1。第12回判定まで当初配分は5枠で、TB1自然成熟とFOW1有償経路がそれぞれ専用枠を要すれば残り7枠・通算14回目が全対象判定の見積り。最終総合試験は全項目合格後に行う。

### 第8回時点の残り枠を固定する

第8回終了から **残り6セッション、通算14回目に全対象を判定** する計画とする。第6・7回で持ち越した実証を全状態比較の前に配置する。第15回は全対象合格を条件とする最終総合試験。各回をCodex単独で順番に実施し、証跡を得た項目だけを合格にする。

| 通算 | 終了時に照合する証跡 |
|---|---|
| 9 | MAD2の通常登録・採用票・burn停止と解除、再起動・reorg後の世代と残高 |
| 10 | MOE2の通常決済、native/EVM双方の一回消費、失敗・巻戻し。別作業のMOE2修正との重複を先に照合 |
| 11 | TB1の同一genesisでの自然成熟、通常holder配送、再起動と枝変更時のrelease |
| 12 | FOW1の有償適格登録、常駐検証者による独立Gate C票の手数料付き配送、同じ仕事の成功Final・価格付け・支払・権利消費 |
| 13 | 新規Pruned NodeのP2P IBD。第11回として前掲した全対象を同一tipのArchivalと比較し、pruning境界、再起動、reorg後も照合。QF1の全所有者の期限と資料回収を含む |
| 14 | 実媒体で初期同期・定常・compaction・snapshot世代交代のピークと増加率を測定し、各台30 GB、2台と作業領域計60 GB、空き下限12 GiBを判定 |
| 15 | 1〜14の終了条件が揃った場合のみ、保存S3実受賞を使う総合試験 |

第9回終了時点から数えると、全通常経路の接続に3回（第10〜12回）、Pruned Nodeの全状態一致までに4回、実媒体の容量判定までに5回を割り当てる。以下の持越しも各回の終了条件に含める。

- 第10回でMOE2のnative会計とEVMで読める市場残高を照合し、第12回でFOW1の価格付け後worker報酬を使って通常の受賞ブロックを受理する。登録上限を実支払額に転用しない。
- 第11回はTB1の自然成熟と配送に加えてQF1の未接続の通知・資料経路を、第12回はMAD2のkernel自動採用・fork指定とclaim効果判定を閉じる。第12回の合否は5機能すべての通常入口・受理・台帳効果を確認して決める。
- 第12回までにTFO3の保存S3実受賞から未決済分の受取先・金額・消費済み権利も照合する。第13回の比較では同一tipの状態ごとの件数とcommitmentを残す。第14回で物理使用量を採れるかは事前に確認する。第10回開始時の作業ボリュームの空きは4.6 GiBで、空き下限12 GiBを既に下回る。この媒体で容量負荷を開始しない。

同一genesisの成熟履歴と容量試験用媒体が条件になる。欠けている場合は該当行に観測値と未達理由を残し、当初の割当と実績を比較できるようにする。

### 通算8回目・QF1の資料要求と応答

隔離設定の標準採掘テンプレートが、選択PANELの署名済み資料要求と、要求対象のMerkle openingを含む応答からcoinbase markerを導出する。要求時はOriginalのヘッダ・取引Merkle rootを照合する。応答は保存済みの要求、期限、index、件数、rootを使う。採掘時と受入時に同じ署名・openingの検査を使い、受入時はcoinbaseの値も照合する。

第7回と同じRust・tokenizer・stack設定で、以下を逐次実行した。各実行前に `MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1` を設定した。

```sh
for mode in notice material; do
  MISAKA_PRIVATE_TB1_COURT_EXPIRY="$mode" cargo +1.93.0 \
    --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
    --config 'profile.dev.package.kaspa-consensus.opt-level=0' \
    test --locked --offline -p kaspa-consensus --features evm --lib \
    private_registered_tb1_pending_freeze -- --ignored --test-threads=1 || exit 1
done
```

- noticeは1件成功（18.68秒）。手動markerを与えずに要求・応答を採掘し、不正なopeningにmarkerが付かないことを確認した。要求直後に再オープンしたRocksDBチェックポイントからOriginal本文を取り除いても、tipとPALW rootが一致し、正しい応答だけが通った。応答のdeltaを戻すと要求待ちのrootと保留が復元された。続く不一致通知が未解決のため、DAA 61でUnverifiedになった。
- materialは1件成功（18.23秒）。応答を出さずに同じ期限を迎えるとUnverifiedになった。

資料要求と応答は短時間の隔離コンセンサス経路で確認した。不一致通知のfixtureは従来の手動markerを使う。delta巻戻しとチェックポイント上の本文除去までを検証した。実ノードの枝切替、全所有者の期限後に起きる自動回収、実媒体で解放された物理バイト数は未検証であり、第13・14回の合否に含める。

### 第9回・MAD2の開票本文回収後の投票と採否の再検証

台帳を持つ隔離profileでは、投票者が指定tipとnetworkから未決提案のmarkerを読み、同じ選択状態のauthority・期限と照合する。本文回収後も登録済みモデルの契約を再検査して署名できる。既決案件・旧tipでは新規署名を返さない。署名付き定足数の再検証は、再計算したmarkerと採否・開票anchorを選択親にある採否行と照合する。履歴確認のための開票読取りと、未決であることを必要とする署名を区別した。台帳がない旧profileは選択履歴とcoinbaseの検証を使う。

第8回と同じtokenizer、Rust 1.93.0、locked/offline、evm、dev最適化設定、stack設定で以下を逐次実行した。

- `MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=1 cargo test -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --test-threads=1` は1件成功（10.73秒）。RocksDBチェックポイント上のauthority・開票本文を除去し、登録済みinstanceへのYES署名を検証した。別契約・反対票を拒否した。採否確定後は未投票席の署名も拒否した。別チェックポイントではauthority・開票・採否本文の3つを除去し、保存された決定と署名群から正票を再検証した。
- `cargo test -p kaspa-consensus --features evm --lib mad2_reads_yes_and_no_from_separate_native_proposals -- --test-threads=1` は1件成功（0.55秒）。旧profileでYES・NOの読取り、重い実分岐への切替と置換された開票への競合署名の拒否を確認した。

第9回の終了条件のうち、burn停止・解除の原子的確定は未達。短時間試験では同じお釣りを使う2件の支払と、10・20 sompiを引く事前計算までを実行した。次の実装箇所は `calculate_utxo_state_with_funded_candidates_v1` のRound収入予約・最終徴収額と、同じaccepted transactionの選択台帳更新である。両者の同時確定と枝変更後の世代・残高照合を必要とする。第14回の全対象判定の計画は維持し、この未達項目を継続する。

### 第9回の継続・MAD2 burnの徴収、停止世代、A→B→A

専用の隔離genesis `MD2BURN` に、停止10・解除20 sompi、accepting mergesetあたり成功操作16件までの試験規則を割り当てた。これらの値は公開用パラメーターの採択を要する。既存のgenesisは従来の手数料を徴収する。

通常のUTXO検証で、実入力、手数料の先行控除、登録者署名、元の収録ブロックと位置、直前の操作、世代、期限を照合する。受理されるbodyだけが、Roundの収入予約と最終徴収額に同じburnを適用する。選択台帳にはclassごとの最新receiptを記録し、巻戻しに必要な旧値は既存のdeltaに保存する。同じ停止取引が別の収録位置で採用されると操作IDが変わり、元の操作を参照した解除は拒否される。追加モデルのEXEC採掘意図は準備時点とclose選択親の両方で停止行を照合する。

第8回のRust・tokenizer設定を使用し、`MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear` と以下の値で `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --test-threads=1` を逐次実行した。

- `MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=burn` は4回連続成功（8.09、8.02、8.04、8.12秒）。登録、YES/NOの世代履歴、同一のお釣りで支払った停止・解除、実際の徴収額、RocksDB再オープン後のPALW rootと操作IDを照合した。
- 選択枝Aで解除世代2、競合枝Bで旧前操作による解除を拒否して停止世代1、Aの再採用で再び同じ操作IDの世代2となった。Bの競合取引は通常手数料を支払い、AとBの各切替で元枝のお釣りが消え、正規枝のお釣りの額と支払先が一致した。
- `MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=1` は1件成功（10.73秒）。以前の隔離genesisでは両取引とも全手数料を徴収した。
- 初回の実分岐試験ではfinality depthが1で、2ブロック前に分かれた競合枝が選択されなかった。burn専用fixtureの深さを8にして確定前に分岐を届けた。公開ネットワークのfork choice・finality設定は変更していない。

第9回に割り当てた登録・採用票・burn・世代・残高の短時間照合を満たした。MAD2の公開完成にはkernel自動採用、BFT fork指定の原子的な経済処理、各claim効果直前の停止照合などが残る。今回の試験は全claim効果の網羅、Pruned P2P IBD、実媒体予算を対象にしていない。

同じ設定で `cargo test --locked --offline -p kaspa-consensus-core --features evm --lib model_pause_v1_ -- --test-threads=1` も2件成功（0.06秒）。登録範囲、catalog指定モデルの免除、解除権限、古い世代の拒否を確認した。

### 第10回・保存したMOE2会計状態からのEVM市場読取り

登録・予約・会計を扱う3プロセスfixtureに、同じmodel IDでAMMの `market(bytes32,bytes32)` を実行する回帰検査を追加した。決済前・決済後、carriage再読取り、delta巻戻し、再適用の5点で、real reserve、curve在庫、売却済み数量、買戻し累計、retired数量をnative行と比較する。復元・巻戻し時にはABI応答全体も比較する。

隔離チェーンの対象DAAではmodel EVMが休止していた。通常設定で呼ぶと成功・出力0バイトになる。読取りシミュレーションでだけmodel EVMを有効にし、同じ状態に対するABI値を検査する。仕事IDとworker releaseは既存の会計fixtureから渡している。通常の受賞候補は `utxo_validation.rs` で価格付け後の報酬とvesting条件を確定するまで `PrivateMoe2AwardUnresolved`。実受賞からの支払と一回消費の合格は第12回のFOW1接続に依存する。

再現コマンド（第9回と同じRust・RocksDB・stack設定）。

```sh
cargo +1.93.0 --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.dev.package.kaspa-consensus.opt-level=0' test --locked --offline \
  -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --test-threads=1
```

親プロセスの試験は1件成功（3.42秒）、登録履歴を再生する2ピアの試験は各1.40秒で成功。通常設定の出力に352バイトを期待した途中版は0バイトで失敗したため、上記の休止時と有効時の両方を検査した。第10回終了時点で第14回まで残り4枠。通常決済、全対象IBD一致、物理容量予算は未達。

### 第11回・QF1のPANEL承認と不一致通知のテンプレート

隔離profileのテンプレートは、選択親に保存したPANEL抽選からOriginal、network、対象の席、期限を照合する。承認では抽選順の全席の署名、不一致通知では選択席の署名、対象relationと観測digestを確認してcoinbase markerを生成する。ブロック受理時には同じ署名検査に加えて、実際のcoinbaseが計算値をcommitしていることを要求する。不一致通知はCourtの判定まで支払を保留する。

TB1の自然成熟に使える履歴を再調査した。既存の2試験ボリューム、S3保存領域、MISAKA-testnet作業ディレクトリ、T12引継ぎ領域の `result.json` 77件中、対象EXECとgenesis・rulesが一致したものは10件。DAA観測列がある9件の最大値は43、選択枝の最初のbond年齢適格DAAは5040。新規の長時間採掘は起動していない。

第8回と同じtokenizer、Rust 1.93.0、locked/offline、evm、stack設定で次の2モードを順に実行した。

```sh
export MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1
for mode in notice clear; do
  MISAKA_PRIVATE_TB1_COURT_EXPIRY="$mode" cargo +1.93.0 \
    --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
    --config 'profile.dev.package.kaspa-consensus.opt-level=0' \
    test --locked --offline -p kaspa-consensus --features evm --lib \
    private_registered_tb1_pending_freeze -- --ignored --test-threads=1 || exit 1
done
```

- noticeは1件成功（18.77秒）。選外署名にはmarkerが付かず、選択席の通知は通常テンプレートから受理され、DAA 61でUnverified。通知直前のチェックポイントを再オープンし、OriginalとPANEL証明書の本文を除去しても保存したdrawで検証できた。別network・旧親を拒否し、正しい署名でもcoinbase markerが欠ければ通知として受理しなかった。通知のdeltaを戻すと保留フラグと前状態rootが復元された。
- clearは1件成功（23.27秒）。選外署名を含む承認を拒否し、全席の正しい承認を通常テンプレートから受理した。DAA 61でReplayVerified、独立FOW1認証は未成立。

今回の接続範囲は署名済み取引から採掘・受理・Court期限まで。常駐PANELによる不一致証拠の自動作成・提出、TB1の同一genesis自然成熟、第13回の新規P2P IBDと第14回の実媒体予算は未達。公開有効化とQF1 REVIEW_HOLD解除は承認待ち。

### 第12回　burn停止中の新規効果と保存S3決済

追加登録モデルの支払済みburn行を、MOE2の見積り・予約・支払直前、QF1 Originalの新規受理、FOW1成功Finalの受理で参照する。各判定は対象ブロックのDAAと選択状態を使う。年次指定が発効すれば既存の停止行を履歴として保持しながら免除する。既存Originalの資料・裁定期限を維持する。

保存S3の `s3-cold-award-settlement.json`、`s3-settled-8703.json`、`s3-cold-settlement-transition.json`、`s3-settled-reorg.json` を再照合した。DAA 8602・8667の各3台は未決済。8703の3台では受取script・outpoint・320084650080 sompiが一致し、別枝往復の前後でも同じ出力と消費済みindex 0を保持した。今回は保存済みJSONの独立した記録間の照合であり、DBの新規再生、採掘、追加決済は行っていない。

FOW1の有償通常経路は未達。現行の3種の登録は、局所評価、REVIEW_HOLD中の投票意図、FOW1用採掘profile未定義のTIRに対応する。独立の適格登録とdraw、常駐署名者の手数料付き配送、価格付け、同一仕事の権利消費を通した証跡が必要。第13回の全状態比較、第14回の実媒体予算は引き続き判定予定で、未達項目を合格に繰り入れない。

保存台帳の照合はリポジトリルートで再現できる。

```sh
python3 - <<'PY'
from pathlib import Path
import json
base = Path("docs/evidence/palw/tfo3-genesis-award-2026-11-24")
def read(name):
    return json.loads((base / (name + ".json")).read_text())
pending = read("s3-cold-award-settlement")
live = read("s3-settled-8703")
transition = read("s3-cold-settlement-transition")
reorg = read("s3-settled-reorg")
assert [(g["generationDaa"], [p["settled"] for p in g["peers"]]) for g in pending["generations"]] == [(8602, [False]*3), (8667, [False]*3)]
award = pending["sharedAward"]
assert award == live["cold"]["award"]["award"] == transition["transition"]["award"]
assert live["settledGeneration"] == transition["generation"] == reorg["snapshot"]
assert live["matchingColdPeers"] == live["matchingLivePeers"] == transition["matchingPeers"] == [0, 1, 2]
payout = live["liveSettlement"]["payout"]
assert award["outpoint"] == payout["outpoint"] and award["scriptPublicKey"] == payout["output"]["scriptPublicKey"]
assert int(award["valueSompi"]) == payout["output"]["amount"] == 320084650080
for point in ("before", "after"):
    assert reorg[point]["spentIndices"] == [[0], [0], [0]]
    assert len(reorg[point]["settlement"]) == 3
    assert all(s["payout"]["outpoint"] == award["outpoint"] and s["payout"]["output"] == payout["output"] for s in reorg[point]["settlement"])
assert [p["exitCode"] for p in reorg["awardJournalReplay"]] == [0, 0, 0]
PY
```

第11回と同じRust 1.93.0、locked/offline、evm、単一テストスレッドを使用。consensusの `private_registered_tb1_pending_freeze`（native certificate=1、Court=clear、registered ballot=burn、ignoredを指定）は1件成功（8.20秒）。実支払済み停止下で見積りと予算要求がModelBurnPausedを返し、既存仕事IDとOriginal行を保持。解除枝・停止枝・解除枝で見積りの回復と再拒否を確認した。`private_moe2_three_process_registration` は1件成功（3.24秒）。consensus-coreの `model_pause_v1_` は2件成功（0.06秒）、`fow1_committed_final_uses_independent_draw_votes` は1件成功（0.07秒）。最後は独立drawと裁定期限の単体fixtureで、有償登録からの成功Finalは未検証。停止中の新規Original・成功Final・支払の各拒否を個別に通す負例も未検証。

### 第13回　登録済み市場を含むpruning pointからのP2P参加

第12回の基点は `9c88c7ebc`。短時間の隔離試験で追加モデルの登録取引を署名・受理し、登録と市場が存在する位置を過ぎるまでArchivalを進めた。送受信で同一のgenesisと試験用finality depth 8、pruning depth 16を使用。受信ノードに与える初期状態はgenesisの構成とその初期UTXOのみ。供給元からTCPで先端を通知する。

修正前は受信側がヘッダー証明を検証し、staging上にpruning pointを設定した後、trusted block 9件の処理中に `wrong block version: got 0 but expected 1` で切断した。DAA windowに含まれる当該ネットワークのgenesisはversion 0であり、通常の後続ヘッダーのversion規則を満たさない。短い履歴を持つ登録済み市場を同期に使ったことで、Round 16終端の状態一致だけでは見えなかった失敗を再現した。

再現コマンドはリポジトリルートで次を実行する。トークナイザー・外部の保存DBは不要。

```sh
export RUST_MIN_STACK=67108864
cargo +1.93.0 --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.dev.package.kaspa-consensus.opt-level=0' \
  test --locked --offline -p kaspa-consensus --features evm --lib \
  registered_market_pruned_node_catches_up_over_tcp -- --test-threads=1 --nocapture
```

修正後、trusted block処理で照合するgenesisは、ローカル設定から再構成した全ヘッダーフィールドが一致し、取引・EVM payloadとも空のものに限る。TCP試験は2回成功（0.98秒、0.91秒）。DAA 3のpruning pointに登録モデル・TIR・MOE2市場の行があり、stagingの生成・採用を確認した。DAA 11で全PALW状態と派生index、全35 UTXO、pruning pointの全carriage、TIR登録情報が一致。受信側の接続とワーカーを閉じ、管理DBが選んだconsensusを再オープンして同じ比較に成功した。初回の終端rootは `53dc6fcfe99aa6990f02956fd8a48739443818f549e33dc668b4ac3b1bd21560a6a520b644a3358af27a6cf5c291760f9c1027d24f7ac332a4a9e6eb53a6e517`。

同じconsensusテストバイナリで `only_the_configured_header_only_genesis_matches_the_window` と `t12_a_moved_pruning_point_s_proof_builds_validates_and_applies_at_225` が各1件成功。前者は完全なgenesisブロック、余分なEVM payload、旧ハッシュに含まれないヘッダーフィールドの差を除外する。未決Court・Gate C、未消費Ticket、未決済報酬、TB1/FOW1/QF1/MAD2固有の非空状態を持つP2P参加、および当該受信ノードでのreorg往復は引き続き未検証。この試験の一時DBはRAM上に置いた。

### 第14回　実媒体の容量判定と容量監視の修正

作業媒体の空きは4,884,262,912 Bで、必要な12,884,901,888 Bまで8,000,638,976 B不足した。Round 16測定器の開始前検査でも12 GiB下限で拒否され、子プロセス、受信DB、ログのいずれも作成されなかった。新しい実媒体IBD、同時compaction、世代交代のピーク、継続稼働予算の合格は未確認。公開対象全経路の接続条件も残っており、第15回の最終総合試験の開始条件は満たしていない。

停止中のS3ホームと隣接する10世代ディレクトリを読み取り専用で再計測した。142,483個の一意な通常ファイルで論理270,484,801,065 B、割当合計270,798,884,864 B、APFSのファイル専有領域40,815,775,744 B。割当には世代間の共有extentが重複する。専有領域には共有extentが入らず、集合全体の正確な物理占有量は求めていない。10ディレクトリにrolling markerは0件。DAA 8602で実受賞claimに属する名前付き資料は3ピア計24,633,344 B、Gate C待ちの後続claimは1,605,632 B。各ファイルの内容を世代manifestのSHA-256と照合した。これらの件別計測から共有journal・SSTを含む限界費用は算出できない。[世代別内訳・ピア別の資料とハッシュ](../evidence/palw/pruned-ibd-2026-11-25/s3-cold-generation-budget.json)

通常の容量監視では深さ制限による未計上と、管理ルートの外に重なる登録先がある場合の二重加算を修正した。走査失敗時は最後の完全な集計を保持し、容量不明を記録して新規予約を拒否する。監視tickは書き込みを停止し、再計測が成功して再開余裕を満たした時に再開する。回収後に実測し直し、不要データの整理だけで余裕が戻れば圧迫時の回収を省く。測定器には子プロセス終了時のサンプルとログを含む合計60 GB上限を追加した。

snapshot・restoreでは、入力ツリー内で同一inodeのファイルをコピー先の世代内で共有する。原本・保存世代・書込可能な復元先は相互に独立。RAM上の短時間fixtureにS3の実際の後続claim資料1件とそのcheckpoint用リンクを置き、reflinkなしのコピーを比較した。通常ファイルの割当が3,203,072 Bから1,601,536 Bに減り、復元後の書き込みでも原本・世代manifestと同じ内容を保存した。この50%は同一ファイルに2名があるfixtureの結果で、S3全体の削減率ではない。

検証はRust 1.93.0のkaspa-utilsライブラリ28件成功、既存expiring_cache試験1件ignored。このうち容量予算15件成功。Pythonのsnapshot等29件、閉鎖DBコピー4件、容量測定器5件成功。閉鎖DBコピーは空き約360 MiBのRAM媒体で既定の512 MiB下限により2件が失敗したため、小さい一時DBをAPFS上で再実行し4件すべて成功した。これらは短時間fixtureである。

```sh
cargo +1.93.0 test --locked --offline -p kaspa-utils --lib -- --test-threads=1
for suite in test_palw_devnet.py test_copy_closed_rocksdb.py test_measure_round16_receiver.py; do
  python3 -m unittest discover -s scripts/drills/tests -p "$suite" || exit 1
done
scripts/palw-devnet storage --home "$HOME/.local/share/misaka-tfo3-award/network-s3"
```

### 第13回の持ち越し　Pruned Nodeでの署名済み追加登録のreorg往復

短時間のTCP試験で、共通親の同一UTXOをAではモデル登録、Bでは通常送金に使用した。Pruned受信側はgenesisからIBDした。pruning pointはDAA 3。供給元で枝を切り替えて先端を通知し、受信側の選択チェーン、全PALW状態・派生index、pruning pointのcarriage、全UTXOを照合した。Aで登録と市場行およびburn相当の支出が現れ、Bで取り消され、Aへの復帰で再現した。

| 時点 | tip DAA | 外れたブロック | 加わったブロック | UTXO数 |
|---|---:|---:|---:|---:|
| 初回同期 | 11 | — | — | 35 |
| A | 12 | 0 | 2 | 36 |
| B | 13 | 2 | 3 | 36 |
| A復帰 | 14 | 3 | 5 | 36 |
| 受信DB再オープン | 14 | — | — | 36 |

最初に試した深いBからの復帰は供給側でDominanceViolationとなった。経済的に同点の枝の浅いreorg上限2 DAAを適用したまま、上限内の枝で往復させた。既存のRustテスト registered_market_pruned_node_catches_up_over_tcp を拡張し、4回成功（1.90–1.96秒）。private_moe2_three_process_registration と only_the_configured_header_only_genesis_matches_the_window も各1件成功。未決Court・Gate C、未消費Ticket・未決済報酬を伴う参加と往復、および実媒体容量はこの試験の対象外。

### 残作業のセッション配分（第14回終了時の再計画）

第12〜14回の未達分を含め、通常経路・全対象状態・実媒体予算の判定に追加6枠、全合格後の最終総合試験に別途1枠を予定する。各枠はCodex単独で順次実施する。番号は工数見積りであり、完了は右欄の結果で判定する。

| 回 | 作業 | 完了を判定する証跡 |
|---|---|---|
| 15 | TB1成熟・配送 | 同一genesisの履歴から通常入口で期限を迎え、配送・再試行・停止を照合 |
| 16 | QF1・MAD2 | 既存のholdを保持した隔離環境で、通常の資料取得・通知・kernel採用からclaim効果と拒否条件を確認 |
| 17 | FOW1 | 適格な独立登録、draw、有料配送、成功Final、価格確定を同一仕事で追跡 |
| 18 | MOE2と決済 | 第17回の仕事で通常の受賞・支払・権利の一回消費、reorg後の復元を照合 |
| 19 | Pruned全状態 | 対象状態を列挙し、それぞれ非空の台帳・資料でP2P同期、reorg往復、再起動後のArchivalとの一致を確認 |
| 20 | 実媒体容量 | 12 GiB以上の空きがある媒体で短時間fixtureを使い、IBD・compaction・世代交代のピークと保存資料を予算に照合 |
| 21 | 最終総合試験 | 上記全項目合格かつ試験開始条件充足後にのみ実施し、結果を記録 |

第15回には同一genesisの成熟履歴または短時間での正規履歴生成、第17回には仕様に適合する有償採掘profile、第20回には実媒体空きが必要。条件未充足なら該当項目は未達として残し、独立項目は順番に進める。新しい長時間試験・公開有効化・QF1のhold解除・VPS配備はこの配分に含まない。

### 第15回　TB1のReady配送障害と成熟履歴

通常holderのReady配送で、あるdealerのoutbox書き込みが失敗すると、同じtick内で後続dealerをすべて飛ばしていた。失敗したdealerを未配送のまま保持し、ほかのdealerについては通常のcustody・選択枝検査を経て配送するよう修正した。呼出元には最初のエラーを返す。

private_tb1_certifies_pruned_panel_from_native_ready が26.64秒で成功。EXECとPANELのholder 0でdealer 0の配送にPermissionDeniedを注入し、同じtickでdealer 1の署名済みReadyが通常の読戻し検査を通り、dealer 0のファイルが存在しないことを確認した。障害解除後にdealer 0を配送し、custody再オープン後にも最初の署名済みpacketと一致した。native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected も子プロセスを含め2.50秒で成功。Rust 1.93.0、locked/offline、kaspa-consensusのevm構成で実行した。

保存したscheduled-native-closeのverifierCutはDAA 8、bond年齢の初回適格DAA 5040、EXEC期限200000。2つのTB1試験領域・S3保存領域・両作業ツリーのresult.jsonを再照合し、genesisとconsensusParamsIdが一致した10件中、DAA観測列を持つ9件の最大値は43だった。今回の証跡はReady配送の再試行とfinality撤回時の停止まで。成熟仕事量は短時間fixtureから供給しており、同一genesisで自然成熟したEXEC/PANELの4台配送は持ち越す。第16回は計画どおりQF1・MAD2。

### 第16回　QF1の開示に基づくPANEL不一致通知

選択親のPANEL抽選とEXEC証明書に結び付いた署名入口を追加した。受理済みOriginal本文を読み、所有者署名・収録枝・席・network・期限を検査する。Stop、End、EarlyEosの開示を既存のCourt判定器で検査し、矛盾が成立した案件のmarkerを観測digestとして署名する。処理中に親またはpruning importの世代が変われば署名を返さない。これにより通知の作成側でも、観測digestを当該開示から再現できる。

第11回と同じRust設定・tokenizerで private_registered_tb1_pending_freeze を逐次実行した。MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 と MISAKA_PRIVATE_TB1_COURT_EXPIRY=notice でStopが18.64秒、さらに MISAKA_PRIVATE_TB1_EARLY_EOS=1 でEarlyEosが18.59秒で成功。両方とも選外鍵と欠損開示を署名前に拒否し、正規通知を通常テンプレートで受理、支払保留、delta巻戻し、期限のUnverifiedを確認した。同一ビルドでexpiry=clear、MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=burn の登録・投票・有償停止・解除・reorg試験も8.06秒で成功した。

この試験ではfixtureが開示を署名入口に渡し、通知を取引に組み込んだ。常駐PANELが対象を発見して開示を取得し、手数料を払って通知を配送する経路と、End通知の結合試験は残る。MAD2のkernel自動採用とfork指定の経済反映も未達。第16回全体の合格は保留する。

### 第17回　FOW1独立再実行の応答長と署名者のモデル選択

署名済みOriginalの答えが短すぎる、または長すぎる場合、従来は申告長で作ったtrace木への追加または終端処理で失敗し、FOW1検証者が完了した不一致を返せなかった。申告長は最初の木のサイズを決めるために使い、登録済みdecode上限と実際のEOSで推論を終了する。実長が違う時だけ最初の木を破棄し、実長を使ってストリーミングでtraceを再構成する。2回のtoken列・Stop・処理位置数・Readoutが異なる場合や資料の読取りに失敗した場合は結果を渡さない。一致時の推論は1回で、producer用の最大1 GiBの記録バッファを予約しない。

独立Gate Cの署名入口は、担当draw、Originalと認証済みEXEC slotからdescriptor・inventory・class・tokenizerを求め、その4項目でローカル実装を選択できるようにした。呼出元から既知のinstanceを渡す既存入口も同じ処理を通る。保持するモデル・資料の取得先は呼出元が供給する。

再現には第11回と同じtokenizer、Rust 1.93.0、locked/offline、evm構成を使用する。MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1、MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear で private_registered_tb1_pending_freeze を実行し、MISAKA_PRIVATE_FOW1_WRONG_LENGTH を short、long、未設定の順に切り替える。署名済みのOriginalが通常の隔離ノードで受理されたあとに再実行する。別途 misaka-palw-remote の signed_fow1_ballots 試験は private-tb1-tir,kaspa-consensus-core/evm 構成で実行する。

最終ビルドで実長は2。申告1と申告3は各9.45秒で、再構成した四つの件数・rootとTicketが独立に生成したOriginalと一致した。通常の一致試験は23.44秒で成功し、PANEL drawをGate Cの担当に渡す負例はモデル取得前に拒否された。投票取引fixtureは0.16秒で成功。一致票と不一致票に別の支払鍵を使い、各取引で手数料250,000 sompiとお釣りを照合した。お釣りを1 sompi変更した取引は資金入力の署名検証に失敗した。これは既知のdrawとローカルUTXOを与える署名試験で、mempoolや台帳に手数料を支払った実績ではない。

第17回の有償通常経路は未達。実装上の3登録profileは引き続き有償予約に進めず、保存した評価記録はG-COSTがUNKNOWN、採用判定がpending。常駐サービスからのモデル取得・配送、同じ仕事での有償予約、成功Final後の独立認証、価格確定・一回消費を通した実績は得られていない。第18回の受賞・決済の判定にもこの依存が残る。

### 第18回　MOE2決済記録と消費済み予約の復元

Pruned用snapshotの復元時にも、残っているEXEC予約の消費件数が1で、モデルと消費済み報酬額が決済記録に一致することを要求する。delta適用・巻戻しと同じ判定を使う。期限経過後は予算行の方が先に回収され得るため、存在しない行に対する照合はできない。期待する状態rootを指定した復元と、決済記録・予約鍵・仕事・attemptの既存検査も継続する。

保存S3の4件のJSONを第12回の条件で再照合した。DAA 8602・8667は3台とも未決済、8703では同じ受取script・outpointに320084650080 sompiを計上し、reorg往復前後で3台とも消費済みindexは0。S3上の新規支払は発生させていない。

通常MOE2の価格確定は未達。RFC-0064のRは認証済みcanonical claimの仕事価格・tier・資本配分・発行上限を反映した額で、予約上限からは確定しない。通常ノードのaward検査は価格状態復元後にPrivateMoe2AwardUnresolvedを返す。第17回の有償仕事から同一claimの支払までを通す条件は残る。第19回のPruned全対象比較、第20回の実媒体検証については第14回終了時の順序を維持する。第18回時点で12 GiB以上の空きがあるマウント済みファイルシステムはなかった。

Rust 1.93.0で `cargo +1.93.0 --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' --config 'profile.dev.package.kaspa-consensus.opt-level=0' test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --test-threads=1 --nocapture` が成功（親試験3.32秒）。未消費、1 sompi不足、正しく消費した予約を照合し、3プロセスで同じブロックと決済後rootを再現した。試験内でsnapshot復元、delta巻戻し・再適用、同じattemptの再使用拒否、期限経過後の復元を確認した。仕事とrelease額は会計fixtureから与えた。

### 第19回　未決claimを持つPrunedノードのTCP参加とreorg

隔離ノードでモデル登録がpruning pointに入った後、署名済みのfloor attemptを通常入口で受理し、未決claimを一件作った。受信側はgenesisからTCPで参加する。登録と市場はpruning pointのsnapshotに入る。未決claimはその境界より後のブロックから再構成する。初期同期直後、枝A、枝B、枝Aへの復帰、受信DBのcold reopenの5箇所で、ArchivalとPrunedのtip、全PALW状態、pruning pointのcarriage、全virtual UTXOを比較した。

全5箇所でclaimはProvisional、Gate Cの照会結果はpending。未決済escrowは320084650080 sompi、予約担保は10752660。これは短時間fixtureのclaimで、S3の実受賞とは別件。fixtureのattemptはharnessが作る署名済み入力を使う。実モデルの推論資料はこのブロックからは復元できない。最初にclaimを登録直後に作った試行では、pruningが未決claim手前で止まり、想定した登録後の境界を作れなかった。境界を先に作ることで、本来の保持上限を守ったまま参加を検証した。

未決Court、Final後のGate C投票待ち、未消費Ticket、決済待ちの実受賞について、状態ごとに同じTCP参加・reorg・再起動を通す検証は残る。今回のclaimは分岐前の共通親にあり、登録とUTXOが枝ごとに切り替わる間も保持された。

第18回と同じCargo設定・evm構成で `registered_market_pruned_node_catches_up_over_tcp -- --test-threads=1 --nocapture` が2.03秒で成功した。保持上限は両ノードともclaim受付blue scoreの一つ前。両側で受付ブロックの本文を読み、ヘッダーに収録されたattemptからclaim ID・trace root・execution rootを照合した。別途、第11回に記載した `s3_cold_pruning_frames_preserve_awarded_and_covering_claims` を読取り専用で実行（11.58秒）。DAA 8602・8667・8703の各3台で世代内のフレームが一致し、保存した受賞claimの85,938葉と後続claimの5,560葉の資料を検証した。S3の保存フレームは今回のTCP試験には供給していない。

### 第20回　保存S3のactive DB予算と低容量時の世代回収

DAA 8602・8667・8703の3世代×3ピアで、停止DBの通常ファイルを読取り専用で再計測した。各ピアのdatadirにあるconsensus、meta、utxoindexから、末尾が.log、または名前がLOG、LOG.oldで始まるinfo logを除く（分類の実装はstorage_budget.rsのclassify）。st_blocks×512を合計したactive DBは11,049,250,816〜11,456,856,064 Bで、9件すべてが30 GB構成のactive DB設計配分10,497,346,816 Bを551,904,000〜959,509,248 B超えた。active DBは自然増加を許す加算型なので、走査量が設計配分を超えただけでノードが停止することはない。受信側stagingがどこまで増えるか、compactionと世代切替時に30 GBへ収まるかは別途実測が必要。共有extentを含む世代別の割当を世代間で足して物理使用量とはしない。[ピア別測定値](../evidence/palw/pruned-ibd-2026-11-25/s3-active-db-budget.json)

Round 16測定器の開始前検査で実媒体の空きは3,908,993,024 B。12 GiBの下限を8,975,908,864 B下回り、受信DB、ログ、子プロセスの作成前に停止した。第20回のIBD・compaction・世代切替のピーク測定は未達。第19回の残件と各機能の通常経路もあるため、第21回の最終総合試験の開始条件は未充足。

低容量時のobsolete consensus世代回収は、従来、管理DBを走査して読めなかった行を候補判定から外していた。このため管理下の旧世代を孤立ディレクトリとして削除する可能性があった。現在は走査・復号・管理キーと行の照合・activeとstagingの参照確認を済ませてから候補を返す。ディレクトリ列挙が失敗しても部分結果を使わず、規定の世代名を持つ実ディレクトリだけを対象とする。

第18回と同じRust 1.93.0・locked/offline・evmの設定で、consensusの `g14_orphan_dir_tests -- --test-threads=1` が0.07秒で成功。実際のFactory回収入口で、管理行が復号不能の場合とstaging参照先が欠けた場合に回収量0で対象ファイルが残った。行の修復後に旧世代001だけが削除され、002内の保存ファイルと004のstagingディレクトリが残った。続けて `registered_market_pruned_node_catches_up_over_tcp -- --test-threads=1` が2.02秒で成功した。S3の保存世代に対する削除・書込みは行っていない。

### 第16回の残件　選択枝の資料応答からQF1 Endの不一致を通知する

リモートPANELの署名入口 `signed_panel_end_from_history` は、選択枝で受理された資料要求と応答から最終readoutとEndを取得し、署名済みOriginalとの不一致を確認する。要求と応答の対応、選択席、期限、親とimport世代を検査する。履歴の探索上限が足りない場合は再試行可能な `WouldBlock` を返す。

第11回と同じ登録tokenizerとRust設定で、`private_registered_tb1_pending_freeze --ignored --test-threads=1 --nocapture` に `MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_END_CASE=1` を設定した。2席の応答を含む直近3ブロックで署名が成立し、2ブロックでは待機に戻った。通知と参照の各取引による2,000,000 sompiの手数料、支払保留、巻戻しを照合した。`MISAKA_PRIVATE_TB1_COURT_EXPIRY=notice` を加えた試験は19.01秒で期限後のUnverifiedを確認。期限切れ指定を外した試験は11.37秒でEndのCourt裁定とDB再オープンを確認した。

Original、資金UTXO、当番の起動、取引のブロックへの投入は短時間fixtureが用意した。常駐PANELによる対象の自動発見・取引配送、MAD2のkernel採用は残る。保存S3の実受賞決済は第1回の証跡を参照。第15〜20回の未達条件は元の枠で追跡する。

同じバイナリでStop、EarlyEosの期限切れ試験と private_moe2_three_process_registration を逐次実行し、各1件成功（18.57秒、18.59秒、3.22秒）。

### 第20回後の残工数見積り

第20回の到達目標は未達。ここから通常経路・全対象状態・容量予算の合格判定まで追加6セッションを割り当てる。作業は単独・直列で、順に (1) TB1同一genesis成熟と4台配送、(2) QF1常駐PANELとMAD2 kernel採用・fork経済反映、(3) FOW1の有償適格性と独立再実行・成功Final、(4) 同一仕事のMOE2価格・受賞・一回決済、(5) 全対象を非空にしたPruned/Archivalの同期・reorg・再起動照合、(6) 実媒体のピーク容量測定を扱う。各枠の受入証跡は上の第15〜20回の表を引き継ぐ。各回終了時に合格した条件、未達条件、次の依存を記録する。最終総合試験はこの6枠の外に1枠を確保し、すべて合格してから着手する。有償profileの適格性と媒体空きは現時点で未充足であり、完了時期の確約はできない。

### 追加第1回　TB1 holderの再起動と古いcloseへの配送

署名済みのholderは、再起動時に排他leaseを取って読み直したcustodyレコードのanchorを、選択枝探索の開始位置に使う。公開前に現在の枝でfinality、期限、資格、pruning import世代を再検査する。未署名のroundは従来どおり選択親から探索する。journal形式は既存のものを使う。

第18回のCargo設定で `private_tb1_certifies_pruned_panel_from_native_ready -- --test-threads=1` が43.61秒で成功。隔離fixtureのPANELはclose blue 64・release開始DAA 144とし、署名時の選択枝で直近64ヘッダーの外にcloseがあることを実測した。EXEC・PANELとも3席が各journalを閉じて再オープンし、最初のpollで同じcloseへの配送が成立した。再署名後もoutboxは最初の署名バイトを保持し、両certificateが受理され、チェックポイントの再オープン後もseedと状態rootが一致した。成熟仕事量はfixtureから供給した。

同一genesis・rulesの保存証跡で確認できた最大DAAは43、bond年齢が適格になる最初のDAAは5040。4台再開の実媒体空きは3,186,176,000 Bで、既存の12 GiB開始下限に届かない。追加第1回の自然成熟と4台実配送は未達として残す。

同じRust設定で `cargo test --locked --offline -p misaka-palw-remote --lib tb1_exposure:: -- --test-threads=1` が15件成功（4.38秒）。A→B→A、子プロセスからの再開、停止マーカーの追加・既存マーカーの同期失敗を含む。停止の記録が永続媒体に残らなかった場合、新プロセスに停止を引き継ぐには独立した隔離記録が必要になる。

### 追加第2回　QF1の選択席がEnd資料を要求する入口

隔離PANELの資料要求に使うnative読取りを追加した。指定した選択親のEXEC closeとslotで既存のintake索引を直接引き、Originalの受理位置・期限とdrawを照合する。担当席、要求済みの席、期限、反証状態を区別する。新しい永続行は作らない。署名側は受理済み本文をヘッダーの取引Merkle rootまで照合し、Originalに記録されたtrace件数から最終readoutまたはEndのindexを計算する。資料要求と同じOriginalの決済参照を返し、公開前にtipとpruning importの世代を再確認する。

試験の入力はEXEC close、slot、席鍵、要求する開示の種類。常駐サービスによる対象slotの割当て、fee UTXOの調達、mempool経由での両取引の配送はこの試験の範囲外。これらは追加第2回の残件として扱う。

MAD2の受入条件はpreのRFC-0067（`7ad72a9e0`）§4–7に従う。KERNEL_AUTOには該当する自動検査が必要。BFT_DECISIONで正規のYESを受理する条件にはG-COST/G-USEの測定合格を加えない。共通技術条件、現行policy・世代、停止、各claimの独立検証は両方で必要になる。現在のT12のQf1BallotIntentは有償予約入口でREVIEW_HOLDを返す。古いQF1の保留、追加モデルのkernel自動採用、fork指定の経済反映はそれぞれ別に追跡する。

第18回と同じRust 1.93.0・locked/offline・evmのビルドで `private_registered_tb1_pending_freeze --ignored --test-threads=1 --nocapture` を逐次実行。登録tokenizer、`MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1` と `MISAKA_PRIVATE_TB1_END_CASE=1` で、2席が発見した別々の開示要求が受理され、署名済み不一致通知からCourt裁定まで11.19秒で成功。`MISAKA_PRIVATE_TB1_COURT_EXPIRY=notice` の場合は支払保留と期限後のUnverifiedまで18.98秒で成功。選外、二重要求、期限、古いtip、要求deltaの巻戻しを検査した。DB再オープン後は、Original本文を除いた状態でも未使用の席を識別し、その席の署名入口がWouldBlockを返した。End指定を外し `MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=burn` でMAD2の既存登録・採否世代・有償停止解除・A→B→Aを再検査（8.08秒、成功）。

### 追加第2回の継続　担当slotを知らないPANEL席の資料要求

native読取り `palw_qf1_material_duties` は、選択親のPANEL抽選記録をOriginal順に走査する。一回に調べる行数を1〜64に制限し、残りがある場合は最後に調べたOriginalを返す。席に該当する行がないページも続行できる。継続位置は同じ選択親に対して使用する。署名側は該当Originalの受理済み本文からEXEC close・slot・参照を読み直し、その親でまだ要求できる席であることを確認する。再起動用の別の永続索引は不要。

前節のEnd試験で、2席の資料要求と不一致通知の3回について、各回2件の署名済み取引を同じMiningManagerにLow優先度・Orphan禁止・RBF禁止で投入した。標準relay設定とPQ支払検査を使い、プールの選択器が両方を収録したテンプレートを取得した。先行要求の収録は同じプールへ通知した。席鍵から両要求を発見し、選外鍵の直接署名、古い親での照会、要求済み席への再割当を拒否した。End裁定まで2回成功（11.18秒、11.22秒）。期限切れは18.74秒で支払保留からDAA 61のUnverifiedを確認した。各試験のdrawは1件。起動・鍵・開示選択・fee入力・資料応答はfixtureが用意している。常駐プロセスと別ノードへのP2P配送、混雑時に2件が別々に収録された後の再配送は未検証。

追加第2回で、同じEnd試験に `MISAKA_PRIVATE_QF1_SPLIT_REQUEST=1` を追加した。最初の席の要求と参照を順番に同じmempoolへ投入し、別々のブロックで採掘した。各入力が消費され、各2,000,000 sompiの手数料を差し引いた釣銭が元の支払先に残った。どちらの単独収録でも要求と支払保留が発生しないことを照合した。その後、席が同じ対象を再発見し、fixtureで新たに資金を付けた対を収録した。通常のCourt裁定まで11.66秒、期限切れのUnverifiedまで18.85秒で各1件成功。常駐プロセスの自動再配送・追加手数料の管理・混雑時のP2P伝播は未検証。計画上は引き続き追加第2回内の部分合格。

### 追加第2回の継続　隔離holderからQF1のEnd資料を要求する

隔離genesisに登録済みのholderで、起動前に `ready-fees.json` の任意項目 `panel_material` と専用の `panel-material.seed`（32バイト、所有者だけがアクセスできる通常ファイル）を設定する。席鍵と手数料鍵は別々に読み込む。PANEL資料要求だけの構成ではVSS peerの設定を省略できる。

```json
{
  "maximum_fee_sompi": 2000000,
  "outpoints": [[null, null, null, null], [null, null, null, null]],
  "panel_material": {
    "outpoints": ["<request fee txid>:0", "<reference fee txid>:0"],
    "end_record": false
  }
}
```

この設定は、台帳から発見した適格な1件に対して、指定した2入力から各取引の上限までの手数料を支払う許可になる。`panel_material.original` を指定するとそのOriginalだけが対象。`end_record` がfalseの席は最終readout chunk、trueの席はEndを要求する。席ごとに異なる開示を設定する。入力はどちらも既存の手数料鍵に属し、他の設定済み支出と重複できない。

常駐処理は1回に最大64行を読み、同じ選択親では続きから探索する。席の署名時にOriginalの本文と現在の当番を照合する。取引ごとに通常のmempool検証とピアへの告知を使う。要求と参照は独立に収録され得る。片方がすでに採掘された場合、同じ2入力による再支出は終了する。既定では初回の2入力だけを使う。追加の手数料を許可する設定は次節に記す。

Rust 1.93.0、第18回のprofile指定で `cargo test --locked --offline -p kaspad --features private-integration-config --lib palw_tb1_holder:: -- --test-threads=1` は6件成功（6.20秒）。今回のテストでは空の先頭ページからの継続、走査終了後の同一tipでの休止、A→B→Aでの再探索、2入力の重複拒否、専用資金だけでの設定読込を検査した。常駐ノード2台間の受信、資料応答、常駐席による不一致通知、再配送用の追加手数料の自動管理は未検証。

同じ署名入口を使うconsensus fixtureも再ビルドし、前節の分割収録を有効にして再実行した。1件成功（11.53秒）。別収録からの再要求とEndのCourt裁定を確認した。ここで2件目の資金と配送を用意するのはfixtureである。

### 追加第2回の継続　指定Originalの再配送に使う手数料の上限

`panel_material.original` で対象を固定し、`panel_material.retry_from_change` を `true` にすると、選択席は同じ要求と参照の再配送を1回分まで賄える。既定値はfalse。Originalを省略したまま有効にすると設定読込みが失敗する。初回と再配送で各2取引のため、選択枝上でこの資金指定が支払う上限は合計 `4 × maximum_fee_sompi`。例えば2,000,000 sompiを指定した場合は合計8,000,000 sompi。

holderは、初回の2入力が消費済みで同じ席の要求が未成立のとき、選択親から直近32ブロックまでの本文を読み戻す。元の入力、署名済み要求と決済参照のバイト列、取引ID、両方の未使用の釣銭を照合し、各出力の金額・支払先と現在のUTXOが一致した場合に限り再署名する。支払鍵、bond、手数料上限は初回と同じ検査を通す。再配送には初回の直接の釣銭だけを使う。片方しか収録されていない場合、本文がprune済みの場合、対象が探索範囲外の場合、釣銭がすでに消費された場合は追加支出しない。再起動時は現在の台帳と同じ設定から判定し直す。mergesetの非選択親ブロックまで遡る回復は対象外。

分割収録のconsensus試験では、初回の支出前、片方の支出後、異なる署名の要求で釣銭を使えないことを検査した。両取引の収録後、探索で得た2つの釣銭から同じOriginalの要求と参照を再び払い、標準mempool経由の収録とEndのCourt裁定まで11.67秒で成功した。

同じバイナリに `MISAKA_PRIVATE_TB1_COURT_EXPIRY=notice` を追加し、再配送後の支払保留から期限切れのUnverifiedまで18.98秒で成功。前節と同じkaspadコマンドは7件成功（6.43秒）。holder側は選択履歴readerを使い、設定からの再オープン、元枝への復帰、再配送の取引ID一致、追加支出1回での停止を確認した。実際のノードプロセス間の再配送は未検証。

### 追加第2回の継続　選択枝の要求に対するEnd資料の応答

隔離holderの `ready-fees.json` に `panel_answers` を追加できる。各要素には `original`（対象Originalのハッシュ）、`actor`（要求した席のactor ID）、`outpoints`（応答用・参照用の順に2入力）を指定する。起動ディレクトリの `<original>.end` は、producerの `LocalOriginal::end_disclosure()` が返す最終readout chunk、経路、End、経路の連結形式を使う。資料を保持するノードが応答でき、手数料鍵には各入力の支出権限が必要になる。応答専用の構成では席の署名鍵ファイルとVSS peer設定を追加する必要はない。

holderは現在の選択枝の未回答・期限内の要求を読み、その要求が指定したindexのレコードとMerkle経路だけを送る。開示が要求時のtrace rootを開くことを確認し、要求を収録したブロックの本文から同じOriginal・attemptの決済参照を復元する。未回答行が残り、その要求本文とローカル資料が読めれば、Originalのブロック本文をpruneした後も応答できる。2取引は個別に手数料上限を適用して通常のmempoolへ投入する。各割当は初回の2入力だけを使用する。

前節のkaspadコマンドで8件成功（6.72秒）。応答専用設定から要求を確認するまでファイルを開かず、欠損・短い資料を拒否し、取得した正しいEndの応答と参照にそれぞれ指定入力を使った。consensus試験も同じ登録tokenizerと分割収録設定で再ビルド・実行し、2席への各応答と参照を標準mempool経由で採掘した。Original本文を除いたチェックポイントを再オープンして同じ資料から応答できた。そこから要求本文も除くと未回答行は残り、応答はWouldBlockになった。End裁定が11.67秒、notice期限切れのUnverifiedが19.02秒で各1件成功。独立プロセス間のP2P受信、producerによるファイルの自動配置、不一致通知の常駐配送は未検証。

### 追加第2回の継続　producerのOriginal発行時にEnd開示資料を保存する

隔離producerの `LocalOriginal::retained_publication` は一回の推論から得たEnd開示の両方のMerkle経路と所有者署名を照合する。応答holderのディレクトリを引数で受け、Original IDをファイル名にして `<original>.end` を確定する。ファイル・ディレクトリの同期とEXECの再確認に成功してから署名済みOriginalの取引payloadを返す。同じ内容での再試行は既存ファイルを使う。既存内容が異なれば発行が失敗し、先に保存した内容を維持する。検証側は `retain_qf1_end_disclosure` で所有者が署名した資料を保存でき、Endが矛盾するときも裁定用に残す。

第3回の登録tokenizerと `private_tb1_stable_panel --ignored --nocapture --test-threads=1` を使用した。通常ケースは12.40秒で成功。producerが保存内容を読み戻してからOriginalを渡し、2つの独立プロセスも同じID・同じ2,813バイトの資料を照合して保存した。競合内容を事前に置いた場合の発行拒否・既存内容の保持と、producerでの同一資料の再保存も検査した。`MISAKA_PRIVATE_QF1_INCONSISTENT_READOUT=1` では矛盾したOriginalの公開をfixtureから与え、3プロセスが証拠を保存してComputationRefutedを確認した（12.38秒）。ブロック配送はfixtureのUnix socketを使用。常駐producerの起動からholderへのP2P配送、資料の世代回収・容量予算、物理電源断時の回復は未検証。

追加第2回の容量診断では、`scripts/palw-devnet storage --home HOME --holder-dir HOLDER` で外部holder直下のOriginal IDに対応するEnd開示資料を指定できる。holderは複数指定可能。現在世代・snapshotも同じ名前の資料を分類する。重なる計測範囲とhardlinkを合わせた合計、holder別の内訳を返す。容量診断の短時間テスト3件、および外部holderの7 Bを計上するCLI試験が成功した。常駐ノードの書込予算と実媒体のピーク測定は未検証。

追加第2回のEnd資料保存では、ノードの容量予算が有効なプロセスで、holderの実際の保存先の物理空きとPalwRetainedの予約を一時ファイル作成前に検査する。容量拒否はWouldBlockで返り、呼出元のOriginal発行は進まない。同一資料の再同期と不一致資料の上書き拒否は予算消尽時も動作する。Rust 1.93.0の独立libtestでend_storeの2件が成功し、既存依存を使ったprivate-tb1-tirライブラリ全体のmetadata型検査が成功。通常のおもな再現コマンドは `cargo test --locked --offline -p misaka-palw-remote --features private-tb1-tir --lib end_store::`。今回はconsensus-coreの既存rlibが約491 MB、ビルド媒体の空きが349 MiBのため、Cargo経由の再リンクを中止した。プロセス間での予約、外部holderの累積使用量に対する常駐予算、同時書込み時の媒体ピークの検証は残る。

追加第2回の外部holder容量計上を常駐予算に接続した。Unixで容量上限とholderを設定したノードは、起動時と15秒ごとのスキャンで、holder直下のOriginal End資料の割当済みバイト数をPalwRetainedに加える。デバイスとinodeが一致する管理領域内の別名は一回計上する。設定ディレクトリを読めない場合は最後の完全な測定値を保持し、使用量不明として受付を停止する。自動削除は追加していない。

Rust 1.93.0で `cargo test --locked --offline -p kaspa-utils --lib storage_budget:: -- --test-threads=1` の16件が成功。縮小した上限での停止、保存済み資料を残した予算の再作成、holderを移動して戻した場合の状態復帰を確認した。同じ版のutilsをリンクしたend_storeの独立libtestは3件成功。2つの子プロセスが共有holderに別々の16 KiB資料を保存し、親で両方のバイト列と割当済み容量を照合、予算を再作成して同じ合計を読み戻した。これは保存後の計測であり、同時書込み中のピーク・プロセス横断の予約上限・kaspad実機再起動の検証は残る。

共有holderの新規End保存は、ディレクトリの非ブロッキング排他ロックを取得してから使用量を再計測し、書込予約・ファイル同期・ディレクトリ同期までロックを保持する。競合中の新規保存はWouldBlockで再試行できる。同一資料の再同期と矛盾する内容の拒否はロック中も成立する。保存先を計測対象に含むノードでは、周期スキャン前に別のwriterが確定したEnd資料も予約判定に含む。再計測は管理領域全体を走査するため、大きなDBでの発行遅延は未測定。

Rust 1.93.0のend_store独立libtestは4件成功。空のholderでは16 KiBを予約できた予算で、別々の子プロセスが各16 KiBを書いた直後の追加16 KiBを拒否し、新規ファイルが生じないことを確認した。ロック保持中の別descriptorに対する新規書込拒否、既存内容の再同期、ロック解放後の再試行も確認した。対象は同一holderを使用する協調writerであり、別holder・任意の外部プロセスの書き込みや実媒体ピークは対象外。

### 追加第2回　別ブロックに入ったQF1資料応答の再配送

常駐holderの `ready-fees.json` で各 `panel_answers` に `"retry_from_change": true` を指定すると、初回の応答・決済参照が別々に収録されても、同じOriginal・席の未回答要求に一度だけ追加手数料を使える。省略時は初回の指定2入力まで。再配送時は、現在の選択親から要求ブロックの手前まで最大32ブロックを読み、初回の2入力、応答・参照のバイト列、各取引の直接の未使用釣銭を照合する。各入力に既存の支払鍵・bond・手数料上限の検査を再適用する。再起動後も設定と選択履歴から同じ再配送取引を構成する。

Rust 1.93.0でready_fees本体と通常の署名関数を使った独立libtestは4件成功（0.48秒）。応答側で設定の再読込、A→B→A、再配送IDの一致、未回答要求が消えた場合・片方の釣銭が消費済みの場合・追加支出後の停止を検査した。選択履歴とUTXOはfixtureのreaderが供給した。常駐ノード間のP2P配送と実ブロックへの再収録は今回未検証。

QF1再配送の追加検査。初回の2入力が探索の先頭付近で見つかると、旧処理は下限のブロックに到達する前に探索を終えていた。変更後は要求側のOriginalブロック、応答側の要求ブロックが、現在のtipの選択親チェーン上にあることをreachabilityで確認してから釣銭を探索する。非選択枝・mergeset上だけにある下限からの追加支出は見送る。

選択履歴readerに、本文を読み出せる側枝の要求を追加した。旧ライブラリではその要求にも追加支払対を生成し、回帰テストが失敗した（3成功・1失敗）。変更後のremoteライブラリをRust 1.93.0で再コンパイルして同じready_fees独立libtestを実行し、4件成功（0.46秒）。本文・2入力・未使用釣銭が一致しても側枝の要求には再署名しない。この混在状態を実ノードで観測した証跡はない。

### 追加第2回　要求本文をpruneした後のQF1応答

通常の要求受付 `VerifiedMaterialRequest::bind_original` は、要求に付いた決済参照の全フィールドを受理済みOriginal本文と照合している。応答側は要求本文が取得できないとき、指定tipの選択台帳にあるOriginal受付行からブロックを引き、本文をヘッダーまで検査して同じ参照を復元する。要求本文が壊れている場合は拒否し、両本文とも欠ける場合はWouldBlockを返す。既存の受付行を読み、新たな合意行やsnapshot形式は追加していない。

Rust 1.93.0でconsensus-coreのmetadata型検査、参照復元コードの独立libtest 1件が成功した。要求だけ残る場合、Originalだけ残る場合、両方欠落、要求本文の破損、attempt不一致を検査した。RocksDBの既存checkpoint fixtureにも、要求本文だけを除いた状態で応答markerと決済参照が元と一致する検査を追加した。この変更後のconsensusバイナリの再リンク・checkpoint試験・P2P実配送は未検証。

### QF1資料応答のcheckpoint復元と読取り障害（2026-11-26）

Rust 1.93.0、空のtarget、locked/offline、kaspa-consensusのevm付きlibtestを再リンクした。通常のget_blockはstatus、header、取引本文、EVM payloadのDB読取り失敗を呼出元へ返す。要求本文の欠落時は受理済みOriginalに記録された参照で応答できる。隔離checkpointの要求行に壊れた値を書いた場合はOriginalが残っていても応答を停止した。両本文が欠ける場合はWouldBlockになった。物理媒体自体の故障は注入していない。

第18回の登録tokenizerとstack設定で、MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1、MISAKA_PRIVATE_TB1_END_CASE=1 を指定し、private_registered_tb1_pending_freeze --ignored --nocapture --test-threads=1 が11.10秒で成功。MISAKA_PRIVATE_TB1_COURT_EXPIRY=notice を加えた場合は17.92秒で成功し、DAA 61でUnverifiedになった。同じバイナリでregistered_market_pruned_node_catches_up_over_tcp が2.03秒で成功し、再編成・再オープンを含む5地点を照合した。QF1資料自体のノード間配送と全対象状態のPruned IBDは未検証。

### 保存S3実決済のcold rollback・再適用（2026-11-26）

Rust 1.93.0で現行の tfo3-consumed-receipt を再ビルドし、上記 check_s3_award_transition.py の一時領域に実ディスクを指定した。DAA 8703の各ピアについて、閉じて開き直した一時DBの決済差分を親→子→親→子と適用した。各段でPALW全状態とUTXO全2,259行を照合し、UTXO commitmentも一致した。受取スクリプト・金額・outpoint・受理ブロック・決済先端を実走時の記録と照合した。DAA 8602と8667は各3ピアで未決済、8703でfrontierが7866から8247へ進んだ。これは保存済み実決済の1遷移の再現で、ノード間で競合枝を伝播させる試験は含まない。原本は読取り専用で開き、各DBファイルのサイズ・更新時刻が前後で一致した。

### Pruned IBD後のA→B→Aとclaim別の資料保持義務（2026-11-26）

登録市場のTCP fixtureにA枝だけの署名済みattemptを追加した。共通の未決claimでは、両ノードのjanitorが参照する保持義務APIは5地点とも削除を許可せず、通常のclaim詳細照会後に保持義務ありと回答した。A枝のclaimでは、枝分かれ前は台帳行なし、Aであり、Bでなし、再選択したAとPruned DBの再起動後はありとなった。各地点で台帳行の有無と保持義務を両ノードで照合し、両者の全PALW状態とUTXO全行が一致した。初回の試行では、IBD直後のキャッシュが空のときに保持義務APIがエラーを返した。これは削除保留であり、回帰試験では通常のclaim読取りによる復帰も確認する。

Rust 1.93.0のlocked/offline、kaspa-consensus、evm付きlibtestで registered_market_pruned_node_catches_up_over_tcp が2.16秒で成功。measure_pruned_market_fixture.py による再実行も5地点一致。実行資料本体の複製・削除、未決CourtのP2P同期、長期容量予算はこの試験の対象外。[2回目の状態比較と物理割当](../evidence/palw/pruned-ibd-2026-11-25/market-tcp-branch-claim.json)

### 同一claimの保持資料にハードリンクがある場合の容量予算と回収（2026-11-26）

Unixの容量計測では同じ区分内の複数名が指すinodeの割当を一度計上する。janitorでは同じclaimのmaterial・answerが同一inodeの最後の2リンクなら、その組を削除して回収できる割当を一度見積もる。保持義務の再照会が肯定なら両方を残す。別の保存先にもリンクがある場合の見積りは0。回収の報告は実際に成功したunlinkを基準にする。区分をまたぐリンクは各区分の予約計算に含まれ、APFSの共有extentはこのinode照合の対象外。受付再開時の物理空き容量の確認は引き続き必要。

Rust 1.93.0、locked/offlineで kaspad の private-integration-clock 付きlibtest palw_retention::tests:: が24件成功。kaspa-utils のlibtest storage_budget::tests:: が17件成功。実ファイルで保持・削除・再走査を検査した。S3全体の削減量やB/DAAを示す試験ではない。

### 保存S3世代間の割当増減と決済世代のWAL復旧（2026-11-26）

停止した3世代のdata配下を読み取った。DAA 8602→8667の65 DAAで全ファイルの割当は56,643,584 B減り、8667→8703の36 DAAでは1,167,249,408 B増えた。後者の内訳でWALは738,643,968 B、consensus SSTは424,648,704 B増加した。各ピアのノードホームでの増加は1 DAAあたり約8.05、12.43、11.89 MB。ログ、資料等を含む世代全体では約32.42 MB/DAA。この数値は閉鎖時点のファイル割当の差で、区間内ピーク、長期増加率、共有extentを除いたボリューム消費とは区別する。世代ごとのAPFSファイル専有量も別欄に記録した。再現は scripts/drills/measure_s3_generation_growth.py に停止世代の親ディレクトリを渡す。[世代別・ピア別の実測](../evidence/palw/tfo3-genesis-award-2026-11-24/s3-generation-growth.json)

全9個のconsensus DBの最終OPTIONSでWALは無圧縮だった。DAA 8703のpeer-0から大きなCourt journal実データ2行を読み、private_palw_journal の durable_court_wal_reopens_after_writer_exit 試験を再実行した。WALを同期して書込みプロセスを終了し、SSTがない状態から2行の全バイトが一致した。WALの割当は無圧縮24,838,144 B、現行の容量制限付きPruned設定9,289,728 B、PrivatePalw設定9,289,728 B。試験時間11.44秒。これは2行への圧縮効果であり、S3全体への削減率は未測定。

### 容量制限付きArchival NodeのWAL圧縮

容量制限とDefault presetを使うPALW ConsensusV2のArchival Nodeで、新規WALのZstandard圧縮を有効にした。対象DBはFactoryが作成するactiveとstaging。Pruned Nodeにはcold SSTのlevel 12圧縮も適用する。

DAA 8703の保存S3のpeer-0を読取り専用で開き、kaspa-databaseのprivate_palw_journal統合試験 durable_court_wal_reopens_after_writer_exit をRust 1.93.0、locked/offlineで実行した。無圧縮・Archival・Pruned・PrivatePalwの4方式でWALを同期した直後に書込みプロセスを終了した。旧無圧縮WALをArchival設定で回復し、圧縮済みWALも標準の読取りで元の2行と全バイト一致した。SSTなしのWAL割当は順に24,838,144 B、9,289,728 B、9,289,728 B、9,289,728 B。14.64秒で成功。

MISAKA_PRIVATE_MARKET_STORAGE_CAP=1 を指定し、evm付きkaspa-consensusのlibtest registered_market_pruned_node_catches_up_over_tcp を単独で実行した。ArchivalとPrunedを同じ試験プロセスの30 GB予算で動かし、IBD終了後に両DBの最新OPTIONSから wal_compression=kZSTD を確認した。5地点で未決claimの保持義務、全PALW状態、全UTXOが一致し、A→B→Aとcold reopenを含め2.30秒で成功。既定の公開運用台数・長期負荷での予算充足とCPUコストは未検証。

### QF1 End応答と決済参照が別々に採掘された場合の再収録

private_registered_tb1_pending_freeze に MISAKA_PRIVATE_QF1_SPLIT_ANSWER=1 を追加した。2席目が要求したEnd開示・決済参照を標準mempoolに通し、それぞれを別ブロックに収録した。各単独収録ではmarkerがなく要求は未回答だった。holderが使うend_material_replyとmaterial_pair_changeで、同じ開示と参照、要求以後の選択履歴上にある両入力の直接の釣銭を復元。fixtureの手数料鍵でその2出力を使い、通常テンプレートで両取引を同時に再収録してmarkerを照合した。2席分の応答を再構成する履歴窓はこのケースでは5ブロック必要で、4ブロックではWouldBlockになった。

第18回と同じ登録tokenizer、Rust 1.93.0、locked/offline、evm付きconsensus libtest、単一試験スレッド、native certificate・End設定を使用。応答だけを分割したケースは11.48秒、MISAKA_PRIVATE_QF1_SPLIT_REQUEST=1 で要求も分割したケースは11.72秒でEnd裁定まで成功した。両方を分割して MISAKA_PRIVATE_TB1_COURT_EXPIRY=notice を加えたケースは18.03秒でDAA 61のUnverifiedを確認。常駐holder間でのP2P再送、常駐席による不一致通知の自動起動はこの試験の対象外。

### QF1の個別裁定を保存履歴から復元する際の本文検査

read_published_terminal_verdict は事件と裁定の両ブロックで取引本文をヘッダーのMerkle rootと照合し、その本文のcoinbaseから各markerを確認する。登録tokenizerを指定し、Rust 1.93.0のevm付きkaspa-consensus libtestで private_tb1_stable_panel --ignored --test-threads=1 --nocapture を実行。通常入力と MISAKA_PRIVATE_QF1_INCONSISTENT_READOUT=1 の両方で、再オープンした各プロセスの試験DBから事件・裁定を一方ずつ取り除くと、個別読取りと履歴走査はいずれもWouldBlockになった。元のmarkerを保持したまま取引を1件追加した本文は、どちらもInvalidDataになった。元の本文を戻すと元の裁定に一致した。対象はローカルDBの保存履歴であり、欠落本文のネットワーク取得と全状態のPruned IBDは未検証。

### 管理ディレクトリ消失時の受付停止と、Pruned同期後のモデル制御判定

容量管理の変更 c26947fa6 では、走査終了時に管理ディレクトリを開けることを確かめる。親ボリュームに空きがあっても、管理先が見つからない間は直近の完全な計測値を保持する。回収付き予約はUnknownUsageを返し、監視tickで受付が停止する。ディレクトリを復帰させた後は同じ使用量で受付を再開した。kaspa-utils の storage_budget::tests:: は18件成功した。

Rust 1.93.0、locked/offline、evm付きkaspa-consensusで、MISAKA_PRIVATE_MARKET_STORAGE_CAP=1 を設定し registered_market_pruned_node_catches_up_over_tcp -- --test-threads=1 --nocapture を実行した。TCP同期直後、枝A、枝B、枝Aへの再編成、Prunedノードの再起動後の5地点で、Archival・Pruned双方の通常APIがgenesisモデルを制御対象外、共通履歴の追加登録モデルを対象と判定した。枝Aで追加したモデルは枝Bでは未登録として拒否され、再選択後に対象に戻った。全PALW状態、UTXO、未決claimの保持義務も照合し、2.36秒で成功した。

| 測定点 | Archival consensus 割当 B | Pruned consensus 割当 B | 試験全体の予算計上 B |
| --- | ---: | ---: | ---: |
| TCP同期後 | 565248 | 299008 | 1093632 |
| A | 667648 | 405504 | 1306624 |
| B | 798720 | 516096 | 1548288 |
| A再選択 | 929792 | 581632 | 1744896 |
| Pruned再起動 | 929792 | 651264 | 1859584 |

各行は状態一致を確認した時点の計測。30 GBの予算は試験プロセス内で共有した。独立した各実ノードのstaging・compaction中のピーク、保存S3全履歴のPruned IBD、停止・解除の全裁定は別途検証を要する。

### Archival設定の永続化とPruned IBDの世代切替時の割当

Factoryが既存の管理DBを開いてArchival設定を切り替える際に、変更後のフラグを書き込んで管理WALを同期する。従来はバッチを作って破棄しており、他のメタデータ更新より先に再起動すると、kaspad起動時のArchival→Pruned確認に古い値が渡り得た。管理DBを切替ごとに閉じて再オープンし、true→false→trueの各状態を確認した。Rust 1.93.0、evm付きkaspa-consensus libtestの g14_orphan_dir_tests は2件成功。既に古い値だけが記録されているDBの過去のモードは復元できない。電源断とdaemonの対話プロンプトは試験していない。

同じバイナリで MISAKA_PRIVATE_MARKET_STORAGE_CAP=1 を設定し、scripts/drills/measure_pruned_market_fixture.py にlibtestのパスとscratch親ディレクトリを渡した。TCP同期と再編成・再起動後を含む5地点の照合が完了。開始から1.372秒の走査で、Pruned側の旧DBに192,512 B、新DBに94,208 B、重複inodeを除く合計286,720 Bの割当を観測した。Pruned側の管理DB等を含む観測最大値は745,472 B、fixture全体は2,097,152 B。走査間に生じたピークとAPFSの共有extentは測定範囲外。実S3規模でのstagingおよびcompaction予算は未検証。[測定結果](../evidence/palw/pruned-ibd-2026-11-25/market-tcp-generation-overlap.json)

### Pruned IBD中断時の世代回収と再試行

Factoryは管理DBの全世代行と稼働世代のディレクトリを確認してから不要世代を削除する。stagingのディレクトリを削除した後、その行の削除とstaging参照の解除を同期書込みの単一バッチで確定する。削除エラー時は管理行と予約を保持し、次のstaging作成を拒否する。旧版の途中終了で参照だけ残った場合は、稼働世代を確認でき、対象ディレクトリが既に消えている場合に復旧する。対象に実体が残っている場合は回収を保留する。

Rust 1.93.0、locked/offline、evm付きkaspa-consensus libtestの g14_orphan_dir_tests を単一スレッドで実行し3件成功（0.18秒）。削除不能の世代に対する新規作成拒否、管理DBを開いた子プロセスがディレクトリ削除直後に終了した際の復旧、旧版の分割書込みの復旧を確認した。稼働世代のfixtureファイルは一致し、新しい世代キーを取得できた。同じバイナリで MISAKA_PRIVATE_MARKET_STORAGE_CAP=1 の registered_market_pruned_node_catches_up_over_tcp が2.65秒で成功し、正規TCP同期、A→B→A、再オープンを含む5地点で状態を照合した。プロセス終了試験は管理DBと世代ディレクトリを対象とし、物理電源断と保存S3規模の同期中断は未検証。

### Pruned IBDのDB作成拒否と再試行

Factoryで次の世代名に未登録のディレクトリが存在すると、世代キーを進める前に作成を拒否する。新規staging行と管理情報は同期バッチで記録する。stagingのRocksDBを開く際は既存のtry_buildを使い、外部WALディレクトリの準備失敗も呼出元に返す。DBを開けなければ作成途中の世代に既存の取消処理を適用する。

Rust 1.93.0、locked/offline、evm付きkaspa-consensusのlibtestで g14_orphan_dir_tests が3件成功（0.20秒）。次世代名に置いた未登録資料が保存され、外部WALの親に普通のファイルを置いた場合は作成失敗後もその内容と稼働世代の資料が一致した。通常設定で管理DBを再オープンし、失敗した世代の行・参照が消え、Factoryの通常入口で後続世代の作成・取消に成功した。同一バイナリで MISAKA_PRIVATE_MARKET_STORAGE_CAP=1 の registered_market_pruned_node_catches_up_over_tcp が2.10秒で成功し、5地点を照合した。管理DB書込み中のENOSPC、電源断、保存S3の実規模での再試行は未検証。

### TB1停止マーカーの部分書込みとholderによる証明書参照の再構成

T12のcustody試験で、子プロセスのRLIMIT_FSIZEを予約済みjournalの長さ+1に設定し、A→Bで停止マーカーの1バイトを書いた後にEFBIGを発生させた。枝Aへの復帰後も同じholderはKeepHaltedと元のOSエラーを維持し、署名を拒否した。別プロセスで再オープンすると不完全なjournalとして拒否された。マーカーが1バイトも残らなければ再起動後に停止を判別できず、当該holder・roundの隔離を独立した永続状態で保持する必要がある。

常駐holderのcertificate_referenceは、参照先候補の各ブロックのヘッダーと取引Merkle rootを検査してから、支払済みrelease batchを選ぶ。EXECとPANELの双方で、採択済みバッチの本文欠落はWouldBlock、支払取引とcoinbaseを残した本文改変はInvalidData、本文を戻すと元の参照に一致した。Rust 1.93.0、locked/offline、単一スレッドでmisaka-palw-remoteの tb1_exposure::tests:: は14件（4.95秒）、evm付きkaspa-consensusの private_tb1_certifies_pruned_panel_from_native_ready は1件（53.32秒）成功。実媒体の電源断、保存S3上のTB1稼働、公開プロフィールでの独立した成熟仕事量は未検証。

### QF1資料配送での選択枝と同期世代の確認

holderは資料要求に署名する前にOriginalが選択親の祖先であることを確認する。応答時は要求ブロックを確認し、本文をpruneした要求の決済参照をOriginalから復元するときはそのブロックも確認する。別枝の要求には応答せず、別枝のOriginalしか得られない場合はWouldBlockを返す。資料探索の位置と再送用の釣銭候補は、先端ハッシュが同じでもpruning importのepochまたは稼働consensusインスタンスが変わると破棄する。旧DBの識別には弱参照を使う。

Rust 1.93.0、locked/offlineでkaspadのprivate-integration-clock付きlibtest palw_tb1_holder::ready_fees:: が4件成功（0.51秒）。選択枝と別枝の区別、要求本文がない場合、同じ先端でのepoch更新・DB交換、台帳から再取得した再送取引IDの一致を確認した。第18回と同じtokenizerを使い、evm付きconsensus libtest private_registered_tb1_pending_freeze をnative certificate・End・分割要求・分割応答の設定で単独実行し、End裁定まで11.52秒で成功。常駐ノード間でのP2P再配送と保存S3での全状態同期は未検証。

### QF1 holderが使うnative取引通知の送信期限

holderの資料取引は submit_rpc_transaction_guarded を通じてnative取引の通知キューに入る。500ミリ秒の送信間隔内に来た少量の通知にはキュー単位でタイマーを設定し、追加の取引や採掘のない間も送信する。同じIDの重複を除き、全ピア向けと最大8ピア向けの宛先範囲を維持する。

Rust 1.93.0、locked/offlineで kaspa-p2p-flows のlibtestを使用。idle_native_batch_reaches_peer_without_another_submit_or_block は、ループバックTCPの受信側で2件のIDを順番どおりに確認し、その後の単独再送も受信した。RPC優先度・送信範囲・重複排除の既存試験と合わせて4件成功（1.03秒）。この試験はfixtureのIDで通知を受信するところまでを扱う。常駐holderからの署名済みQF1資料取引について、別ノードが本文を取得し採掘前にmempoolへ受理する結合試験は未検証。

### QF1再送取引のTCP配送と受信側テンプレート

資料要求と参照が別々に採掘された時点で、選択枝の釣銭を使って署名した再送2件を送信側の submit_rpc_transaction_guarded に渡した。応答と参照が分割された時点でも同じ試験を実施した。各時点で2つの独立したRocksDBチェックポイントからノードを起動し、ループバックTCPで接続した。受信側は空のmempoolから取引本文を取得し、署名を含む全取引の一致を検査した。両時点とも受信側の先端は進まず、標準のブロックテンプレートに再送2件が選択された。元のfixtureは続いてEnd裁定まで完了した。

第18回のtokenizerとビルド設定で、MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE、MISAKA_PRIVATE_TB1_END_CASE、MISAKA_PRIVATE_QF1_SPLIT_REQUEST、MISAKA_PRIVATE_QF1_SPLIT_ANSWERを1に設定し、MISAKA_PRIVATE_QF1_RELAY_GENESIS_MSに全子プロセス共通の現在のUnix時刻（ミリ秒）を渡す。Rust 1.93.0のevm付きconsensus libtest private_registered_tb1_pending_freeze を --ignored --test-threads=1 --nocapture で実行し、1件成功（12.85秒）。要求・応答それぞれについて受信テンプレートの親と2取引IDを出力した。送信取引の組み立てと入金にはfixtureの鍵と手数料を使用した。常駐holderの設定読込み・自動起動からこの配送に至る結合、受信側テンプレートそのものの採掘、公開設定での稼働は未検証。

### Pruned受信ノードが生成したQF1資料ブロックの返送と裁定

上記のTCP fixtureで受信側をPruned設定にした。受信側は自身のmempoolから得たテンプレートをheartbeatブロックとして通常の合意入口に提出し、そのハッシュをArchival送信側へ通知する。送信側はTCPでブロックを取得する。両側で選択先端、UTXO検証済みステータス、保存されたPALW state rootの一致を確認した。資料要求では両DBの未回答caseが今回の要求ブロックを指し、応答では両DBのcaseが終了し、それぞれの保存ブロック本文から認証済みEnd開示を取得できた。元の裁定fixtureにも受信ノードのブロックをそのまま渡し、End裁定まで完了した。

直前節の同じフラグ・libtestで1件成功（13.08秒）。この回の要求ブロックは 0a57444113fe、対応するstate rootは 9453bfcb72fe。End応答ブロックは bb858d8b9f2b、state rootは f0691dbe4229（各先頭12桁）。各再送時点の採択済みDBから独立したチェックポイントを作成した。鍵、資金、時刻、PoW省略はprivate fixtureの設定。常駐holderからの自動発行、fresh IBD、実際のディスク回収後の同じ開示の取得、全対象状態の容量予算はこの試験の対象外。

### 新規Pruned IBDで再現したQF1 Original本文の欠落（未解消）

直前節の同じ試験に MISAKA_PRIVATE_QF1_FRESH_IBD=1 を加えると、genesisとpremineだけを持つ3台目がTCPでArchivalノードに接続する。受理条件にはstaging DBへの世代交代、UTXOの同期、資料ブロックの選択、未回答caseまたはEnd資料の読戻し、3台のPALW root一致を含む。失敗時のOriginal読取りは、DBキー欠落、ローカル容量不足、ストア読取り異常を区別する。

深さ12の調査ではDAA 27の同じ選択ブロックでdeltaを比較した。既存fixtureが受理後に追加したFrontierとtable 136のfloorは元DBだけにあり、本文を再生した新規DBにはなかった。後続ヘッダーの親root検査が不一致を検出した。この成熟入力をブロックから再生できたという証拠には使えない。

深さ4の隔離fixtureでは成熟入力が取得済みsnapshotに入り、その後の資料要求の再生に進んだ。stagingを採用してUTXOを取り込んだ後、prepare_qf1_material_request が選択済みOriginalのヘッダーを読み、BlockTransactionsの本文キー欠落を検出した。受信ノードは仮想状態の解決を保留し、接続を維持した。RUST_LOG=kaspa_consensus::pipeline::virtual_processor::utxo_validation=debug,kaspa_p2p_flows::ibd=info で元ブロック、欠落したストアキー、IBD保留を確認できる。Rust 1.93.0、evm付き、locked/offlineの新規IBD試験は12.19秒で失敗。要求に続くEnd応答の新規IBD検査には到達していない。同じバイナリで追加変数を外した既存2台の配送・ブロック生成・End裁定は12.92秒で成功した。

公開条件には、期限内に必要なOriginalの認証可能な資料を新規ノードに配送し、再起動・再編成後にも必要期間だけ参照できることが残る。今回の変更は取得失敗の分類と再現まで。

### QF1資料要求とEnd応答の新規Pruned IBD（2026-11-26）

深さ4のprivate fixtureで、選択済み台帳のOriginal受理行を64行ずつ走査し、要求期限内で本文が欠けたブロックだけを既存のRequestIbdBlocksで取得した。受信側は選択台帳のブロックID、ヘッダーの再計算ハッシュ、取引Merkle rootを検査する。資料要求の再生には元の受賞参照・trace root・件数を使う。DB世代に付随するメモリ上の保持上限は4096件で、満杯のときは期限切れを除き、空きがなければローカル容量エラーとする。プロセス終了でこの補助資料は失われる。再取得には該当本文を供給できるピアが必要となる。

直前節の3台構成で private_registered_tb1_pending_freeze をRust 1.93.0、locked/offline、evm付きで実行し、1件成功（14.69秒）。要求時と応答時それぞれで新規Prunedノードがstagingを採用した。要求時の3台のPALW rootは 2de29822295f、End応答時は b8260af5c562（先頭12桁）。要求時には履歴ブロックの通常取得ができないままcaseが開き、元ヘッダーと別ブロックの取引を組み合わせた補助入力がInvalidDataとなり、正規本文の再取込み後もrootが一致した。応答時にはcase終了と保存された認証済みEnd開示の読戻しを3台で確認した。元のfixtureでもEnd裁定まで完了した。

この試験は前節に記載したprivate成熟入力をsnapshotから取り込む。新規ノードでのプロセス再起動・reorg後の再取得、供給元ピア切替え、全資料の保持期間と物理容量予算、全公開経路の総合試験は未検証。

### 同じ先端で再起動したPrunedノードのOriginal再取得

private PANEL設定のPrunedノードはピア接続時に、安定したUTXOセットに対応する選択台帳からOriginalの不足を調べる。通常のIBDと同じ本文取得・認証処理を使う。同じ起動中の複数接続は初回の取得処理を共有し、失敗した場合は次の接続が再試行できる。選択先端に追いついていて後続ブロックの通知が来ない場合も対象となる。

上記のprivate fixtureで資料要求とEnd応答の各時点について、TCP・consensus worker・managerを終了し、管理DBが選択した同じ世代を新しいFactoryで開き直した。両時点のPALW root、未回答case、回答済みEnd開示をそれぞれ再検査した。要求時には、起動直後の不足一覧に実際のOriginalがあることを確認した。最初にgenesisしか持たないピアに接続して取得に失敗し、先端と不足行が保持された。続いて同じ先端のArchivalピアに接続すると、新しいブロック通知なしに不足行が解消した。履歴ブロックの通常取得は引き続き失敗し、保存台帳のrootは一致した。Rust 1.93.0、locked/offline、evm付きの private_registered_tb1_pending_freeze が14.86秒と15.10秒で成功した。2回目はIBD本文要求のサーバーログも確認した。

試験では供給元アドレスを順番に指定した。ピアの自動発見、OSプロセスの強制終了、再編成で義務が復活した場合の再取得、公開設定の容量予算は未検証。

### QF1資料要求・End応答での全UTXO照合とディスク割当

同じ短時間fixtureで、Archivalの保存済み選択PALW状態（全行・派生インデックス）、state root、仮想UTXOの全outpointとentryを比較元とした。受信して採掘したPruned、新規IBDを完了したPruned、同じ世代を開き直したPrunedがそれぞれ一致した。要求時はOriginal本文の再取得後にも照合した。

既存の3台試験に MISAKA_PRIVATE_QF1_STORAGE_CAP=1 を追加し、Rust 1.93.0、locked/offline、evm付きconsensus libtestの private_registered_tb1_pending_freeze が14.75秒で成功した。各ノードのfixtureディレクトリをStorageBudgetで走査した。以下のファイル割当はバイト単位。新規ノードは両試行ともgenesisで303,104 B。IBD中は通知直後から完了まで50 msの待機を挟んで走査した。

| 時点 | 全UTXO行 | Archival | 採掘したPruned | 新規PrunedのIBD中の最大観測値（観測数） | 新規Prunedの再起動後 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 資料要求・未回答 | 47 | 1,499,136 | 1,495,040 | 823,296（8） | 831,488 |
| End応答・case終了 | 52 | 1,531,904 | 1,630,208 | 835,584（8） | 839,680 |

全観測点で各台30 GB以下、ボリューム空き12 GiB以上だった。要求時の新規IBD完了時はWAL・info logに761,856 B、残るactive DBに61,440 Bを計上した。再起動後はそれぞれ212,992 Bと618,496 B。走査には管理DBとアドレスDBを含む。予算との比較は観測時に行った。観測間の瞬間最大、APFS共有extentの実回収量、S3規模の定常増加率、圧縮・compaction中の公開容量予算は未検証。

### QF1資料要求・End応答をまたぐA→B→A

MISAKA_PRIVATE_QF1_MATERIAL_REORG=within-window を前節のfresh IBD・容量計測に併用する。private fixtureのfinality深さは3、pruning深さは4。既存の深さ1で試した際にはArchivalがAを選択し続け、分岐切替の検証には使えなかった。通常設定と従来の試験の深さは維持した。

Aは資料要求またはEnd応答を含む。Bはその直前の台帳から作った空のheartbeat 2ブロック。独立に延長したAは元の資料ブロックを含めて3ブロック。Archivalに各枝を通常の入口で提出し、先端をTCPで通知した。新規IBD後に管理DBから開き直したPrunedノードで、A→Bでは1ブロックを外して2ブロックを追加し、B→Aでは2ブロックを外して3ブロックを追加したことを確認した。

| 分岐対象 | B選択時 | A再選択時 |
| --- | --- | --- |
| 資料要求 | 未回答caseなし、UTXO 47行 | 同じ要求ブロックを指す未回答caseが復活、48行 |
| End応答 | 元の要求が再び未回答、保管中のEnd本文による開示は無効、52行 | case終了、各DBから読んだEnd本文による認証済み開示が復活、53行 |

各切替点でArchivalとPrunedの全PALW状態・state root・仮想UTXO全行が一致した。初期検証中に1回、受信側のピア登録を確認する前に切断と判定した。両側の登録を待つようにした最終版で短時間試験を連続2回実施し成功した。これは期限内の同一Originalについて、枝による資料要求の復活とEnd応答の巻戻しを検査する。元のOriginal自体が異なる枝への再編成や、期限切れで回収済みの資料を再取得する経路は未検証。

### 保存S3のWAL全量を使った圧縮・重複排除の実測

scripts/drills/measure_s3_wal_archive.py に停止snapshotの親とscratchディレクトリを渡し、DAA 8602・8667・8703の3ピアで数値名のRocksDB WALを全件走査した。実ファイルの一覧とSHA-256を各世代のmanifestと照合した上で、内容ごとに1本ずつscratchにZstandardレベル3で圧縮し、復号した全バイトのSHA-256を再照合した。圧縮フレームは検証直後に削除した。

| DAA | WAL本数 | 論理バイト | ファイル割当バイト | APFS専有バイト | 圧縮フレームのバイト |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 8602 | 9 | 153,421,225 | 153,427,968 | 153,427,968 | 60,461,471 |
| 8667 | 9 | 38,822,176 | 38,838,272 | 38,838,272 | 15,317,386 |
| 8703 | 12 | 777,453,639 | 777,482,240 | 0 | 308,718,956 |

30本の内容はすべて異なり、ファイル全体のSHA-256による世代・ピア間の重複排除で削れるWALは0本だった。圧縮で論理969,697,040 Bが384,497,813 Bになり、独立した保存媒体に書くペイロードは60.35%減る。8703の元WALは共有extent上にあるため、現行世代を圧縮するだけで同量の空きが戻るとは見積もれない。測定対象は停止ファイルの外側の圧縮であり、ノード内のnative WAL圧縮の削減率、SST・blob、全世代の復元に要する空き容量は別途計測する。[再現結果](../evidence/palw/pruned-ibd-2026-11-25/s3-sealed-wal-compression.json)

### DAA 8703の3ピアの実WriteBatchでnative WAL圧縮と再起動を照合

保存世代 bda15b77344d4277b7a1a63c690da3a1 の各peerのconsensus-001を同一ボリュームの一時領域にcloneし、全通常ファイルのSHA-256を世代manifestと照合した（peer順に169・165・162ファイル）。各コピーをread-onlyで開き、最古の保持WALの開始sequenceからDBのlatest sequenceまでをRocksDBのget_updates_sinceで取得した。バッチ間のsequence連続性を確認し、各バッチをそのまま2個の空DBに書いた。片方はStorageDbBounds::CONSENSUS、もう片方はPRUNED_PALW。両者とも512 MiBのwrite bufferを使用した。2個ともWALを同期し、DBの終了処理前に子プロセスを終了させた。再オープン前にはSST・blobがないことを確認した。

| peer | sequence範囲（両端含む） | バッチ数 | 書込み操作 | 復旧後の行数 | 通常WAL割当（B） | 圧縮WAL割当（B） |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 3872598〜3877074 | 97 | 4477 | 692 | 255,348,736 | 118,390,784 |
| 1 | 3873321〜3878057 | 101 | 4737 | 692 | 269,107,200 | 118,267,904 |
| 2 | 3876238〜3880877 | 101 | 4640 | 718 | 268,574,720 | 118,140,928 |

復旧ごとに、有効な全キーと値をWALを無効にして独立再生した参照DBと順序付きで全件照合し、各値を保存元の台帳にも照合した。範囲削除を含むバッチもnativeのまま再生した。Rust 1.93.0、locked/offlineのkaspa-database integration test sealed_wal_replay をピア別に順次実行し、25.00秒・29.59秒・31.60秒で成功。再現には MISAKA_S3_WAL_FIXTURE に使い捨てのコピー、MISAKA_S3_WAL_FIRST に表の開始sequenceを指定する。

これは3ピアで保持されていたWAL末尾の再生である。過去から残る全SSTを含む台帳の再構築、物理電源断、実負荷でのcompactionとWALの同時ピーク、公開運用の長期容量予算は未検証。

### S3実WriteBatchの再生中にWAL・compaction入力・出力が重なる割当

DAA 8703の3ピアそれぞれで、前節と同じ保存世代のconsensus-001をscratchにcloneし、全ファイルのSHA-256をmanifestと照合した。各コピーの保持WAL末尾をPRUNED_PALW設定の空DBに再生した。バッチ数の先頭約3分の2を3回に分けてflushして入力SSTを作り、自動compactionを停止した状態で手動compactionを開始した。RocksDBがcompaction実行中と報告してから残りのバッチを書き、WALを同期した。1回のディレクトリ走査の前後でnative counterが正、かつWAL・元の入力SST・新しい出力SSTの割当がすべて正の観測を集計した。

| peer | 重なりの観測数 | 最大観測時DB全体（B） | 同時点のWAL（B） | 入力SST（B） | 出力SST（B） | 完了後DB全体（B） |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 735 | 110,157,824 | 37,445,632 | 63,631,360 | 9,011,200 | 46,534,656 |
| 1 | 739 | 109,735,936 | 37,019,648 | 63,635,456 | 9,011,200 | 46,108,672 |
| 2 | 944 | 120,942,592 | 37,449,728 | 73,355,264 | 10,067,968 | 47,595,520 |

全3件で完了後の入力SSTの割当は0 Bとなり、新しいSSTとWALが残った。DBを閉じて再オープンし、独立再生した参照DBと全キー・値を比較した。peer順に692・692・718行が一致し、各値は保存台帳とも一致した。上表は隔離DBのファイル割当で、測定は各ピア順次。DB全体にはWAL・SST以外の管理ファイルも含む。

再現は前節と同じピア別の MISAKA_S3_WAL_FIXTURE・MISAKA_S3_WAL_FIRST を指定し、Rust 1.93.0で cargo test --locked --offline -p kaspa-database --test sealed_wal_replay -- --exact sealed_wal_compaction_overlap --ignored --nocapture --test-threads=1 を実行する。3件とも成功（13.97秒、15.07秒、15.97秒）。観測間の瞬間最大、S3の既存全SSTとblobを抱えた通常運転時のcompaction、IBD stagingとの三者重複、公開用30 GB予算の充足は未検証。

### 停止snapshotの作成中断とrolling世代の回収

snapshot作成に使っていた匿名の一時ディレクトリは、コピー中にプロセスが終了すると次回のライフサイクル操作で識別できず、割当が残った。また旧世代の削除中に終了すると、その世代の公開名から不完全なデータが見え得た。

通常のsnapshot入口はストア内の専用作業領域に未公開コピーを置く。rollingでは新世代のID、ディレクトリのdevice/inode、元のhomeとidentityを持つ世代交代記録を同期してから公開名へrenameする。次回のライフサイクルロック取得時に公開状態を照合し、中断したコピーを回収する。公開済みの場合は同じhome・identityの最新2世代を残し、期限の来たrolling世代を作業領域にrenameしてから削除する。milestone・復元前バックアップは対象外。所有者や公開先の識別が矛盾すると回収を止める。旧版の匿名ディレクトリの所有権は推定しない。

短時間の子プロセス試験でファイル同期後のコピー中、公開rename直後、旧世代内のファイルを1個削除した直後にプロセスを終了させた。次のロック取得で復旧し、2回目の取得でも未決済データ、milestone、必要なrolling世代、無関係な一時ディレクトリの内容が保たれた。公開先inodeの不一致を注入した場合は全公開世代を保持した。test_palw_devnet.pyは31件成功。別のscratchで measure_s3_retained_snapshots.py を実行し、保存済み3世代のpeer-0の実資料でrolling 2世代とmilestone 1件を保持し、DAA 8667・8602の復元内容が原本と一致した。80回のディレクトリ同期時の最大観測割当は56,307,712 B。実媒体の電源断とS3全DBの復元ピークは未検証。

### 第22回　Pruned holderのOriginal再取得後の署名と応答

PANELの要求署名は通常の履歴本文を検査し、本文が欠けている場合に限り、IBDまたは再接続で認証済みのOriginalから受賞参照とtrace件数を読む。台帳の選択ブロック・期限と保存資料を照合し、署名前に座席の担当、署名後に先端とimport世代を再確認する。Endの応答は要求本文・Original本文が両方とも回収済みでも、期限内の同じ資料とholderが保管する開示から生成できる。読み取り障害や本文破損はそのまま返す。自動走査で資料待ちになった担当は、残りのページを走査し終えてから同じ先端でも再試行する。

Rust 1.93.0、locked/offline、evm付きのconsensus libtest private_registered_tb1_pending_freeze に前節と同じsplit request/answer・TCP relay・fresh IBD・within-window reorgを指定し、11.23秒で成功した。コールド再起動後に通常取得できないOriginalをTCPで再取得し、要求が消えたB枝で署名・参照がArchival側および元の有償要求とバイト単位で一致した。A枝への復帰時には両ノードで新規署名が止まった。別の隔離checkpointでは要求とOriginalの本文がともに欠落していると応答はWouldBlockになり、認証済み資料の取込み後に同じ参照・End応答markerを再現した。初回の検査では試験がEnd応答に別の座席の鍵を渡して失敗したため、元の要求に対応する座席に修正して再実行した。

この試験はprivate設定の独立したconsensus managerとTCPピアを使う。常駐kaspadを別々のOSプロセスとして起動し、資料不足の検出から署名・自動配信・受理までを一続きに通す条件は残る。

同じRust・Cargo profileで cargo test --locked --offline -p kaspad --lib palw_tb1_holder -- --test-threads=1 を実行し、7件成功（1.00秒）。常駐holderのReadyFees入口では、受賞IDを固定しない自動走査で本文不足を検知し、ページ末尾に到達した後、同じ先端で資料を供給すると、本文がある場合と同一の要求・参照ペイロードを含む資金付き取引2本を作成した。

### 第22回　forkカタログのコンテナrootと報酬予約

TIRモデルは台帳にinventory rootで登録され、forkカタログには認証済みのコンテナrootでも指定できる。MAD2追加モデル投票に使う分類と、MOE2の新規予算予約・報酬見積りの分類が異なっていた。旧実装ではカタログの発効後も、追加モデル市場を持つ同一モデルに対し、1000 sompiの報酬と1961 sompiの責任額を予約可能と判定した。修正は選択状態・対象DAAを引数に取る既存のtier判定を予算予約と見積りにも適用する。

短時間fixtureで実際のTIR bindingをfoldし、次のDAAでそのコンテナrootを指定する年次カタログを構成した。市場・bond予算は試験入力とした。市場がある場合は発効直前の予算要求が通り、発効位置ではUnknownRegistrationになった。市場がない場合は逆の境界となり、発効後の見積りはMiner 1000 sompi、Panel 0、Option配分なしだった。同じ先端に市場が残る場合は見積りも拒否した。これはカタログ適用後の予約条件の検査であり、BFTのYESがfork登録・カタログ・採用根拠を原子的に作成する遷移、市場のdrain、実際の支払はこのfixtureの対象外。

Rust 1.93.0、locked/offline、evm付きの kaspa-consensus-core libtest で tir_catalog_tier_reaches が1件（0.17秒）、moe2_ が3件、palw_model_market_v1::moe2:: が8件成功した。旧実装での誤った予約許可も同じコンテナを用いた回帰試験で確認した。

### 第22回　保存会計台帳を使うEXEC決済とfork指定の照合

通常の受賞foldは受賞DAAの選択状態からモデルの経済分類を取得する。保存済みのMoe2SettlementBookを渡すEXEC決済入口でも同じ読取りを使い、モデルID・artifact root・登録DAA・所有者・市場と分類が一致することを支払前に確認するようにした。

短時間fixtureでは追加モデルの登録、EXEC予約とclaimの受付を済ませたbookをcheckpointから復元した。受賞位置から同じモデルIDとartifact rootを指定する年次カタログをfixtureの設定に与え、旧市場が残る状態で決済を呼んだ。UnknownRegistrationで拒否し、呼出し前後のstate root、book全体、delta全体が一致した。元の設定で同じ予約を決済でき、snapshot復元とdelta巻戻しの検査も通った。

Rust 1.93.0の cargo test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --test-threads=1 --nocapture を既存のdev profileで実行し、親試験は3.65秒で成功。登録ブロックを再生した子2プロセスでも同じ決済ブロックを確認した。経済分類の切替はfixtureのカタログ入力であり、BFT票からfork登録と分類を確定させる入口は未検証。

### 第22回　QF1の資料要求側・応答側を独立した常駐holderとして起動する

隔離PANELネットワークの `--palw-tb1-holder-dir` で、`ready-fees.json` の `material_only_genesis` に対象genesisを指定できる。要求側は `panel_material` と座席の `panel-material.seed`、応答側は `panel_answers` で担当するOriginalとactorを指定する。各側が自分の `ready-fee.seed` と手数料入力を使う。これで資料担当者にTB1 releaseの署名鍵やcustodyを配る必要がなくなった。従来の設定は従来どおりbootstrapの登録を確認する。資料担当を含む常駐holderはそのディレクトリを終了まで排他利用する。

短時間fixtureは閉じた4台の登録履歴から2台分を一時領域にコピーし、一方を要求専用、他方を応答専用として起動する。両側のdirectory leaseを外側から確認し、loopbackで接続し、両側が同じgenesis・rulesetと保存時の先端を返すことを調べる。正常停止後に同じコピーからもう一度起動する。起動待ち30秒で一度タイムアウトした際、スタックはRPC listen前の組み込みcourt検査を実行中だった。起動待ちだけを最大180秒にし、接続と先端照合の待ちは各30秒に保った。

再現は private-integration-config 付きの misaka-rfc12-private-node と停止済み scheduled-native-close のディレクトリを指定する。

```sh
python3 -B scripts/drills/qf1_material_holder_peers.py "$private_node" "$scheduled_native_close"
```

このfixtureの入力は費消不能なテスト用outpointで、対象のOriginalと選出されたPANELの仕事を含まない。実際の要求・応答の署名、自動配送、選択枝への受理を常駐2プロセスで一続きに確認する条件は残る。

11月26日の実行では、初回（PID 35799・36163）と再起動後（36341・36379）の双方で、要求側が返した仮想DAAは9、両側の選択先端は保存時の `541b0a37e10a4889b6f3e75d5a1b41dcdb19d7b9b6b95cb4cc7e5170ea356083aa34b8417bd2f6440eacfd2180f383d59754168e63fdc42bc0128895eb0c81f5` と一致した。各回とも両方のholderの排他取得を確認し、両ノードの終了コードは0だった。

Rust 1.93.0で `cargo test --locked --offline -p kaspad --features private-integration-config --lib palw_tb1_holder -- --test-threads=1` を実行し、9件成功（8.76秒）。追加した回帰試験は、明示指定のない設定でcustodyが必要なこと、genesis不一致、PANEL規則の欠落、他業務との資金設定の混在、同一holderディレクトリの二重使用を検査した。終了後の再取得も成功した。

### 第22回　保存したReadyの釣銭で独立したQF1 holderを起動する

前節の常駐2台fixtureの手数料入力を、停止済みscheduled-native-closeで受理済みのReadyの出力に切り替えた。要求側と応答側はそれぞれ別の支払鍵を使う。tb1-ready-fixtureの `--material-holder` は公開planとpolicyから隔離genesisを復元し、fixtureの要求鍵が候補集合に含まれることを確認してから一時領域に役割ごとの鍵を書く。応答側に要求側の署名鍵は置かない。

```sh
cargo build --locked --offline -p kaspad --features private-integration-clock --example tb1-ready-fixture
python3 -B scripts/drills/qf1_material_holder_peers.py "$private_node" "$scheduled_native_close" --fixture-generator "$generator"
```

各peerが返したP2P IDで相互の接続を確認する。手数料入力は通常のRPCで選択先端のvirtual UTXOを読み、4出力の金額・支払先script・非coinbaseを照合する。登録時点の台帳には選出済みの仕事がないため、応答設定に指定した既知のReady取引IDについて資料要求は成立しない。両peerのmempool（orphanを含む）で当該入力の支出の有無も確認する。

初回とコールド再起動後の2回とも、両台は同じ選択先端（前節の541b0a37…）・仮想DAA 9に一致した。要求側の2出力は各999,628,475 sompi、応答側の2出力は各2,499,478,475 sompi。4出力とも各支払鍵のscriptに一致し、試験中の両回の観測点でmempoolに支出はなかった。2回とも両プロセスが正常終了した。これにより常駐試験の手数料資金と候補鍵の条件を確認した。対象Originalの登録、候補からの席の選出、有償の要求とEnd応答の自動配送はこの実行では未検証。

### 第22回　常駐holderの送信処理を使った有償の要求・End応答のTCP配送

holderが準備した2取引の送信をFlowContext::submit_qf1_material_pairに集約した。異なる手数料入力、既存mempoolとの競合、個々の取引がすでに受理されているかを照合する。各新規挿入時にpruning importの世代と安定状態を調べ、poolの書込みロック内で選択先端とconsensus instanceを照合する。既存のRPC経由の取引検証・P2P告知を呼ぶ。要求と参照が別々に採掘された場合の回復には前節までの釣銭規則を用いる。

これまでTCP fixtureの送信側が独自に組み立てていた簡略な検査を、常駐holderと同じ入口に置き換えた。古い先端と異なるimport世代でそれぞれ対応する理由で拒否し、どちらの取引も送信側poolに入らなかった。正しい状態では要求と参照が受信側のpoolに届き、受信側が同じ2取引を収録した。End応答のケースでは応答だけを先に送信側poolに入れ、同じholder入口が参照の配送を完了した。両方とも、同じペアの再提出が成功し、受信側の選択ブロックと状態を送信側で照合した。

検証にはRust 1.93.0、前掲の登録tokenizer、native certificate、split request・answer、fresh IBD、within-window reorgを使用した。relay genesisの時刻を実行直前に指定し、`cargo test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --test-threads=1 --nocapture` が11.21秒で成功した。Prunedの新規IBD・再起動・A→B→A後もArchivalと一致した。この試験はfixtureが準備した署名済み取引を2つのconsensus manager間で配送する。別々の常駐プロセスが選出席を発見して署名するところから受理までを一続きに観測する条件は引き続き残る。

変更後のkaspadでも `cargo test --locked --offline -p kaspad --features private-integration-config --lib palw_tb1_holder -- --test-threads=1` が9件成功した（8.91秒）。

### 第22回　Pruned再起動時のQF1 Original再取得とpruning importの競合

Prunedノードの最初のピア接続が不安定なpruning importと重なると、従来のIBDフローはOriginalを取得せず、ノード全体の初回取得を完了と記録していた。同じ先端にいるピアとの間では、新たなIBDを起こすブロックが届くまで必要な資料が欠けたままになり得る。

隔離PANELで動くPrunedノードは、ピア接続時と既存のIBD待機ループ（アイドル時3秒）で取得を再試行する。取得前に安定フラグ、進行中のimport、UTXO取込状態を読み、選択先端を固定して資料を要求する。取得後も同じ先端・import世代で安定している場合に、そのconsensusインスタンスの初回取得を完了と記録する。同時取得は1件に制限し、他のピアはその取得を待たずに通常のIBDを続ける。インスタンスを置き換えると新たに確認する。完了記録はDBを保持しない弱参照で、同じインスタンスの完了後はこのチェックで資料を再走査しない。

Rust 1.93.0の `cargo test --locked --offline -p kaspa-p2p-flows --lib cold_original -- --test-threads=1` が1件成功した。制御したimport状態と先端を返すConsensusProxyを使い、不安定状態・有効なimport tokenで取得しないこと、取得中のキャンセル・取得エラー・世代変更・先端変更後にやり直せること、安定した完了後は同じインスタンスで重複取得せず、別インスタンスで取得することを検査した。

前節と同じ `private_registered_tb1_pending_freeze` のTCP fixture（fresh IBD、within-window reorg）も11.10秒で成功した。Pruned再起動後にはOriginalを持たないピアに続いて、同じ先端のArchivalピアから実際にOriginalを再取得した。要求・応答の両ケースでA→B→Aの各枝の選択状態・state root・仮想UTXO全行が一致した。importの不安定期間とピア接続の重なりは上の制御試験で検査した。実デーモン同士でそのタイミングを再現し、同じ仕事の選出・署名から決済までを観測する条件は残る。

### 第22回　常駐する資料提供者が選択台帳から未回答のOriginalを探す

`ready-fees.json` の `panel_answers` で、資料提供者は対象の席（`actor`）と2個の手数料入力を先に指定できる。`"discover_original": true` を明示すると、選択台帳の期限内のOriginal receiptを1回につき最大64行読み、その席に未回答の要求があるOriginalを順に調べる。供給元は従来どおりholderディレクトリの `<Original ID>.end`。要求と選択枝を再照合し、開示が要求されたtrace openingと一致するときに応答・決済参照の2取引を作る。期限・裁定・資料認証は既存の経路で判定する。

資料がまだ届いていない仕事は保留して次を調べ、末尾まで進んだら同じ先端でも再探索する。破損した開示はエラーとして返し、その仕事で支出しない。先端・consensusインスタンス・import世代が変わると新しい選択状態で再探索する。設定でOriginalを省くだけでは支出を認めない。自動探索に認める支出は指定入力の1組までで、釣銭の再支出はOriginalを明示した設定で扱う。同じ席に対する自動探索と個別割当の重複は設定エラーになる。

`private_registered_tb1_pending_freeze` では選出された席の未回答要求を、この照会で発見したIDから開いた。保持するEnd資料を照合して応答と参照を組み立て、TCPで受信したノードがそれらを収録した。fresh IBD・コールド再起動後も、要求のある枝だけで同じIDが見つかり、End応答が収録済みの枝では探索結果から消えた。Archival・Prunedの両方でA→B→Aの各切替時に照合した。別のactorと当該Originalより後のページには対象が出なかった。前節と同じ登録tokenizer、native certificate、split request・answer、fresh IBD、within-window reorgを使い、Rust 1.93.0のconsensus libtestが11.12秒で成功。

kaspadの `cargo test --locked --offline -p kaspad --features private-integration-config --lib palw_tb1_holder -- --test-threads=1` が9件成功（9.83秒）。holderの読取りfixtureで、最初のページにその席の要求がなく次ページに要求がある場合、ファイルが届くまで支出せず、同じ先端で後から供給すると個別指定と全バイト同じ2取引を生成した。設定を開き直しても同じ2取引になった。自動探索に釣銭を指定した設定、IDを記入し忘れた設定、異なる入力を使って同一席に個別割当を重ねた設定を拒否した。

更新後のprivateノードをビルドし、前節の `qf1_material_holder_peers.py` で保存済みscheduled-native-closeのコピーを開いた。今回は要求・応答の双方にOriginalの事前指定がない。初回・再起動とも両ノードは同一先端・DAA 9に一致し、各回で4個の指定出力が未使用、両mempoolにその支出がないことを確認した。4プロセスとも正常終了。この保存台帳に選出済みOriginalは含まれず、常駐2台で同じ仕事の要求からEnd応答の受理までを観測する条件は残る。

### 第22回　fork指定がEXEC閉鎖と同時に発効する境界

隔離MAD2の署名済みEXEC採掘意図では、準備と閉鎖の両方で同じ現行YESを必要とする。従来は各ブロックが確約する親状態のDAAでkernel契約・追加モデル区分・burn停止を評価していた。閉鎖ブロックのDAAでfork指定が発効するとき、追加モデル用YESが1ブロック余分に使える判定になっていた。各ブロックが確約する親の行と投票世代を読み、予定済みの規則は準備ヘッダーと閉鎖ヘッダーそれぞれのDAAで評価する。選択先端についての既存の照会は引き続き、その先端のDAAで回答する。

Rust 1.93.0、locked/offline、evmで、kaspa-consensus-coreの `tir_catalog_tier_reaches_a_class_through_its_bound_container_root_only` が1件成功（0.18秒）。選択親で有効な追加モデルのコンテナが次のDAAからfork指定になる場合、同じ親の行に対する追加モデル用kernel契約の検査が発効時に拒否を返した。

登録tokenizerと `MISAKA_PRIVATE_MAD2_REGISTERED_BALLOT=pre-draw MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1 MISAKA_PRIVATE_TB1_COURT_EXPIRY=clear MISAKA_PRIVATE_FOW1_PRODUCER_CONSENT=1`で `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --nocapture --test-threads=1` が1件成功（3.96秒）。初回のfixtureでは親と閉鎖が同一DAAだったため、正規のheartbeatを準備前に1個受理し、模擬時計を進めた。親DAA 3・閉鎖DAA 4を観測し、回帰試験で両者の差1を必須にした。登録済みモデルへの3署名YESをcold reopen後にも照合し、同じ署名・同じ閉鎖候補を用いて、指定がDAA 5なら採掘意図の検査に成功し、DAA 4なら追加モデル区分の不一致で拒否した。候補検査に渡すforkカタログの発効時期を変更した試験であり、BFT票の包含がforkカタログ・経済状態を原子的に更新する経路とkernel自動採用は未検証。QF1有償予約ではREVIEW_HOLDを確認した。

### 第22回　別プロセスで選択台帳から同じOriginalのEnd要求・応答を生成

登録済みQF1の短時間fixtureで、選出済みOriginalを含むDBを要求側の読取りプロセスで開いた。起動時にgenesis、consensusParamsId、選択先端とstate rootを照合し、席の鍵で選択台帳を検索してEnd要求に署名する。要求・参照がTCP配送されて収録された後、応答側は後続のDBを別プロセスで開き、席の未回答要求を検索する。そこで得たOriginal IDで外部の `<Original ID>.end` を読み、開示と参照を再構成する。両者とも検索入力は席で、Original IDを渡して対象を指定する処理はない。応答と参照が別々に収録された場合の再送に、この応答側の出力を使った。

前掲の登録tokenizer、native certificate、End、split request・answerと現在時刻のrelay genesisに、`MISAKA_PRIVATE_QF1_PROCESS_PAIR=1 MISAKA_PRIVATE_QF1_FRESH_IBD=1 MISAKA_PRIVATE_QF1_MATERIAL_REORG=within-window` を追加。Rust 1.93.0で `cargo test --locked --offline -p kaspa-consensus --features evm --lib private_registered_tb1_pending_freeze -- --ignored --test-threads=1 --nocapture` が13.38秒で成功した。要求側と応答側のPIDは41640と41642。TCP受信側のブロック受理、Pruned新規IBD・コールド再起動、A→B→Aの各枝でArchivalとの選択状態・state root・仮想UTXO全行を照合した。reorg後の再署名チェックでは、fixtureが固定していた最終Readout chunk用の要求に加え、End用の要求も元の署名条件で全バイトを照合する。

このDBは隔離consensus fixtureのcheckpointで、仕事量の一部をfixtureが供給し、手数料取引の署名とノードへの提出は親のfixtureが担う。独立した2台の常駐kaspad holderが同じ仕事で、探索・支払い・提出・相手での受理まで自動実行する条件は残る。

### 第22回　要求側と応答側の各プロセスで手数料取引に署名

前節の読取りプロセスが、それぞれ開いたcheckpointの選択UTXOを使い、資料と決済参照を運ぶ2取引にも署名する。要求側にはfixtureが指定した2入力を渡す。応答側には先に別々に収録された2取引の元入力を渡し、本人が発見した要求のブロック以降で、同じ開示・参照を払った直接の釣銭を照合する。役割ごとに異なるfixture支払鍵を使い、出力の所有者、未担保、非coinbase、残高を検査する。署名済み取引を受け取った親は選択UTXOから各手数料が2,000,000 sompiで釣銭が元のscriptへ戻ることを検査して、そのまま提出する。

前節と同じ登録tokenizer・split request/answer・relay genesisの短時間fixtureで、process pair・fresh IBD・within-window reorgを有効にした `private_registered_tb1_pending_freeze` が13.13秒で成功した。要求と応答で同じOriginalを発見し、それぞれの署名済み2取引がTCPで相手のmempoolに届き、テンプレートに収録され、双方で選択された。PrunedのIBD・再起動・A→B→Aで選択状態とstate root、UTXO全行をArchivalと照合した。従来の単一プロセスの署名入口も同じテストを別実行し12.84秒で成功。各プロセスの署名は隔離fixtureの固定手数料方式を使う。常駐kaspad holderの手数料見積り・自動提出・受理を2台で一続きに観測する条件は残る。

### 第22回　常駐holderと同じ手数料計算でEnd要求・応答を配送

kaspadの有償carrier署名処理とholderの手数料上限・支払鍵照合をkaspa-pq-validator-coreに移し、常駐holderと前節の2プロセスが同じ関数を使用する。署名と同じ長さのscriptでcompute massを見積もり、relay手数料を差し引いて元のscriptに釣銭を戻す。試験の親は、各署名済み取引の入力UTXOと釣銭から実額を読み、完成した取引のcompute massから再計算した額と照合した。

前節と同じprocess pair・fresh IBD・within-window reorgの `private_registered_tb1_pending_freeze` が13.18秒で成功。徴収額はEnd要求293,475 sompi、要求の参照250,000 sompi、End応答250,000 sompi、応答の参照250,000 sompiで、いずれも指定上限2,000,000 sompi以内。受信側が取引を収録し、Prunedの新規IBD・再起動・A→B→Aで選択状態・state root・仮想UTXO全行を照合した。Rust 1.93.0、locked/offlineで `cargo test -p kaspad --features private-integration-config --lib` のフィルタ `palw_tb1_holder`（9件）、`palw_carrier_fees`（5件）、`a_proof_replaces_only_a_lane_carrier_and_pays_above_it`（1件）も成功。署名プロセスはfixtureが起動し、親が各ペアを送信入口に渡した。常駐2台がそれぞれのサービスループから自動提出して受理される条件は残る。

### 第22回　QF1有償ペアの片方だけが収録されたときの再送

常駐holderでOriginalを指定して `retry_from_change` を有効にしたとき、従来は手数料入力が2つとも消費済みになるまで再送を組み立てられなかった。片方だけが選択枝で収録された場合も、使われた入力は同じ資料または参照を載せた取引の直接の釣銭、残りは元の未使用入力と照合し、2取引を組み立て直す。対象のOriginalまたは要求ブロック以降の直近32ブロックで照合できる範囲に限る。再送の入力は従来どおり支払鍵・手数料上限・担保・成熟条件を通す。

kaspadの `palw_tb1_holder` 9件成功（9.08秒）。要求と応答の両方についてどちらの入力が先に使われても正しい組み合わせを選び、別ペイロードで使われた元入力や既に消費された釣銭からの再送を拒否した。前節のprocess pair・fresh IBD・within-window reorgの `private_registered_tb1_pending_freeze` で `MISAKA_PRIVATE_QF1_SPLIT_ANSWER=reply-only` を設定し13.38秒で成功。End応答だけを収録後に応答側プロセスが台帳を開き、応答の直接の釣銭と未使用の参照入力を使った2取引がTCPで受理された。各250,000 sompiで、台帳とstate root、仮想UTXO全行がPrunedの新規IBD・再起動・A→B→A後もArchivalと一致した。両取引を別々に先行収録する従来設定も13.28秒で成功。常駐2台の自動提出を通した実証は残る。

### 第22回　QF1署名プロセスからの直接提出と収録ブロックの照合

隔離fixtureのEnd要求側と応答側を順に別のOSプロセスで起動する。各プロセスは自身の選択台帳から対象と手数料入力を読み、署名した資料・参照の2取引を自身のノードから提出する。TCP受信側は両取引をmempoolで観測して通常のテンプレートからブロックを作り、受理して通知する。送信側も両取引が同じ選択ブロックに入ったことを確認する。後続のPruned IBD・再起動・A→B→Aでは受信側が作ったそのブロックを使用した。

前節の private_registered_tb1_pending_freeze で process pair、fresh IBD、within-window reorg を有効にし、応答のみ先行収録する reply-only が13.72秒、応答と参照を別々に先行収録する設定が13.56秒で成功した。要求側と応答側の起動タイミングはfixtureが指定する。2台の常駐kaspadのサービスループが同じOriginalから自動で要求・応答・受理へ進むことは引き続き未検証。

### 第22回　先行収録のないEnd応答を独立プロセスから直接提出

前節の独立プロセス試験に、応答も参照もまだ支払っていない状態からの初回提出を追加した。応答側が選択台帳から未回答の要求を発見し、保持しているEnd資料と2つの未使用入力で回答する。受信側が両取引を同じブロックに収録し、要求が解消したことと認証済みEndを確認した。手数料は応答と参照で各250,000 sompi。新規Pruned IBD、再起動、A→B→Aの後にも、Archivalと選択状態・state root・仮想UTXO全行が一致した。

前節の private_registered_tb1_pending_freeze の設定から MISAKA_PRIVATE_QF1_SPLIT_ANSWER だけを外し、13.59秒で成功した。reply-only を設定した片側収録済みの再送も再実行して成功。プロセス起動はfixtureが担い、手数料はfixtureが用意した入力ごとの鍵で支払う。常駐holderの自動起動・検出・署名・提出を2台で通す条件は残る。

### 第22回　各holderの単一支払鍵でEnd要求・応答を配送

常駐holderの資金設定では1つの ready-fee.seed が資料取引と参照取引の両方を支払う。独立プロセス試験でも、通常の署名済み送金をmempool経由でブロックに収録し、要求側の鍵へ2出力、別の応答側の鍵へ2出力を作るようにした。各プロセスにその役割のoutpointを明示し、2入力の所有者、選択UTXO、担保ロック、手数料と釣銭を検査する。片方または両方を先行収録する場合も同じ送金の出力から始める。

前節と同じ process pair・fresh IBD・within-window reorg の private_registered_tb1_pending_freeze で、MISAKA_PRIVATE_QF1_SPLIT_ANSWER を未設定、reply-only、1とする3通りが成功。初回応答、片方の釣銭からの再送、両方の釣銭からの再送でそれぞれTCP受理とPruned側の選択状態・state root・UTXO一致を確認した。両役割のプロセスはfixtureが順番に起動し、署名鍵はテスト用の決定論的な鍵を使う。常駐kaspadのサービスループによる2台同時の要求発見・応答・収録は引き続き未検証。

### 第22回　常駐holder 2プロセスの自動要求・応答とTCP収録

隔離fixtureの同一選択台帳から、要求側・応答側それぞれに独立したDB、ConsensusManager、mempool、TCP接続、holderディレクトリを用意した。各プロセスがkaspadの ConfiguredHolder と HolderService を起動し、自身の ready-fees.json と ready-fee.seed を読み込む。要求側には当番鍵、応答側にはactorと保存済みEnd資料を渡す。初回配送ではどちらの設定にも対象Originalを指定しない。両サービスの接続後に、別の受信ピアがmempoolに届いた要求・参照を収録する。応答側はこの選択ブロックから要求を発見し、End応答・参照を自動提出する。両サービスが応答までの2ブロックを選択してから停止した。

Rust 1.93.0、locked/offline、前節と同じ登録tokenizer・ビルド設定で private_registered_tb1_pending_freeze を実行した。MISAKA_PRIVATE_TB1_NATIVE_CERTIFICATE=1、MISAKA_PRIVATE_TB1_END_CASE=1、MISAKA_PRIVATE_QF1_SPLIT_REQUEST=1、MISAKA_PRIVATE_QF1_PROCESS_PAIR=1、MISAKA_PRIVATE_QF1_FRESH_IBD=1、MISAKA_PRIVATE_QF1_MATERIAL_REORG=within-window とし、MISAKA_PRIVATE_QF1_RELAY_GENESIS_MS は起動時の現在時刻（ミリ秒）とした。

- MISAKA_PRIVATE_QF1_SPLIT_ANSWER 未設定で初回配送が13.61秒で成功（PID 44824・44825）。要求と参照の手数料は293,475・250,000 sompi、応答と参照は各250,000 sompi。同じOriginalの要求解消とEnd資料を照合した。実際に収録された両ブロックで、Prunedの新規IBD・再起動・A→B→A後にArchivalの選択状態・state root・仮想UTXO全行と一致した。
- reply-only（13.82秒）と1（13.60秒）では先行収録後の台帳から指定Originalで常駐holderを起動し、片側または両側の釣銭から再送できた。

この試験は実際の常駐サービスをlibtestプロセスに組み込む。TCPピアと署名・収録を含む自動配送を検証した。kaspadのCLIからの2台起動・再起動は別の既存試験であり、同一の実行でCLI起動から有償応答までを通す確認は残る。MAD2のkernel採用とfork経済反映はこの試験の対象外。
