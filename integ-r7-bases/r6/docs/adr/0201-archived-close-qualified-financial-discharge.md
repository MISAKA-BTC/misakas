# ADR-0201 — archived closeの期限内実受付と責任解消

Status: **Proposed / Review Required** (2026-10-21)。詳細仕様化・隔離実装の選択待ち。[RFC-0034](../rfc/0034-archived-close-qualified-financial-discharge.md)に根拠、時限・署名・原資・金融writer・保持と受入条件を具体化した。

## 問題

[RFC-0030 §12](../rfc/0030-court-end-effects-and-pending-liability-archive.md#12-private-append-of-precommitted-archive-material)のprivate appendは不足bytesを保存するが、実受付pointを保存しない。後からprivate v5が正しいと判定しても元のassembly期限内に提出したとは限らない。これを履行と同一視すると、署名済み期限とdepositの金融効果を黙って変更する。一方、court終了後でも**元期限内に実際に受理・完成した**正しいbodyは、未投稿応答の配送・収録仮定なしにpositive fulfillmentを確認できる。

既存depositは前払escrowではなく、履行による名目責任の解消から新しい送金やPanel売上は発生しない。既確定徴収や他claim責任の返還を導出しない。

## 選択肢

| 候補 | 内容 | 境界 |
| --- | --- | --- |
| S1（推奨） | 元期限内の実native completionと元署名/native v5を束縛し、当該sideだけpositive fulfillmentとして一度解消。late valid資料は妥当性と時限履行を分離し、absenceはF02適格判定を別途要求 | 実受付point・同一state-root・原資/有料入口が要る。不明はno-slash/hold。hard自動返還なし |
| S2 | 正しいlate completeを提出義務完了として、元期限後でもdeclarationの名目holdを解消 | 元署名の期限効果を緩和する新しい経済規則。既存F02/Aの単なる接続ではなく、追加の変更採択が必要 |

S1を推奨する。実際に受理された期限内の正しい履行を解消しながら、遅延資料や欠落した供給根拠から違反・返還を推定しない。S2を暫定動作にしない。

## 採択範囲

求める判断はS1による詳細仕様化・隔離実装である。既存承認Aの裁定と未決責任分離を維持する。新しい料金・保持期間、公開late受付の本番採択、旧claim移行、pre統合・武装・配備を含まない。

未回答の間は経済入口の既存hold監査と、方式に依存しない全分岐/実供給実装を続ける。採択前にarchiveのfulfilled statusを変更したり、実費未接続の公開受付を開いたりしない。元署名deadline、元session/original hardを上げない。

実装・検証結果は未記録。G14全体の受入条件、RFC-0030 E01–E10、T01–T15を縮小しない。

## S1 private実装初稿

[RFC-0034 §7](../rfc/0034-archived-close-qualified-financial-discharge.md#7-s1のprivate実装初稿)に、同checkpointのactive prefix出所、native chunk全数の元時限資格、元署名/v5と同root、once-only名目責任解消を接続する候補を記録した。新key25 / row31で既存wireを保持する。旧時刻なしappendや出所不明prefixは、最後のchunkが元期限内でもholdを維持する。実徴収・返還・他side・元stake・claim creditは変更しない。試験結果はまだ未記録で、公開有料入口、全原資、全責任解消/pruning、G14全体の完了とはしない。

## 2026-10-21 ユーザー選択 — S1の詳細仕様化・隔離実装

ユーザーは「S1：期限内の実履行だけ責任解消」を選択した。元の期限内に実際に受理・完成した正しい応答だけ当該sideの未決責任を一度解消し、期限後のproof-valid資料の妥当性と金融上の履行を分離する。上の未選択表記は選択前の履歴である。

この選択は詳細仕様化・隔離実装の承認であり、新料金・本番採択・pre統合・武装・配備を含まない。公開有料入口・実原資・同一rootのnative v5・全必要materialの時限資格・once-only処理・他責任保持の接続と試験は別途必要である。実装・試験の成功をこの選択だけから導かない。

## 2026-10-21 隔離候補の検証checkpoint

S1のprivate native writer・別rooted provenance（key25/row31）・同一root/native v5・当該declaration名目責任の一回解消まで接続した。nativeの期限内再提出で既存同値bytesの現在pointを新規記録でき、過去時刻は遡及しない。wire/OOMのNativeResource分類、実serializer/decode容量の事前予約も修正した。caps・元deadline・旧wire/署名domainは変えていない。

関連84 unique selected pass（重複除外、archive filter11とnative frame42は7件重複）、core全tests型検査・非test consensus lib検査はexit0。これはprivate native fixtureであり、実原資・公開有料入口・実返還/徴収・全exit/pruning・別claim金融composition・実node全T01–T15・G14全体の完成ではない。[正確な範囲・raw log・source/binary hash](../evidence/palw/g14-completion-2026-10-21/s1-and-original-cover.md)。文書検査は既存の参照欠損によりFAILのまま。
