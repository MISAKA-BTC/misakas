# A6 — actual SourceLedger / global Case usage の local replay

第2候補の実装commitは `1b7fff400`。A0指定のcommon-journal基盤に、A2実装のSourceLedger・G14 geometry・journal preparationを取り込んだ。前提commitは `f2779028e`（A0 `608f0d38d`）、`6e29c7e8a`（A0 `ec52d329f`）、`44b667891`（A2 `b4499397f`）。[第1候補の38件](A6-COMMON-JOURNAL-VALIDATION.md)とは試験範囲・候補を分ける。

最終code候補は `199ae602f`。料金分離修正 `cb4538ac4` と、実4unit fixture・失敗隔離回帰を含む。対象 **53件成功 / 最終失敗0 / ignored0**。内訳はcore helper2、A2 SourceLedger/G14 geometry37、actual TIR common replay14（既存8＋新source6）。前候補38件へ加算しない。rawログ・source hash・再実行手順は [source replay evidence](../../../evidence/positive-court-a6-source-replay-2026-11-01/) に保存する。

最初のcompileでは試験のAggregateUsage importが不足し、次の試行ではfixtureのartifact unit数を1としたassertが実geometryの4と一致しなかった。両ログを保持し、最終候補では全4artifact＋1inputを完了させた。0件だった誤ったlib filterも成功件数へ含めず、正しいintegration targetの37件を実行した。A0レビューで発見したmapperからCase feeへの混入も修正し、最終helper2件を再実行した。

## 実装境界

`CommonSourceReplayStateV2` は保存されたusage・receipt・paid状態を入力にせず、独立に得たResult・service・geometry・admission metadataからZERO usageの新しいCaseと実 `NativeSourceLedgerV2` を作る。実native ownerのhistorical guard・受理済みprefix・canonical preimage認証の内側で使うlocal projectionであり、native受付・duty・資金・creditの能力を作らない。

全eventでsequenceの連続性、直前Court snapshot、Case/SourceLedgerのglobal usage一致を確認する。op3/4は実A2 `prepare_journal_*` → `validate_prepared` → `apply` を使い、source差分を反映した後、Courtのsigned wire・attempt差分を一度だけ加算する。E/T counterは独立に渡し、追加work・canonical retentionは必須 `NativeCommonUsagePolicyV2` から得る。native料金換算の成功defaultは無い。Case.usage.feeは資料サービスの真正first receiptによるdueだけとし、mapperにfee fieldを設けない。Court/control/Panel料金は別のqualified funding ownerの責務であり、このprojectionへ足さない。テスト用E+T換算・retention・source料金は任意のtoy値であり、採択されたnative tariffではない。

完全NoOpのrequest/responseコピーを新履歴へ追加しない。どのエラーでもcacheをpoisonし、getter・再適用から部分状態を再公開できない。receipt/fee intent数は実ledgerとhelperの双方で `max_responses` に制限する。追加fee-intent BTreeMapのkeys/values/tree overheadは、ledgerのreceipt/reservation・geometry・known buffersと同時にnative ownerのpeak quoteへ含める。論理上限だけでRAM leaseを取得したことにはしない。

kind1は課金・台帳変更・policy callbackより前に `MissingSettlementPort` で拒否する。qualified native settlementの成功stub、public paid setter、settlement overlay、保存済みFundingからの復元は無い。fee intentとreceiptのservice完了は支払済み状態ではない。

## 追加監査 — 未完了source dutyの予約保護は未実装

A4の追加read-only監査を受け、`199ae602f` の実装も照合した。helperの `add_usage` はCourt signed wireを同じ `transmitted_bytes` へ無条件に加算し、mapperのwork/retentionを加えたCaseを `Service.budget` と照合してSourceLedgerへ戻す。次のop3/4はこの累積usageを起点に同じcapを照合する。したがって、Courtが余白を先に消費すると、受付済みdutyへの正当なop4でもBudget拒否され得る。現在のhelperはsourceの未完了予約を保護するnative能力を実装していない。

