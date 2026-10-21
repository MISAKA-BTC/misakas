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

## 2026-10-21 ユーザー選択 — S1の詳細仕様化・隔離実装

ユーザーは「S1：期限内の実履行だけ責任解消」を選択した。元の期限内に実際に受理・完成した正しい応答だけ当該sideの未決責任を一度解消し、期限後のproof-valid資料の妥当性と金融上の履行を分離する。上の未選択表記は選択前の履歴である。

この選択は詳細仕様化・隔離実装の承認であり、新料金・本番採択・pre統合・武装・配備を含まない。公開有料入口・実原資・同一rootのnative v5・全必要materialの時限資格・once-only処理・他責任保持の接続と試験は別途必要である。実装・試験の成功をこの選択だけから導かない。
