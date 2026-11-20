# ADR-0225 — Court nonceとPanel固有チャレンジを役務・承認へ束縛する

<!-- SWC1-AMENDMENT-BEGIN -->
> **2026-11-17 監査対応追補:** 本提案の新SWC1 profileとの接続は [ADR-0226](0226-staged-work-commitments-and-consensus-eligibility.md) / [RFC-0058](../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)、対応RFCの§16へ具体化した。経済TrialSlot・Court nonceから仕事の費用下界を導かず、段階別未知乱数、実計算WorkKey、元root、成熟・現在stateの再検査を要求する。この追補は採択・native実装・公開activationの完了ではない。
<!-- SWC1-AMENDMENT-END -->

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**
- 提案: [RFC-0057](../rfc/0057-court-nonce-and-panel-challenges.md)
- 根拠: ユーザーの「nonceをcourtに用いて同じcourtを出せないようにする仕組みを設計しRFCへ記述する」という依頼。
- 対象source: main@2d8212b634880b0967fbf24b00b41936c2322c14。先行RFC-0055/0056の文書成果へ追加する。
- 本記録は統合後の提案記録。通常閲覧先の20 RFC／50 ADRを増冊せず、正式採択・native実装・実測・activationは別に判定する。

## 1. 問題

claim・Panel・roundに結び付かないCourtや、提出のdigestだけを一回性のkeyにした処理では、別nonce・署名・relayで同じ役務を数え直す余地がある。nonceを外側へ付けるだけでは、他人の公開資料を用いて完全再実行を偽る行為までは検出できない。

既存0017は元claimへの署名binding、post-commitの箇所選択、完全承認と局所検査の区別を規定している。同時に、認証済み局所資料の共有を認める。shared court/source quotaもactor/relayごとの水増しを防ぐ。このため「Courtのbytesが同じなら全面拒否」という変更は、正当な資料共有や単独反証を壊す。

## 2. 決定案

### D1 — nonceは外側の義務に結合する

native claim・AcceptedOrigin・principal・duty・round・action・stepからObligationKeyを導く。期待root/profile/ruleset/plan/資金責任/期限は別ContextHashへ固定する。元Minerのcanonical整数計算、trace leaf、execution rootへPanel固有nonceを混ぜない。

元native IDやbeacon seedを切り詰めず、typedな長さ付き符号化で包む。監査したnative seedの64 bytesと、新CPN1 digestの32 bytesを区別する。署名対象にcanonical bodyのdigestとchunk rootを含め、部分搬送と完成bodyの両方を検証する。

### D2 — positiveの個別challengeは本人の承認条件にする

元commit後の既存native乱数を用い、Panelのidentity・義務・元rootに結び付くchallengeを一意に導く。対象空間、selector版、k/R、最悪費用は抽出前のPlanSpecへ固定する。導出後のtargetをContextへ戻して循環させない。

有限の拒否法と疎な部分Fisher–Yatesで最大kR hashと有限mapを用いる。上限到達はSelectionUnavailableであり、無料のseed/round再選択・算術的有罪・元時計再開を起こさない。nonceの相違が対象集合の相違を必ず生むとはしない。

### D3 — 共通coverageとPublicOneHonestを維持する

既存のclaim共通の必須局所coverageを省略しない。新しい個別challengeは、その人の完全承認を数える追加条件である。全員提出待ちを新しいFinal条件にせず、他Panelの不提出だけで新Qを作らない。全選出者の完全検証責務は維持する。

新規受理では未消費の義務を要求し、受理後はAcceptedPositiveRecordと対応するonce消費を照合して一件を数える。受理後も未消費を要求して承認を消す設計にしない。同一bondの重複位置・再署名を別人の完全承認にしない。

### D4 — negativeは既存caseの状態へ結合する

negative・資料要求・応答・終端にはcase/actor/phase/stepのActionNonceを使い、新しいseedを待たせない。個別sample外で発見した不正や、適格第三者の反証を受理する。

