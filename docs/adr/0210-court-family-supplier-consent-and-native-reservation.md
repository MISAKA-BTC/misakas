# ADR-0210 (CourtSupplierConsent) — Court全分岐の供給者契約と原子的native予約

> **並行割当衝突:** 同番号の [ADR-0210 (AllModelG14PanelOne)](0210-all-model-g14-panel-one.md) は別の主文書です。両方の元番号・完全パス・採択内容・wire/署名domainを保持し、修飾名と完全パスで区別します。[番号台帳](../maintenance/numbering.md#2026-11-02-並行割当衝突)。

Decision: **Accepted / 実装中・統合未検証・未有効化** (2026-10-30)。
[RFC-0042 (CourtSupplierConsent)](../rfc/0042-court-family-supplier-consent-and-native-reservation.md)を採用する。
承認者はユーザー。具体案と署名実装を提示した採択質問に対する回答は、
**「新契約を採択して接続まで進める」**。公開ネットワークの有効化・配備は別である。

既存7-family source graphに含まれない材料・供給役を、旧pledgeへ無断追加すると役務の同意と
実原資が不足する。この進行不能・資金不足を、新しい双方署名contractとnativeな全branch予約で閉じる。
RFC-0035の旧wire/署名の意味を保存し、新version envelopeにliteral旧transportと新consent集合を拘束する。
parameter/prompt、semantic table、tokenizer raw、Prefix生成後selection/stopの全native inventoryを対象にする。

料金・資本は既存scheduleと実導出した最大packet/材料byte数からchecked計算する。
新feeは既存material-fundingと同じ実sponsor bond principalから専用held trancheへ移し、
元Court UTXO escrowへ二重計上しない。既存native burn・locked-UTXO・SourceIncome・物理queue・
receiptの一回性を保つ。名目予約だけで支払済み・fundedとしない。

Hはnative original hard。既存のper-packet service/inclusion時計でH以内に実際にdefault確定した場合だけ
元sponsorへ全額返金し、それ以外はH後に供給者へ支払う。未使用・未解決も同じfee規則で扱い、
H後のdefaultでfeeを遡及没収しない。別のservice bond・材料保持責任は維持する。
due=HのdefaultがH+1になるlate制限を明示する。処理機会のない未供給をslashせず、no-slash recoveryを維持する。

旧pledgeの義務拡張、未定費用をゼロとする受付、申告graph/供給済みflagの信用、
原資のない名目payoutは採用しない。固定した新単価・期間は追加しない。
未承認の測定値を承認済みとすることもない。

独立レビューは署名domain・tag衝突・原子的rollback・保守的需要上限・金融保存則・保持と
default帰責・codec/IBD/reorgを確認する。実装/統合試験が未完の時点でreadinessを閉じない。
PRINCIPLES §8・§9、claim認証、モデル制度、初期mainnet延期scopeを変更しない。

履歴文書の未実装記述は当時の記録として保存する。本ADRはRFC-0035の役割別供給契約の
未確定部分をRFC-0042の範囲で確定し、旧契約への遡及適用や他の未採択案の採用は行わない。

実装上の閉じ方はRFC-0042 §3.1の専用clock version 3とする。新contractだけで、
認証済み実table幅と実native応答turnからoriginal・owner queue/evictionを導出する。
旧V1/V2の時計・IDは保存し、長い材料保持を短いowner処理の待ちへ加算しない。
実TIR Prefixに存在しない旧PublicPrefix履歴復旧だけを新TIR quoteから除外し、実TIRの
MMR・再取得・全query材料の資源は維持する。door・容量・単価の引上げや測定承認ではない。
出生のnative規則contextとliteral材料による復元再導出、cohort metadataとframeのGC整合を検査する。
