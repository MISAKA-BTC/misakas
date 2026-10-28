# ADR-DRAFT — モデル横断の大量courtを、固定証拠上限・容量予約・有界延長で処理する

**状態:** Proposed / 未採択  
**日付:** 2026-10-19  
**対応RFC:** [RFC-0025](../0025-scalable-objective-court-many-models-mass-disputes.md)(旧 Draft 名 RFC-DRAFT-palw-scalable-objective-court-v2.md)

## 背景

モデル数の増加に伴い、多数の独立claimに対するcourtが同時に発生する。単発courtを高速化しても、総証拠量・検証work・保持量が共有資源を超えるなら、全件を同一の3,200 DAAで終えることはできない。

従来snapshotはoriginal acceptance時の固定収集期限とFinal/bond holdを持つ。bondだけ遅らせても、古いcollect_untilでconvictionを拒否するなら安全な延長ではない。根拠は[source-audit-v2.md](source-audit-v2.md)を参照。

## 決定

### D1 — 目的を単発短縮から多モデル・大量courtへ拡張する

通常のoriginal受付起点でdoor≤3,200 DAAを目標とする。1,000/10,000件の有限burstでは、各courtの予約期限を守りながらqueueを排出する。持続過負荷は新しい経済的責任の受付を抑制して制御する。

### D2 — 小さい局所証拠と総量上限を両方持つ

前版A/B/Cを採用する。fragment、close、transcript全体、work、IO、RAM、roundsの上限を分ける。17,408 B/32 KiB/64 KiBはdraftの開発目標候補であり、実測・形式化されるまではUNPINNEDとする。

モデル/context/graphの許容範囲内で共通の上限を持ち、超過するprofileはclaim受付前に拒否する。巨大な演算は正しいloweringでさらに分割し、全faultの反証可能性を維持する。

### D3 — original claim受付で、将来の訴追容量を確保する

courtだけ後から受付拒否する設計を採らない。cold処理・control・敵対的分岐・terminal・保持・回復を有界テンプレート化し、globalな予約calendarに入れる。モデル・アドレスの分割で無料枠を作らない。

control枠を確保するだけで収録保証が成立するとはしない。base chainの有界inclusion前提と、合意で検査可能な予約規則を明示する。

### D4 — 混雑時は期限・Final・保持・bondを連動して延長する

`d_i=max(a_i+3,200,F_i+1)`、`E_i=d_i−(a_i+3,200)`を用いる。最大延長、保持、価格、署名、fundingをv2受付前に合意する。

後から到着した案件や同じ争点の再提出で既存courtを再延期しない。1 originalに複数courtがある場合、個別deadlineを保ち、frameの経済的保留だけ必要な最大期限へ集約する。

### D5 — 訴追権を優先し、過負荷の入口を制御する

全ownerがactiveな枠占有を含めて、適格な外部1人の反証経路を保証範囲内で保つ。費用、FIFO、owner枠、Sybil別アドレスだけで保証したと称しない。未証明ならarming不可。

新規原本claimの予約が資源/延長/保持上限を超えるときは、原本claim受付を抑制する。既存の有効異議を捨て、不正をFinalへ逃がすことは禁止する。

### D6 — ブロック容量拡張は明示的なprofile変更とする

まず小型化・予約配分・検証実装を改善し、それでも必要なら1×/2×/4×の候補を測る。bytesだけでなくwork/mass、伝搬、低性能node、reorg、pruned IBDを審査する。混雑を理由に無制限に自動拡大しない。

### D7 — checkpoint間隔と証拠保持を分離する

10,000 DAAをcheckpoint作成間隔の候補とする。未解決courtの証拠・予約・延長・bond holdは、複数checkpointを跨いでも保持する。安全に解決・確定した過去の詳細証拠はpruneする。

IBDは既存の認証済みpruning/chain/state検証経路と接続する。rootだけ・管理者署名だけのsnapshotを無条件に信頼しない。

## 採用しない案

- 全件のdoorを常に10,000へ一律変更する案。軽負荷まで遅くなり、大量件数の容量問題は別に残る。
- 全件3,200固定のままdeadline後の証拠を落とす案。混雑が不正claimの逃げ道になる。
- bond返還のみ延長する案。証拠提出・保持・Finalが整合しない。
- 10,000 DAAより古い全証拠を削除する案。延長中courtや復旧の必要証拠を失う。
- ブロックサイズを無制限自動拡大する案。承認対象nodeの検証・伝搬・同期能力を超え得る。
- 中央operatorが混雑と有罪を主観判断する案。客観的courtの要件を失う。

## 帰結

平常時は短いdoorを維持できるprofileだけを受け付け、混雑時は透明な追加待ちと追加保持費用を負担する。nodeのactive stateと予約管理は複雑になるが、容量不足を隠して安全性を落とさない。

1万件が3,200以内に終わる保証ではない。宣言した有限負荷とサービス条件の下で各予約を守り、有限burst後に解消し、攻撃の持続流入で先行案件を無期限延期しないことを保証目標とする。

## 採択条件

RFC C1–C10、出荷binaryの実ノード結果、one-outsider/枠占有、料金と署名済み責任、checkpoint/IBDの同値性が揃うこと。3,120→3,200、v1→v2、有効化height、wire/codec、capacity profileは明示的な承認を要する。

本ADRは採択・コード実装・t12配備を実行しない。
