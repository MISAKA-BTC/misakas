# A6 — 共通case journal・actual checker再生・native材料保存

本記録は `codex/rfc42-a6-common-journal` の第1候補を固定する。旧 `codex/rfc42-a6-recovery` は保持し、旧private archiveを新Courtへaliasしていない。A0指定base `815b7ca1c` にnarrow/full-context/future-frame修正と正式 [NATIVE-JOURNAL-R1.md](NATIVE-JOURNAL-R1.md) を取り込んだ。

実装は `5c30f499c`（codec・scanner・chain・actual A1/A4 local replay）、`830cd110d`（caller-owned native DB batch・actual accepted-data reader）、comment-only追補 `947ee98f7`。直接Borshのaux151降順拒否は `a8c6c505b`、Route manual reviewも新hash `1b6dce…` に更新した。

## この候補で確認した範囲

**対象38件成功 / 最終失敗0 / ignored0**。shared codec9、core wire/link9、直接Borsh1、実TIR local replay8、native store/reader11。旧native61/route25や前候補30、他担当の件数は加算しない。core・native nodeは検証用opt0/debug0/incremental0/jobs2、stackは標準。core/consensusのlibrary型検査も成功した。rawログ、source hash、再実行条件は [common-journal evidence](../../../evidence/positive-court-a6-common-journal-2026-11-01/) に保存する。

最初のcore testではA0 baseのaccounting dev依存不足、次のcore checkではgetter重複、実TIR試験では後述のreadonly callback件数assertを修正した。native nodeのdefault opt2試行は実行前に中断し、opt0/debug0候補の11成功だけを数える。過去の失敗・中断ログも別名で保持する。

## codecとリンク

shared leafのborrowed scanはpayload/aux件数・個別/総bytes・追加owned heapをchecked arithmeticで制限し、strict aux参照順、重複、EOF、初期sequence1/previousZEROを確認する。head/historyは同じfull recordを正式採択の別keyでhashし、history.originはrecord.origin。Python stdlibだけで生成した独立vector3件と域分離を照合した。未知kindのBorshエラー生成がallocationを起こす点はraw tag先行拒否へ修正し、成功・攻撃入力の検査区間allocation0を確認した。

coreのexplicit-limits stream scannerは長さを先に検査し、短いeventにもrecord inline・固定workspace・aux要素を課金する。limits無しのgeneric scanはread0でUnsupported。型付きheap訪問はfield/variantを網羅する。全履歴をVec&lt;巨大enum&gt;へ復元せず、chain readerはDigest indexだけを保持する。

headからpreviousを辿る検査と前方の再取得検査を行い、source欠落・invalid identity・local capacityを区別する。2passのread/hash/indexを予約quoteし、再取得時の改竄・短縮・追加tail・scope違い・history/head aliasを拒否する。このリンク結果はdata identity診断でありnative受理やcredit能力ではない。

## 実A1/A4再生

実 `TirCourtCheckerV2`／`PreparedCheck`／`ContractCourtV2` でResult→W/record→全frame→Finishを再実行する。saved cursor/Matched/errorを状態へ注入しない。各eventでnative-owned historical guardを固定し、exact aux参照・原preimage・native受理・canonical性・state/cost-changing inclusionを必須portで再確認する。完全NoOp、future-positionや認証前拒否を新journal eventへしない。現在のcodecはbody op0..7のみで、未採択Assignment op9をop0へ偽装して受理しない。

実算術Pass／正真正銘のoutput mismatch／Panel fault、失敗時の費用保持、record/relation欠落、role失効、clock/generation変化、native port未供給拒否を検査した。末尾材料がリンク検査後に失われると、それ以前のreadonly認証callbackは起こり得る。一方、replayerはfresh private runtimeを全件事前確認後に作り、失敗時に外部状態作成・効果適用・final公開・live funding commitを行わない。先行readonly検査回数0という過剰なassertを、この非公開契約へ訂正した。

historical stateと現在native時刻の期限評価を分け、合成zero-Tx clock originを作らない。live prepare/admission/funding commitを再呼出しせず、結果はlocal checked factと診断だけを返す。actual native role/source/funding/qualified settlementやcurrent-root final照合を解決するenvironmentは成功default無しであり、欠ければErr。新receipt復旧・bindはA4/A5のgenuine readonly factoryと別である。

logical quoteはRAM pool ticketではない。native session ownerはsource backing・index・body・facts/診断・private heapを含むmax_known_buffer_bytesとworkの実leaseを事前に保ち、出力や取り出したpartsの常駐中も所有する必要がある。

## native caller batchと資料reader

`DbPalwStateV2Store::stage_positive_case_journal_batch_v2` は既存PALW writeguard下、caller所有WriteBatchへだけstageする。raw材料keyは `[PalwStateV2Tip=223,0x4A,eventHead64]`、既存singleton `[223]`／identity `[223,0x49]` と非衝突。head/history・scope/case/result・sequence/previous・初期op0・Case/JournalHeadのold/new deltaとcoalesced/intermediate遷移を照合する。最初の旧headのrawも実DBから読み、欠損・破損を拒否する。

全codec/CAS/既存bytes/quote確認後にだけbatchへputするため、エラー時の既存batch bytesは不変。ticketを同じnative per-block DB writeまたは放棄まで保持し、native aux151とUTXOは既存typed store経路で同batch確定する。Raw storeは別commitを発行せず、typed UTXOをraw keyで書かない。append-only材料はroot reorg/reindex後も保守保持し、現在Case headがその生存authorityとなる。GC・IBD材料提供は未実装。DBのretained_bytes quoteは同時LOCAL memoryであり、Caseのcanonical retention/feeへ足す値ではない。

実RocksDB batchの公開前不可視・sync commit後の再起動・marker/root rollback・raw保守保持・旧材料欠落・stage失敗時batch不変を検査した。raw pinned readerは長さだけを制限し、両identityの再hashはchain層が事前予算下で行う（隠れた追加hashをSourceに入れない）。

accepted-data readerは実selected-chain readguard、header再hashとclock、AcceptanceDataの全accepted ordinal（coinbase含む）、carrier body/Merkle root、index、fresh TxIDを照合する。欠損を空やskipにしない。native Transactionのcanonical raw bytesはwriterへ逐次比較し、mass/idを含む全bytesとEOFを検査する。Merkle workspaceはtyped slot数から別予約する。通常block-store cache/intakeとcallbackのclone費用、PALW/pruning leaseはnative ownerが別途所有する。

このreaderはAcceptanceData全体のaccepted-id-root再構築やbody176/role/opportunity/qualified spend検査を提供しない。実native ownerのaccepted prefix・原資料・UTXO・scope/epoch/generation・料金/受取人・continuation検査が必須である。raw cache／aux151 root／保存されたpaid・Matchedからnative admission、terminal、payment、Gate Cを作らない。

## 未達と後続

full native restart／実資金精算／T12／Node間P2P・IBD／公開activationは成功としていない。新job authority・未対応Assignment codec・actual native environmentやqualified spend接続が無ければ拒否を維持する。既存NumericalManifestのgenerator PIN不一致は別baselineであり、全generator roundtrip成功とは扱わない。

A0から追加依頼された実A2 SourceLedgerを使うExternalState結合は後続候補で検証する。本38件の成功をその新候補へ流用・合算しない。物理RSS/30GB footprintや検出率・参加率を仮定なしに保証しない。暗号学的なモデル計算証明を用いず、モデル二階層の凍結とclaim単位のGate Cを維持する。
