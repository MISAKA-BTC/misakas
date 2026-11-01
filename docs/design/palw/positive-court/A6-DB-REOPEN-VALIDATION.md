# A6 — durable journal data と actual local replay の結合候補

2026-11-01。code候補は `f8d3bd3fa` と `355408e5a`（HEAD `355408e5a3426926d863193b76ffaa810704bef0`）。本記録の時点では **追加6件は未実行** であり、成功件数は0。A0/A4の中央検証と競合しないようheavy buildを保留している。rustfmt・差分検査・offline Cargo metadata解決・独立read-only reviewは完了した。旧候補の38件・53件を本候補の成功へ加算しない。

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
