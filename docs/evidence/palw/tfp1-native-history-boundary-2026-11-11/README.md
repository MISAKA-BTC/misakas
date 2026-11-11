# TFO3のnative履歴境界と有界reader

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前ターンは`3302fe592`でpaired originalの再照合と有料資料caseの継続条件を変更しており、progressと分類する。今回も[元計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の通し実装を継続する。Studioの同一claimの公開受付・Final・当選時権利消費・対応blockは **NOT_PROVEN**。

## 保存と復元

[新しいhistory行](../../../../consensus/core/src/palw_tfp1_original_history_v1.rs)はaux 206、固定686 bytes、claim keyとする。元のSource/OriginalId/AcceptedOrigin、selected parent、実行直前のstate rootとjournal位置、順序付きpopulated UTXO入力のdigest/count、実fee、元admission入力のdigestを保持する。後者はjob/class、manifest scope規則、raw notice/case入力、supply/rescue、usage/budgetを含み、198の導出済み期限だけでは区別できないreadiness入力も固定する。これはデータ同一性の記録であり、正しい予約・支払・計算をそれ自体で認定しない。digestから入力のpreimageを復元できるという意味でもない。

[prepare/restore](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/native_link.rs)と[専用writer](../../../../consensus/core/src/palw_kernel_route_fold_v1.rs)を追加した。204と206を同時に挿入し、片側だけの既存行へ後付けしない。汎用writerは206を拒否する。snapshot/deltaの最終検証も204と206の相互参照を要求し、片側だけの適用・逆適用を拒否する。旧一般PublicOriginalの行198だけにこの条件を追加したものではない。TFO3新profileは未有効化のままである。

一時rollbackは型別に計上したstate copyを所有し、失敗時は両行とjournalを戻す。全状態copyを本番の有界な局所undoへ置き換える課題は残る。行の固定幅preflight、immutable journal、挿入点照合も登録した。

[processorのreader](../../../../consensus/src/pipeline/virtual_processor/processor/positive_assignment_archive.rs)に`load_tfp1_original_carriage_v1`を追加した。呼び出し側が渡した任意boundaryではなく、current rooted 206から読む。既存の有界readerを再利用し、選択鎖・actual acceptance ordinal/carrier・元UTXO入力とFull fee・親状態・実delta prefixを確認する。元受理DAAのfunded/FPR2/FPA1 fenceとcanonical TFO3 framingを再確認する。既存のFPA1サイズ制限は変更せず、履歴側だけで通すようにしない。

このreaderをPMR受理等のproduction dispatcherから呼ぶ処理はまだない。core側でも、完全TFO3 coordinatorが実行直前位置とvalidated inputsを捕捉して新writerへ渡す呼び出しは未接続である。factoryに引数を渡せることを、実native prefixから捕捉した証拠としない。

## 静的レビューと回帰コード

独立担当がreaderを実装し、core差分を静的レビューした。FPR2→TFO3→TFO2→TFO1で走査が重なるため、追加framingをpayload最大3回分として走査前に計上した。204→206の逆向き参照検査も追加し、片側だけのjournalを拒否するよう修正した。

[native writer fixture](../../../../consensus/core/src/palw_state_v2/tests/native_writers_v1.rs)は二行保存・冪等性・容量不足・forward/revert・片側欠落・snapshot/preflightを扱う。[元claimのfixture](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original.rs)は同じ締切で異なるreadinessになる予約を通常prepareで作り、206によるrestoreが拒否する回帰を追加した。新codec回帰は別key、切断・余剰bytes、version/入力数/イベント改変、feeとUTXO生成DAA・tx改変を扱う。これらは **WRITTEN_NOT_RUN**。UTXO・prefix・資金・抽選・価格の一部はfixtureであり、公開受付の実証ではない。

## 残件

- 完全なquote/intake共通profileと全保護枠・価格・保持予算。新206や履歴取得の費用も含める。
- actual pre-object prefix/UTXO/feeのcaptureと、Frame/book/escrow/198/204/206の原子的production install。
- 元profileからadmission入力を再導出し206と照合する処理、元drawの認証済み選出一覧、pruning後も必要な有限の履歴保持。
- 新reader→PMR accepted order・料金控除・資料応答・局所court・独立承認・Final・receipt spend/blockの接続と通し検証。

モデル全量配信やHF取得可否認定は再導入していない。公開probe・配備・送信なし。Rust build/testは元計画に従い後段へ留保し、整形・差分・文書検査を実施する。