positiveを先に提出した本人の訂正noticeも別の適格義務として扱う。元の肯定責任は削除しない。Qと客観的反証を多数positiveや新nonceで覆せない。

### D5 — 共有資料・提出・支払いの一回性を分ける

EvidenceMaterialIdは共有できる資料、ObligationKeyは役務の一回性、CourtSubmissionIdは署名する内容を表す。既存のReward/Receipt/OffenceOnceは別に維持する。新IDだけから発行・報酬・生成権を作らない。

同一subjectの正当な再送は元receiptの参照又は無効果とする。異なるbodyの同slot提出はConflictであり、bytesが違うだけでequivocationとはしない。相互排他的な署名命題、適用policy、資金責任等が立証された場合に限り既定の裁定へ接続する。

### D6 — 帰責は客観的な署名命題の範囲に限定する

完全肯定をした元claim/root/scopeに有効な反証が確定した場合、事前に認可した有限の責任契約を適用する。LocalHoldsを別箇所の不正によって全体の虚偽承認へ拡大しない。資料不足、seed停止、root不一致の申告だけで計算有罪にしない。

正しい資料のコピー、外注、共同計算、本人のGPU利用の有無は、この証拠だけでは立証できない。独立再実行の責務と、観測できる虚偽肯定への制裁を区別する。nonceやBFTという名称だけから能動的な正直者一人の存在を導かない。

### D7 — 元の資源・経済・復旧契約へ接続する

selector、全actorの潜在資料、staging、署名、局所検査、担保、最遅帰責、保持・復旧までRFC-0055へ予約する。既受理救済へRFC-0056の新Dを再要求しない。共通quotaや総搬送1 BPSをnonceごとに増設しない。

通常の取消・期限切れ・pruningは消費済み義務を復活させない。正規reorgは義務・資金・承認・case・支払いを同checkpointでundoする。新profileへのdowngrade迂回を拒否し、旧claimの署名・時計・権利は元規則で保持する。

## 3. 比較した構成

| 構成 | 判定 |
| --- | --- |
| clientが自由にnonceを選ぶ | 同じ役務の追加抽選・ID水増しを防げないため不採用 |
| 証拠bytesのglobal重複禁止 | 合法共有と単独反証を妨げるため不採用 |
| 全Panelの個別challengeをFinal必須にする | PublicOneHonestの正常完了を他者不提出で止めるため不採用 |
| negativeにもrandom sampleを要求する | sample外の有効反証を排除するため不採用 |
| nonceを元traceへ混ぜる | 元rootとcanonical整数実行を変えるため不採用 |
| native義務へのnonce・主体別challenge・共有資料台帳 | 本RFCの提案。保証限界と完了条件を明示する |

## 4. 凍結仕様への扱い

新profileの署名scope/envelope、本人別challenge計数、nonce/義務の有限台帳と責任接続を提案する。既存のglobal pool、モデル階層・登録、算術、WorkId、仕事credit、全体検証責務、単独反証、PublicOneHonestは変更しない。

今回のRFC記述依頼を、既存凍結仕様の正式採択・wire番号登録・公開activationへ拡大解釈しない。旧本文・過去結果・baselineは保持し、通常閲覧先には提案への追記pointerだけを追加する。

## 5. 実装・レビュー・検証

[ソース監査](../maintenance/revisions/2026-11-17-court-nonce/SOURCE_AUDIT.md)、
[実装接続表](../design/palw/integration/court-nonce-implementation-plan-2026-11-17.md)、
[レビュー](../maintenance/revisions/2026-11-17-court-nonce/REVIEW.md)、
[検証記録](../maintenance/revisions/2026-11-17-court-nonce/VALIDATION.md)を参照する。

抽象試験は契約の反例検査であり、native全route・実資本・beacon安全性・実LLM・最低Node・IBD・閉ループ性能・公開activationの証拠ではない。本人の独立全体再実行を証明したという判定を受入条件にしない。
