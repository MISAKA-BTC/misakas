# 公開資料・モデル未保有Panel — 独立レビュー記録

日付: 2026-11-09。固定基準は[REVIEW-CRITERIA.md](REVIEW-CRITERIA.md)。本文執筆に参加していない2エージェントが、執筆後の仕様を審査した。判定を得るために基準や保証の限界を緩めていない。

## 指摘と修正

今回の独立レビューでは、安全性のBLOCKERは報告されなかった。次の表現・参照・報告上の明確化を実施し、再確認した。前回のFAILとB1〜B5は[前回記録](../2026-11-09-palw-execution-integrity/REVIEW.md)をそのまま保持する。

| ID | 指摘 | 修正と確認 |
| --- | --- | --- |
| R1 | RFC07の「leafだけを計上」、policy0.4のleaf別会計はatomic batchの部分creditにも読める | batch全体Final後に全leafの非重複仕事を一回計上、一leaf反証で全体Refuted、部分Final／creditなしと明記。統合reviewerが再確認 |
| R2 | RFC18 §9.4が正常系のcase終結を、供給default専用の§8へ参照している | §2.4／§4.1／§7のChallengerDefault・LocalPredicateHolds等へ訂正。両reviewerが再確認 |
| R3 | RFC18 §4.1の無応答defaultの参照で、供給者と申立人が一括されている | 供給義務は§8、申立人の次手違反は本節と§2.4へ分離。core reviewerが再読して残件解消を確認 |
| R4 | 大資料の客観的供給・検査前提の未確立は、実装・測定だけの残件に見せない方がよい | README／STATUS／0050／0218／policy0.4にもPROFILE PROCEDURE NOT ESTABLISHEDを独立表示。統合reviewerが再確認 |
| R5 | RFC18の新節9.4が9.3より前に配置されている | 意味を変えず9.2→9.3→9.4へ整序。core reviewerが再確認 |

## Core review

担当: `public_panel_review_core`。RFC16〜20の全本文、RFC05の現行規範を確認し、**CONDITIONAL SPEC PASS**と判定した。R3の最終参照修正も再読し、非BLOCKER編集残件の解消と判定維持を確認した。

一人前提の所在、cold取得、全必要資料、最低1件の全scope承認、早期Final禁止、全体coverage、元Miner commitへの全関係binding、局所court、資料到着と検査完了、全合法係争・原時計・個別予約・atomic batchが整合していると評価した。B1〜B5の再発経路は見つからなかった。

## Integration review

担当: `public_panel_review_integration`。PRINCIPLES §13、RFC0050／ADR0218、policy0.4、RFC05／12／16〜20、07／10／11とパラメーター台帳の接続を確認し、**CONDITIONAL SPEC PASS**と判定した。

旧75／41との版分離、1署名即Finalや全員ACKの禁止、旧55パラメーター行の保全と新しい設計1行、cold入口と取得後検査、同batch取消と消費、atomic batchの会計、数万モデルの容量と実測の区別、未完gateの入口表示を確認した。

## 最終判定と未完事項

**条件付き文書規範として2系統ともPASS。今回のレビュー範囲に未解決の設計BLOCKERなし。ADOPTED_DESIGN_NOT_ACTIVATED。**

これは次の成立を認定したものではない。

1. **PROFILE PROCEDURE NOT ESTABLISHED:** 大容量の公開資料について、虚偽未着と正直なcold Panelへの選択的秘匿を、有界なnative根拠で区別し全scopeの検査前提を確定する具体手順。その手順がないprofileは有効化不可である。公開URL、一部chunk、別Panelのreceipt、quota消尽、時間経過を代用しない。
2. **IMPLEMENTATION NOT ESTABLISHED:** 実LLMの全kernel・初回trace・局所witness・native checker・資料要求・Q・承認・決済の対応実装。
3. **MEASUREMENT NOT ESTABLISHED:** cold唯一正直者、最大合法虚偽case、Node最低要件、混合負荷、包含、restart／reorg／IBD、モデル数×活動頻度に対する実容量。

正常完了の保証には正しいMinerの公開供給と応答、正直者の能動的完全検証、客観的な全合法case終結、実資源・包含・原期限が必要である。これらが成立しないときに未検証Abortへ進む安全性と、正常Finalを完了できる有用性を別に扱う。全拒否・全Abortだけを完成と報告しない。
