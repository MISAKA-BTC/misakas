# A6 — durable journal data と actual local replay の結合候補

2026-11-01のcode候補は `f8d3bd3fa` と `355408e5a`（runtime source `355408e5a3426926d863193b76ffaa810704bef0`）。初回owner runは依存cdylibのlink errorでbuild exit101となり停止した。**追加6件を含む31件のruntime試験は未実行、成功件数は0**。rustfmt・差分検査・offline Cargo metadata解決・独立read-only reviewは完了したが、型検査・実行の完了とは扱わない。旧候補の38件・53件を本候補の成功へ加算しない。

## 追加する検証

`positive_court_a6_db_replay.rs` の3件は、既存の実TIR/G14/Source fixtureを使う。class/job/role/rateは引き続き明示的なlocal test contextである。

1. 実 `insert_delta_batch_tracked_v1` と `stage_positive_case_journal_batch_v2` を同じcaller WriteBatchへstageし、両ticketを同期writeの結果まで保持する。全DB/store/cache所有者の解放をstrong/Weakで確認して同pathを再openする。保存deltaをretained loaderで読み、Case/JournalHeadからpinを再構築し、実DB sourceからfresh Court/Sourceを再生する。保存前とCase・進捗・5 receipts・2 unions・5 dues・fee15・local factが同じであること、2回再生してもfeeが増えないことを検査する。
2. 再open後のmiddle/tail欠損・形が正規のtail改竄を拒否する。保存済み最新pinを保持し、fresh link検査と以前のchecked-link tokenによる再読をともに拒否する。元fixtureが同じrawを保持していてもfallbackせず、external state/effect/final/live commitを公開しない。
3. prefix/fullの両caller deltaとfuture raw tailを保持したまま、保存された古いprefix pinだけを再生する。未完了の4unit資料duty・Pending・fee0を再構築し、後続の完了やfee15を混ぜない。

native journal store側には、別の3件を追加する。異なる3 Caseでcoalesced/intermediate tailを混在させてreopen/forwardを照合し、片Caseの旧raw欠損・破損時には全caller batchが不変であること、batch放棄時にclone/fresh readerへrawやmarkerを公開しないことを検査する。

## 範囲と依存

これはclean DB close/reopenとfresh local cacheの検証である。実Node processのkill/crash再起動、IBD、native selected-headのreorg rollback、完全なaux151 bundleの復元、UTXO精算、native役割・予約・実RAM admissionの成功ではない。deltaのcaller data rootや保存headだけでnative権限を作らない。実native ownerの未供給条件は [Source replay handoff](A6-SOURCE-REPLAY-HANDOFF.md) のままである。

raw stage単独はCase/head deltaを保存しないため、typed delta writerと同じbatchで検査する。retained delta ownerはrowsの借用・再生中保持する。baseline Court/Source cacheは小さなexpected DTOを取り出して解放するが、fixture・DB・再生結果の同時memoryをengine1回のlogical quoteで予約したとは扱わない。strict resource modeでないticketの存在を、native lease実績と呼ばない。

追加依存はTIR executorのnative targetに限るdev-dependencies `kaspa-consensus` / `kaspa-database` / `rocksdb` の3件で、新package versionやlibraryの通常依存edgeを増やさない。native testのbuild graphには既存consensus/databaseを含む。逆向きのconsensus→executorも既存dev-edgeであり、full offline Cargo metadata解決を確認した。

source hashと未実行状態は [source freeze](../../../evidence/positive-court-a6-db-reopen-2026-11-01/source-freeze.json) に固定する。中央Rootで実行する場合は統合後のsourceと結果を別candidateとして記録する。

## owner実行runnerの準備（未実行）

A0との調整で、まずstack修正を追加せずowner `355408e5a` のまま既存targetを使う計画とした。新DB試験は重い`config::Params`生成や`apply_object`のfoldを呼ばず、typed deltaのserialize/retained decodeとtiny TIR/Source再生を使う。ただしderived enumのstack安全性も含めて実行は未計測であり、標準stackの初回停止を成功へ読み替えない。

[run-owner.py](../../../evidence/positive-court-a6-db-reopen-2026-11-01/run-owner.py) は既定で読み取り専用planを作る。A0が排他的なheavy実行枠を渡すまで、`--execute`、旧binaryの圧縮・削除、コンパイル、DB試験は開始しない。旧3binaryはgzip保存後に実tempfileへ復元しsize/SHAを照合してから、同一性を確認したその旧exeだけをunlinkする設計。復元先は新しいdirectoryに限定し、新しいcache成果物へ上書きしない。

build前に空き4GiB以上を必須とし、実行中512MiBを割ったら自身のprocess groupを停止する。Cargo親が先に終了しても子が残る場合を検出する。SIGTERMを無視する子を残して親が終了する軽量process-control回帰は1件成功したが、これはDB試験やnative予約の検証件数ではない。compiler/testの失敗後には同run directoryで再試行せず、stackも増やさない。compiler開始前の容量停止だけは、既存backupのSHAと同じplanを確認した上で明示的に再開でき、以前の停止記録を残す。

