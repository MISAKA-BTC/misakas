# A6 — native結合の具体的な接続条件

2026-10-31。A6のnode-local保存・再検査・同期APIと、A0が所有するnative wire/state/funding schemaの境界。新しいglobal ID、native row prefix、P2P番号をここでは割り当てない。A0の後続commit `42a60b840` はobject 176 / aux 151と正規keyを予約している。native rowのvalue/body schema・live decoderはその時点でも未完成であり、A6のprivate archiveをaux 151の任意bytesへ偽装して開放しない。

## 実装済みの呼出し境界

- `database::palw_positive_recovery::PositiveRecoveryStore`: current archive/head/observed DAA、関連行のbefore/after、undoを一つの `WriteBatch` / `write_opt(sync=true)` に保存する。head・clock・関連行のCAS、再送、同一履歴のreorg再適用を検査する。
- `misaka_palw_positive_recovery::commit_initial`: fresh admissionに対する関連行差分を読み取り専用で準備し、一回の保存取引に渡す。
- `JournaledCoordinatorV2::prepare_record/prepare_clock` → `commit_prepared`: 旧runtimeを消費し、実Node checkerと累積予算を再実行した次stateからだけ関連行差分を作る。現在のnative時計と検査時計が違えばproject前に拒否する。
- `restore_and_commit`: 信頼済みheadと全eventを検証して再起動する。clock由来のexpiry/返金/予約解放を一つの保存取引で確定してからruntimeを返す。`restore_selected` は読み取り専用previewであり、native副作用の公開入口ではない。
- `RecoveryChunkImportV2` → `CheckedRecoveryImportV2` → `publish_import`: 有界搬送・再読込み・全計算再検査を終えたtokenのみDBへ公開できる。source/transportのhash一致だけでは公開しない。
- `RecoveryRetentionV2` → `prune_retired`: 同じscope/head/clockの再検査済み置換資料と全horizonを確認してから、旧undo/receiptだけを有界に削除する。現在archive・現在head・関連行・他caseは保持する。

`kaspad::palw_positive_recovery` は上記Node向けAPIを登録する。native dispatcher、public preset、network flag、Wire variantは追加していない。

## A0/A4/A5が供給する必要のあるauthority

### 1. 同一native execution viewへのanchor

A0所有の具体的なTrustedAnchorは、少なくとも次を同じ選択履歴へ結び付ける必要がある。

- network、branch、ruleset、case、署名済みresult、登録class/artifactと入力/依存
- 現在のselected native state rootおよびstate generation。DAAが同じでもreorgや状態置換を検出すること
- 期待するjournal headとadmission DAA、固定されたrole key / drawn roster / policy
- 全eventのcanonical carrier、accepted DAA、同一DAA内も含む正規event順序

`RecoveryContextV2::resolve` はそのviewからfresh admissionと期待headを解決する。`authenticate` はeventの実内容・所属・accepted DAA・正規順序を検証する。peerと一緒に届いたorigin、保存されたcursor/Pass、DBにあるheadだけをauthorityにしない。現在のprivate result/witnessを新contract DTOへaliasしない。

### 2. 同じexclusive writer guardとread-only projection

native wrapperはanchor解決・再検査・現在branch/generation/clockの再確認・関連行の準備・同期commitまで、既存chain-stateのexclusive writer guardを保持する。一般の直接DBwriterもこのguardへ従う必要がある。

`project(checked_state)` は **差分の準備だけ** を行う。そこでcoin hold、支払、資源公開、cache更新を先に実行しない。A4/A5の現在viewを検証し、次の行を `RelatedRow { key, expected, value }` として返す。

- 新resultに結び付くcase/admission/role liability行
- A4が認めた全positive/fraud資源・全候補分・搬送/保持/復旧費用の予約行
- A5が実原資とpayer署名consentを検証したhold/payment/refund/burn/settled service dedupe行
- native側のstate generationおよび選択されたjournal headとの対応行

A6はopaqueな行差分をCAS付きで保存するが、coin規則・署名scope・有罪判定を独自に作らない。実UTXOやpayer consentを検証するnative projectorの成功実装は本branchにない。私設fee meterをnative入金に変換するdefault実装もない。

同じDAAの別branch/generationは時刻closureだけでは識別できないため、projectorがnative viewを再検証して対応する行CASを必ず含める。network信用には別途現在のGate C等が必要であり、ローカルexact一致・署名・保存成功からcreditを作らない。

### 3. reorgとIBDへの登録

native reorg undoの中で、`PositiveRecoveryStore::rollback(case, transaction)` を関連行と同じjournalとして呼ぶ。逆順・別case更新・異なるbefore-imageへの上書きは拒否する。reorg後は旧runtimeを捨て、新しいTrustedAnchorから再検査する。

現在のPALW row-stream/state rootとprotocol106/107 snapshotにはPositive履歴collectionがない。A0が所有するversioned native schemaとIBD dispatcherへ、上記bounded importerとchecked publicationを接続する必要がある。旧carriageのrootをPositive計算検査として流用しない。既存pruning-point trustと自己再検査を区別し、資料喪失時は未検証のままにする。

### 4. 保存・再実行費用の予約

現実装はwhole-history再実行・再encodeであり、入力量Nに対する累積費用が二次になり得る。`quote_update` と `quote_admission` は繰返し再実行のrecords/bytes、全候補のelements/terms、archive/undo/WriteBatch用の論理buffer量を返す。native受付はこれらをA4のquoteに **追加して** 全claim/block/epoch容量へ予約する必要がある。

論理archive bufferのquoteには余裕を含む。decoded heap、関連native行、allocator、RocksDB cache/WAL/compaction、既存世代、SpillArea、ネットワークsessionのメモリ/保存量は別の上限として合計する。quoteは実測時間・RSS・30GB footprint保証ではない。未測定native profileを有効にする根拠にしない。

## 完了状態の区別

A6のNode-local adapter、実RocksDB・実TIR・実process終了・二つの独立DB間importの検証と、native合意経路での受理・実UTXO精算・全Node同期は別である。後者のdispatcher/具体authority/schemaが未接続な間、Gate Cやnative activationを成功にしない。
