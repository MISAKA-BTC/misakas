# 初回MaterialRequestと同じcaseの後続予約

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

目標はStudioの同一claimを公開T12で受理し、Panel検証・Finalを経て、当選時の権利消費と対応するブロック生成を確認すること。本変更は初回資料要求を保護枠へ組み込む実装であり、公開受理や実支払の完成を意味しない。

## 実装した接続

[前段のSelection継続予約](../selection-material-continuation-2026-11-11/README.md)に対し、notice期限内のMaterialRequestを同じ物理予約表へ追加した。選出順位ごとに初回要求を一つ置き、その順位の後続グラフの根から因果辺を張る。全順位の初回要求・後続搬送・終端を一つの候補で予約し、枠不足・期限違反・費用超過時には一部だけ確定しない。初回要求の占有時間をnotice後の6,880 DAAへ二重算入しない。

初回要求の相対時刻、service cost、保持量を新しいresource identityに含める。quoteの送信量・work・I/O・control、保持量・byte×DAA・rentへ加算し、署名された順位別予算を確定前に検査する。固定carrier長は下限であり、署名検査・外側搬送・保存索引・undo・実予約探索まで含む本番価格の確定は残る。

締切だけが同じでも、資料不足を検出できる時刻と包含幅が異なることがある。署名後のprepared originalにもnative readinessを保持し、署名前quoteと同じ最早時刻・締切を要求する。順位の入替えについても、compiled scopeと初回packet IDをclaim・物理予約scopeから照合する。

関連ソース:

- [共通予約kernel](../../../../consensus/core/src/palw_batch_court_v1.rs)
- [順位別グラフの原子的予約](../../../../consensus/core/src/palw_batch_court_v1/protected_cases.rs)
- [費用・保持と順位の照合](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/admission/protected.rs)
- [署名前後共通preclaim](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim.rs)
- [初回要求の時間・費用下限](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim/initial_request.rs)
- [実署名fixtureのquote/intake同値・拒否回帰](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim/tests.rs)

## 搬送と実UTXO fee

[PMR1 carrier](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_carrier.rs)を追加した。固定幅の外側headerにclaim・Source・物理予約scopeを置き、元の署名済みManifestRequest bytesを保持する。未知版・幅・種別・subnetwork違反を拒否し、この入口はDrawnBond要求だけを扱う。decode成功は署名認証や受理ではない。

fee handoffはvalidated transactionの実入力・出力差額とcalculated feeを照合し、先行chargeを控除した同じ予算から一度だけ追加割当する。費用不足・context不一致・重複chargeは元の台帳を変えない。動的なfee台帳にはnativeメモリ所有を付ける。将来このhandoff本体をqueue/Arc等へ保持する側は、その保持費用も別途計上する必要がある。

## 検査範囲と残件

回帰コードは、初回枠不足・後段拒否のrollback、順位入替え、同じ締切でreadinessだけ変えた実署名入力、notice最終枠・幅ゼロ・carrier下限不足、実fee不一致・先行控除・二重chargeを追加した。静的監査でreadinessと順位照合の欠落を見つけ、修正した。Rustのbuild/testは[元の実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い未実行。

残件は、実requestの署名・rooted original/draw・実行中の包含順・book slot・供給義務を再照合するnative writer、pending caseと順位別paid receipt・物理枠消費の原子的保存、同じfeeのcollectible側控除、rollback/reorgである。同一取引からhandoffを作り直して二重支払認定しない一意性は、このwriterが保存状態とともに強制する。単なるfee割当やcodec成功をpaid receiptとしない。

初回MismatchNoticeの直接入口も別途必要であり、既に反証を持つ参加者へMaterialRequestを強制しない。完全TFO3 coordinator、Panelの応答・全体承認、Final・生成権・全consumerへの接続、公開候補の通し検証は未完。公開profile gateは閉じたまま。今回、公開照会・送信・配備はしていない。最後の保存済み公開観測は[DAA21239のG14_ROUTE_ABSENT(FpTokenizer)](../selection-native-notice-2026-11-11/README.md)。同一claimの公開受理・権利消費・対応blockは **NOT_PROVEN**。

本資料要求はclaim固有資料のための実装である。Minerによるモデル全量配信、HF取得可否証明・モデル可用性認定は再導入しない。[PRINCIPLES §13.3](../../../PRINCIPLES.md)の責務分離に従う。