旧binaryの保全だけを別枠で行う場合は `--preserve-only` でcompiler/testを呼ばずに止められる。これも現在は未実行であり、heavy枠の調整前に自動では行わない。その後の実行は同planとbackupを確認してから行い、4GiB条件を維持する。

初回runnerと一致する計画は [owner run plan r2](../../../evidence/positive-court-a6-db-reopen-2026-11-01/owner-run-plan-r2.json)。r1も以前のrunner digestを持つ原記録として変更せず保持し、局所link修正後の新計画は後述のr3へ分ける。

native libraryはRootの実検証がpinしたRFC14 completion release archiveを使う。RocksDBは23,048,496 bytes、Snappyは46,784 bytesで、正確なpath/SHA、同一bindingsとarm64/libc++・default compression featureの照合を [archive review](../../../evidence/positive-court-a6-db-reopen-2026-11-01/external-native-archive-read-only-review.json) に固定した。最初に調べた別の104MiB候補とは区別する。両external `LIB_DIR` と `STATIC=1` / `COMPILE=0` を指定し、追加C++ compiler呼出を禁止する。外部archiveではlibc++自動linkも消えるため、global RUSTFLAGSを変えず、単一のfinal test targetへだけ `cargo rustc ... --profile test -- -l c++` を渡し、JSONが返したexeを直接実行する。

対象はnative store/readerの14件とactual TIR common replayの17件、計31件を予定する。まだ1件も実行していない。source/runtime owner結果とRoot source結果、clean DB reopenと実Node再起動/IBD/native authorityの資格を分ける。

旧EVM consensus library候補はembedded metadataがmain Downloads checkoutを指すためA6所有と扱わず、削除しない。A6所有と確認できる旧Core853 pairについても [read-only cache plan](../../../evidence/positive-court-a6-db-reopen-2026-11-01/obsolete-rust-cache-read-only-plan.json) を返しただけで、まだ削除していない。

## 初回owner run — build停止、runtime 0件

A0の専用枠割当後、`--preserve-only` で旧3exeをgzip保存し、実tempfileへの復元・size/SHA一致を検査した。722,180,536 bytesを194,621,101 gzip bytesで保持し、元のexact exeだけをunlinkした。さらにA0が許可した旧Core853のrlib/rmeta計482,185,893 bytesだけを、所有source・stat/SHA・lsof未使用・他Cargoなしを再確認して削除した。他owner/main由来3 EVM pair、current Core aa8/ca2、C++ archive、raw、モデルは変更しない。空き4.54GiBで実行開始条件を満たした。

固定runtime source355、runner `9776b83dd`、標準stack、exact release archive、2jobs/opt0/debug0の初回buildは `kaspa-wrpc-server` の依存cdylib linkで停止した。外部RocksDB/Snappy指定によってsys build scriptのlibc++ link指定が失われ、final test target限定の `-l c++` では中間cdylibへ届かなかった。これはlink設定の失敗であり、archive ABI不一致や新試験のRust型検査成功を意味しない。Cargo exit101、test未起動、runner停止・own PG消滅を確認し、同run-dirで再試行もstack増加も行っていない。

836,209-byteの原build log、status/plan、exact旧runner、旧exe保全manifest、Core2files削除map、開始PID訂正前後は [owner run 1](../../../evidence/positive-court-a6-db-reopen-2026-11-01/owner-run-1/) に固定した。compressed exe本体は同manifestが指すprivate tmpに保持し、原raw記録を書き換えない。

## 局所link修正案 — 未実行の別run

[native links override](../../../evidence/positive-court-a6-db-reopen-2026-11-01/native-links-r2.toml) はCargo標準の `target.aarch64-apple-darwin.rocksdb` 設定だけを使う。sysのnative link metadataへstatic RocksDB/Snappyとdylib libc++を入れ、中間cdylibへも伝播させる。sysのbuild scriptを省略し、SHA固定のrelease `bindings.rs` を `rustc-env.OUT_DIR` で使う。元のcustom metadata `out_dir` と `cargo_manifest_dir` を別に保持し、他codec sys cratesの通常link経路を変更しない。

global RUSTFLAGS、新しいtarget directory、不可視wrapper、Rust source/料金/schemaの変更は無い。overrideのBuildOutputはCargo fingerprint対象だが外部入力bytesは別途検査が必要なため、archive2本とbindings・設定fileをplanへ固定し、前後SHA照合を行う。sysと逆依存Rustの再buildはあり得る。[read-only根拠](../../../evidence/positive-court-a6-db-reopen-2026-11-01/native-links-r2-read-only-review.json) は現Cargoの実装・公式仕様との照合で、実link成功の証拠ではない。

次候補は [owner plan r3](../../../evidence/positive-court-a6-db-reopen-2026-11-01/owner-run-plan-r3.json)。初回のverified gzipをCRC・uncompressed size/SHAも含め再照合し、**別run directory** の新manifestから参照する。旧pathを再削除せず、初回結果を成功へ加算しない。Rootのheavy枠releaseまでコンパイルを再開しない。
