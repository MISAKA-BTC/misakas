# TFO3三行保存の局所undo

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前ターンは`aa0e21ae3`の三行保存をコミットしたprogress。今回も実コードを変更した。Studio同一claimの公開G14受付・Final・当選権利消費・対応blockは **NOT_PROVEN**。

## 今回の接続

[TFO3三行writer](../../../../consensus/core/src/palw_kernel_route_fold_v1.rs)から全stateのrollbackコピーを除き、[専用inverse](../../../../consensus/core/src/palw_state_v2/tfp1_original_inverse_v1.rs)へ接続した。元original keyとclaim key、開始journal index、固定64 bytesのlookup bufferだけを保持する。書込み前に三行とも存在しないことを確認する。

失敗時には、追加journalが198→204→206の最大三件の新規挿入prefixであり、key・現在の値がjournalと一致し、未記録の対象行がないことを先に全件照合する。その後にjournalのowned keyを移して逆順に削除する。rollback中に新しい行やencoded payloadを割り当てず、無関係なstateを複製・走査しない。想定外の書込みは部分的に消さずlocal resource faultとし、外側候補の破棄を要求する。

中央writerでは三tableの派生index変更を拒否するguardを追加した。現行のGC・execution cohort・compact root・monetary member・Positive aux indexは対象外だが、将来それらを更新する変更が入る場合、inverseも明示拡張する必要がある。旧wire・table番号・行bytesは変えていない。

個々のwriterの予約と、失敗後も所有するjournal bufferの予約はbuilder破棄まで保守的に維持する。これは実メモリを残したまま再試行に容量を返すことを防ぐ。lookupのscratch leaseは局所ownerとともに解放する。全foldの時間・メモリ上限やclaim/frame/book/escrowの外側checkpointまで局所undo化したわけではない。

## 回帰コードと検査範囲

[実writer fixture](../../../../consensus/core/src/palw_state_v2/tests/native_writers_v1.rs)で、3行成功時の追加予約量と最初の行を書いた後の拒否量を記録する回帰を追加した。成功予約量より1 byte少ない枠で途中拒否・全state/journal復元を検査し、無関係な払出行32件を増やしてもwriterの追加予約量が同じことを検査する。これはwriter開始後の増分だけの検査で、prefix構築・root計算やプロセスpeakの測定ではない。prefixは明示fixtureであり、別rootを持つ本物の受付の成功を示さない。

専用inverseの回帰では0〜3件の各prefixの巻戻し、既存行拒否、順序・key・値・old行の不整合、journalにない対象行を検査する。不整合はどの行も削除する前に拒否する。raw行を注入するinverse単体fixtureは有効なTFO3受付ではない。**これらはWRITTEN_NOT_RUN**。Rust実行は[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の通し実装後に行う。

## 完全受付の予約計算について確認した事項

独立したコード・仕様読解で以下を区別した。未確定値をfixtureから埋めて公開profileを有効化しない。

| 入力 | 現在の根拠・不足 |
| --- | --- |
| notice/hard | native court定数の3,120/10,000 DAA。残余は差分から導く |
| network、class/container/TKR、既存予約帳 | native state/paramsから取得可能 |
| 初回draw、cold取得、全体検査、最初のnotice/資料要求 | 新profileの実予約と有限の時間・資源budgetへの接続が不足 |
| 資料取得後の再検査・局所化・提出・裁定・回復 | 各工程の費用、最遅手番、同時需要を含む完全graphが不足 |
| manifest | 固定body 2,104 bytes/10 scopes。native冷開始128 bytes・依存68 bytes・ordinary rules512 bytes・HF descriptor等の部品はあるが、productionの全scope codec/上限/保持・供給budgetへの統合が不足 |
| aggregate budget | compiled graphのwork/bytes等は利用可能。unique bytesの重複排除、実fee/escrow、利用実績・全枝・保持・試行回数への帰属が不足 |
| outsider/defense | 第三者の資格・bond・fee・順序、防御者と対象caseの対応が必要。drawn graphの単なるrank違い複製では実装にならない |
| ChainScope.branch | 署名前に固定できるanchorのnativeな所属・鮮度照合が必要。未知の受理blockや常に変わる最新parentを署名値にしない。206の実受理parentとは別責務 |

旧Batchのestimateは適用範囲が一致する処理だけに使える。新profile全体の測定や公開成功の根拠にしない。残件は完全profile resolver、実費・全枝共同予約、quoteと受付の接続、Nodeの全prefix候補filter、PMR継続からPanel・Final・権利消費・blockまでの公開検証。配備・送信・公開probeは行っていない。