53件のlocal試験は差分の一度加算・source due・失敗隔離を確認したものであり、役割別の予約保護やnative capへの正規射影を認証した結果ではない。mapperが明示toyであることに加え、mapperを通らず加算されるCourt wireにも保護が必要である。現時点で実native policyは未接続であり、このhelperをそのままnative受付へ有効化しない。

[RFC-0044 §13](../../../rfc/0044-positive-court-full-coverage.md) はMinerの資料供給、Panelの再計算・証拠構築、Nodeの検査・保存・復旧を別々に測り予約する。[COMMON-CONTRACT §4](COMMON-CONTRACT.md) の全Panel合計はMinerの有限義務を鍵替え等で再生させない条件で、Panel/Nodeの計算担当をMinerへ移す授権ではない。[NATIVE-CONTRACT-R1](NATIVE-CONTRACT-R1.md) はfeeの区分を明示している一方、work/retention/wireの各fieldについてsourceとwhole-Caseの射影を確定し切っていない。3fieldを厳密にsource専用と断言したり、全作業を同じMiner義務へ合算してよいと扱ったりしない。

native接続には、役割別cold最悪値と全未完了source dutyの予約をadmission時に確保し、Court遷移後も各capで「消費済み＋未完了source予約＋許可するCourt消費」が収まることを守るownerが必要である。source応答で予約を解放する際も、消費への移動を同じ原子的遷移に含め、二重計上・枠の再生を防ぐ。workspaceをcanonical retentionへ足さず、payer同意でMinerの計算担当・責任範囲を拡張しない。具体的な射影・予約・役割責任の契約はA0が明文化した後に接続し、新rate/schemaや成功stubをここで補わない。

この共有枠による供給機会の喪失を、Miner delivery defaultへ読み替えてはならない。既に受理済みの応答Completeも取消さない。Budgetはlocal再生拒否でありMiner有罪の事実ではない。機会条件を満たさない帰責defaultは無効のまま、未検証claimのcredit拒否を維持する。本追記は実装・元ログ・53件のsource snapshotを変更しない。

## 結合試験の内容

実TIR class/jobからgeometryを導出し、実inventory builderのG14 openingと入力資料を用意する。署名済みop3 requestとop4 responseを、実Courtが検査するframeの間へ挿入する。ExternalStateはproduction helperそのもので、疑似counterだけを増やす代替台帳は使わない。

検査対象は、2種類のsource union・receipt・fee intent、global wire/work/attemptの一度加算、fresh ZERO basis、request/response再送、dutyまたはcanonical history欠落、部分global basis、origin差替え、kind1の型付き拒否、poison、live commit未呼出し。既存actual TIRの正常・不一致・Panel fault等8件も同じ候補で再実行する。

全source完了後の実署名付き偽current frameはPanelのWitnessRejectedだけとなり、MinerはPendingのまま、global charged work/wire・5 receipts・source due15を保持する。別試験では実ledgerがfirst receiptを適用した直後にreadonly policy callbackを失敗させ、case/usage/history/geometry/result/union/receipt/fee intentの全getterがPoisonedとなること、fresh replayでは正常完了することを確認した。

## ownerへの引継ぎ

A0は第1候補の保存・accepted-data reader・証拠を選択統合済み。追加のactual native job/role/funding authority portは現採択契約から供給されていないことをA0と確認した。新Assignment案の採択や、native料金・qualified moneyの具体化前に成功経路を推測して埋めない。

後続接続には、実accepted prefixとcanonical資料、scope/epoch/generation、role/opportunity、admission予約/責任、rooted料金schedule、actual UTXO input/lock/recipient/source-once/Court marker/残due/continuation/old-new CASを、同じnative viewとcommit batchで検査するownerが必要である。local factやaux151、raw journal cacheだけでGate C・paymentを生成しない。

full native restart・実資金精算・P2P/IBD・GC/pruning供給・T12・公開activationは未達である。raw資料の保守保持は実装したが、Node間供給の成功とは扱わない。既存の暗号学的計算証明不採用、モデル二階層固定、claim単位の検証条件を維持する。
