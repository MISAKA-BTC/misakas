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

`RecoveryContextV2::resolve` はそのviewからfresh admissionと期待headを解決する。`authenticate` はeventの実内容・所属・accepted DAA・正規順序を検証する。peerと一緒に届いたorigin、保存されたcursor/Pass、DBにあるheadだけをauthorityにしない。unkeyed checksum列やgeneration directoryの末尾から期待headを自動選択しない。短い正常prefix・余分なchecksum-valid tailも保護されたheadと違えば拒否する。headが一致していても、全eventの正規origin/順序とcheckerの再認証を省略しない。準備済みunit/sizeや保存meterを将来のquota割引へ復元する入口は設けない。現在のprivate result/witnessを新contract DTOへaliasしない。

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

## 2026-10-31 aux151 接続追補

A0 の正規 VALUE schema (`327393e8e`) を取り込み、`NativeWireScanV1`、`NativeHeapV1`、native carriage consistency と `from_served_rows_v1` へ登録した。従来の「native schema がない」という記述は本追補で更新する。public preset・carrier dispatcher・実 UTXO/funding 授権は引き続き別の接続責務である。

- aux は **151**、共通 key は最大142 bytes、VALUE は最大16,384 bytes。10 kind の version/revision、enum/Option tag、key/value kind・ID、余剰・切詰めを検査する。新しい ID は割り当てない。P 認証 row は A0 所有の別 table136 であり aux151 に混ぜない。
- wire scanner は固定 buffer を使い、BTreeMap へ集約する前に Positive key の重複・逆順を拒否する。直接 Borsh decode でも重複 aux key と aux151 の上限・局所整合を拒否する。
- 短い ClaimIndex も `size_of::<NativeRowValue>()` に加え、固定 decode workspace と attempt-index の node envelope を全 entry に課金する。raw Vec の費用と別に計上し、不正 row に割引を与えない。共有 hash は同一 Borsh stream を逐次入力し、preflight のための Vec を作らない。
- 復元では全10 kind の双方向参照、元 admission DAA における Miner/payer/Panel 署名、coverage/plan、case+attempt 一意性、資料三状態、receipt literal 数と union completed 数、PacketReceipt author/range を検査する。初期 Result/ClaimIndex/Case/Funding と JournalHead は同時に必要であり、head 欠落は拒否する。
- native JournalHead は ResultAdmission を含む初期 sequence=1、previous=ZERO。初期 history origin は Result.origin、全 sequence の history origin は Case.last_event と一致する。private archive の初期 records=0 と alias しない。sequence>1 の previous/head/history digest の preimage・正規 domain・accepted prefix は、実 native history backend が別途再構築する。構造整合と root membership だけで履歴再実行成功や credit を認めない。
- 復元は `O(N log N)`、全 Union/Receipt の prefix 走査は合計1回ずつ、補助 index は Witness 数以下。署名再認証は Case 当たり2回、Witness 当たり1回。report は計数 DTO であり、実 native budget/receipt/credit capability ではない。

A6 の `PositiveRecoveryStore::rollback` は選択履歴と関連 native 行の transaction undo に用いる。A2 の local spent I/O budget は branch/reorg で巻き戻さない別寿命の永続領域に置き、この undo の RelatedRow に含めない。chain request/accepted-response の undo と local traffic の消費を同一視しない。

検証と残る境界は [A6-AUX151-VALIDATION.md](A6-AUX151-VALIDATION.md) を参照。

## 2026-10-31 native writer batchへの接続案

A4との照合では、native aux151は既存の `PalwDeltaEntryV2::KernelRouteRow { table, key, old, new }` で差分・before-imageを保持できる。`VirtualProcessor::commit_utxo_state` は `palw_state_v2_store.write()` guardを取り、`insert_delta_batch_tracked_v1(&mut batch, current, root, delta)` のticketとguardを、typed UTXO/MuHash/acceptanceと同じDB writeまで保持している。まずCourt/A2/A5のnative行差分をこの既存PALW deltaへまとめる。A6 private storeのRelatedRow raw keyでUTXOやtyped native storeを直接書き換えない。

A2 accepted-material preimageが別namespaceを必要とする場合は、具体的な共通history schemaの確定後、native typed material-storeのcaller-owned batch準備APIをこのguard下へ接続する。prepareはbounded encode/CAS/quotaを済ませ、caller batchの確定前に新native cache/terminalを公開しない。private archiveの型を共通historyへaliasしない。

private再開cacheまで同じbatchへ同梱する必要が生じる場合のport案は、A6 DB-wide mutexを所有するscoped `RecoveryBatchGuard` がstage_commit/rollbackで準備した差分とundoをcaller batchへ追加し、実DB write終了まで生存する形である。これは未実装の提案であり、既存commit APIの能力として扱わない。native exclusive guard→A6 guardの順序を統一し、準備失敗時caller batchを変更せず、private metadata以外をこのportから操作させない。native aux151は既存PALW delta経路を優先し、独立WriteBatchでnative fundingとjournalを二重commitしない。

追加レビューの順序・counter・receipt条件は [A6-ORDER-COUNTERS-VALIDATION.md](A6-ORDER-COUNTERS-VALIDATION.md) を参照する。
