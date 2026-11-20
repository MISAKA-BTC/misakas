# ADR-0224 — 全claim共通の受付難易度とcanonical負荷観測

<!-- SWC1-AMENDMENT-BEGIN -->
> **2026-11-17 監査対応追補:** 本提案の新SWC1 profileとの接続は [ADR-0226](0226-staged-work-commitments-and-consensus-eligibility.md) / [RFC-0058](../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)、対応RFCの§16へ具体化した。経済TrialSlot・Court nonceから仕事の費用下界を導かず、段階別未知乱数、実計算WorkKey、元root、成熟・現在stateの再検査を要求する。この追補は採択・native実装・公開activationの完了ではない。
<!-- SWC1-AMENDMENT-END -->

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**
- RFC: [0056](../rfc/0056-global-claim-admission-difficulty.md)
- 継承する容量規範: [RFC-0055](../rfc/0055-global-admission-court-calendar-and-one-bps.md)
- 実装調査の基準: MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14
- 依頼の範囲: court資源制限とは別に、全種類の新規claimを対象とする共通受付難易度を独立RFCとして具体化する。
- 承認の境界: RFCの記述依頼を、凍結済み設計の正式な改定採択、native実装完了、性能測定完了、公開activationの承認へ読み替えない。

## 1. 背景

RFC-0055は、共通bounded checkerの費用上界、全合法継続のflow/stock予約、Node内部のpreflight/lease、既受理courtの保護、全carrier合計1 BPSを定める。このhardな容量保護と、新しい検証責任を作る候補量の調整は役割が異なる。

RFC-0056 §1のmainソース調査では、Attemptの既存W/class ticket、FreePromptの新規受付、Final後のFP receipt、TFP1/TFO1/TFO2の実資金・予約、一blockのcourt費用上限を区別している。これらの存在を、全新規route共通の負荷連動Dが接続済みという証拠にはしない。TFO2内部のwire version 3を独立したTFO3実装とは扱わず、将来routeは適用対象と実装済み経路を分ける。

本提案は、RFC-0055 §6の受付Dを独立して詳細化するものである。採択された新profileではClaimAdmissionDifficultyV1（CAD1）をその一つのDとして用い、UAI1のDと二重に掛けない。RFC-0055の全容量規範、既受理責任、既存Wと経済資格は維持する。

## 2. 提案する決定

### 2.1 四つの制御を分離する

| 制御 | 対象 | 本提案の境界 |
| --- | --- | --- |
| D_admit / CAD1 | 新originalが高コストな検証責任を作る前の候補資格 | 全model・全新規routeに一つのtargetを使う |
| 既存W_work / class target | 既存適用版の仕事・生成条件 | CAD1で上書きせず、既存の必要条件として維持する |
| FP receipt target / P_m / CCU / モデル生成権 / ExecCredit | Final・認証・資本・経済契約 | D合格や原受付だけで権利を発行・消費しない |
| 全carrier合計1 BPS | 外側envelopeの搬送 | CAD1から追加permitを発行せず、RFC-0055の供給・包含条件を使う |

無報酬・評価というラベルやroute名ではなく、native状態遷移が新originalや新しい検証責任を作るかで判定する。複数originalのbatchは各originalに独立の試行・ticket・予約を要求する。既受理の適格notice、反証、応答、終端、保持を新しいDへ通し直さない。新originalを内包しない通常取引、BASE/control、既存権利のredeemは各自の既存資格・上限で処理する。

### 2.2 重い処理より前に共通gateを置く

有界な固定header、checked length/count、ingress lease、認証、identity、資本参照、一回性、簡単なshape規格を先に確認する。その後、資金とcontrol容量を確保したPresealに対し、事前固定Dと唯一のseedでticketを判定する。eligibleでcommitに一致する資料がある候補だけ、予算化したintrinsic検査・actor graph・plannerへ進める。

最終受付ではRFC-0055に従って現canonical prefixの実資金、全合法継続、全期限区間のflow/stock、carrier/controlを予約する。original・資金・book・actor責任・一回性・cursorを同checkpointで保存し、部分受付を残さない。D合格は容量予約の成功を保証しない。

無料quoteは有界な型・範囲確認までとする。詳細plannerを呼ぶquoteはeligibleな資金付きPresealと署名済み費用scopeへ束縛し、basis変更時の再計算も予算と実費を消費する。quoteを予約権にしない。未認証RPC、D不合格、fork候補にも費用があるため、Dとは別のingress/control上限を維持する。

### 2.3 実行機会の一回性と結果commitを分ける

既存WorkIdの意味を変更せず、その外側にTrialSlotKeyを置く。TrialSlotKeyは認証された既存資本・実行機会/sequenceに束縛し、output、trace、署名bytes、route、model aliasを変えて追加試行を作れないようにする。最初のPresealが当該slotの入力・結果・model・targetを束縛する。

同じ入力の正当な別仕事は、別の有資金実行機会・新sequenceと必要費用を使える。同一promptを世界中で永久禁止するキーにはしない。別鍵・別modelを増やしても同じ資本の二重lockやglobal予算の増設は認めず、異なる実資本で支払う別trialを身元審査で同一視しない。

資本sequence高水位・認証済みnullifierと移行規則によって、pruningや世代変更で消費済み機会を復活させない。新しいSHA-256 digestと既存native IDを区別し、既存IDの型・canonical表現を黙って変更しない。codec、domain、長さ上界はRFC-0056の版付きpolicyへ束縛する。

### 2.4 Preseal・未知seed・観測の因果順序を固定する

120,000msはRateClock上の計画epochとし、120 DAAや120受理blockへ換算しない。target eのDを収集開始前に固定し、e−2でPresealを収集、Start(e−1)で集合をfreeze、固定後の既存beaconからseedを得て、eでrank順の有限cursorを処理する。epoch b開始時の観測更新はtarget b+2へ適用し、既公表のb/b+1を変更しない。

seed・資料readiness・伝播・canonical包含の期限は共通clock/control契約へ束縛する。Nodeの私的な受信時刻でready集合を変えず、seed失敗時に都合のよい別anchorを選ばない。bootstrapの空epoch、seed不成立、未処理尾部を、完全に観測した空需要と混同しない。既受理courtとBASE/controlは新規受付停止中も元契約で継続する。

Preseal自体に、記録・freeze・hash・順位・最悪訪問・資料保持・精算のcontrol容量と実費・資本lockを確保する。未知seedでeligibleが少ないと予想して最悪control予約を削らない。件数は有限の共通残資源から導き、one-winnerや固定claim数を追加しない。

### 2.5 Dにはcanonicalなclaim責任負荷を入力する

分母C_Aは共通qualification済み供給Cから固定system、最低通常取引、ingress/control、復旧の先行枠を差し引く。負の容量や、正の需要に対するゼロ供給を整数wrap・緩いDで解消しない。外側permit releaseの費用と、そのcarrier内でco-packできるclaim payload/検証容量を別次元にし、固定1 BPSだけでclaim容量を二重控除しない。

Uは、現在の未来bookに残る全claim責任の比率、完了epochのcanonical費用debit済みflowの比率、完了epochの認証live-state/保持stock上界のpeak比率の最大とする。三つを足して同じ仕事を二重計上しない。境界時点の残予約だけを見て、epoch内に消費・解放した仕事を低負荷と誤判定しない。

未実行の未来予約の過去の瞬間peakを無条件には難化根拠にしない。合法解放後も実費、live-state peak、発生済みNoCapacityの観測は残す。hostのCPU使用率、RSS、worker終了時刻、RPC件数、申告仕事量を合意controllerへ投票しない。

### 2.6 全拒否・観測不完全・低負荷を区別する

qualified_demandは、eligible候補をnativeに訪問し、identity・一回性・既存資格・必要資金・対応shape・内在的な全工程/期限適合を確認した場合にだけ立てる。算術結果全体の正しさを前段で確認済みという意味ではない。

NoCapacityは、同じprofile・期限・readiness・資金で、固定system予約を残した空claim-bookなら収容でき、既受理責任との競合で不成立となる場合に限る。unsupported、資金不足、資料不足、内在的期限不能、planner予算切れ・未処理・実装エラーを混雑票にしない。

観測は集合、seed、ready root、cursor、全disposition、canonical費用、stock peakとclose根拠へ束縛する。完全な空集合やeligibleゼロはcompleteになり得るが、未処理・欠けた進行を空需要として合成しない。

| 観測 | 提案する更新 |
| --- | --- |
| 適格需要がありUがhigh、または正規NoCapacityがある | Dを増加。受理数ゼロでもhighになり得る |
| completeでUがlow、混雑・供給停止なし | 連続lowにより緩和。完全な空集合も意図したidle decayの対象 |
| incompleteだが確定済みの適格high根拠がある | 増加可能 |
| incompleteでhigh根拠なし | 保持しlow_runをreset。lowにしない |
| profile/seed/共通service等がcanonicalにunavailable | highより優先して保持しlow_runをreset |
| 私的RPC洪水・各hostの遅延・復旧不調 | 合意観測の根拠にしない。各Nodeは保留・復旧し架空の観測を作らない |

D範囲1〜2^120−1、1024基数、768/256閾値、ceil(9D/8)、floor(8D/9)、3連続low、checked uint256はRFC-0055から継承する設計案の値である。小さい整数での丸めを含め一意に計算し、制御係数を機器性能の実測値とは扱わない。policy変更は事前公表し、凍結targetへ遡及適用しない。

### 2.7 費用・cancel・正誤判定を分離する

Preseal受付・凍結・hash等の実行済み費用は、IneligibleやNoCapacityでも消えない。未使用lockは署名済み契約と有限の精算予約に従って返還する。同一slotの再送、結果差替え、target移動、失敗後の別seedへの無料carryで追加抽選を作らない。

cancelでfreeze済み集合、既消費費用、確定disposition、過去のclose結果を削除しない。受理後の救済・精算・保持責任はRFC-0055で維持する。NoCapacity、seed障害、私的未受信、資料待ちを算術的不正としてslashしない。reorgは集合・seed・D・cursor・費用・slot・予約を同じjournalでundo/replayする。

## 3. 維持する原則と適用範囲

モデルのcanonical整数計算が完全一致する前提、モデル非保有Node、独立全体承認を要するFinal、適格な一人の反証保護を維持する。未知modelの個別benchmark・品質審査・名前allowlist、Node全推論・GPU fallback、計算正当性の暗号学的証明を追加しない。

一万/五万modelでもD stateと入口・資本・保持・claim予約の予算は全体で共通とする。model登録数を直接Dへ掛けず、毎blockの全model走査を必要条件にしない。registry、集合、nullifier、counter、復元・GCは共通stockと実行予算を使う。

既存P_m、CCU、tier、モデル生成権、有限credit/receipt、旧WorkId、署名、過去Final、原期限、既受理責任は変更しない。共通Dにこれらの係数を二重適用せず、原受付から均等なblock配分や一claim一blockを作らない。実受理率・Final到達率は供給、候補、資源要求にも依存するため、入口を含むP_m比例を保証しない。延期・無効化済み機能を、CAD1対応を理由に開放しない。

## 4. 採用しない案

- raw RPC数、host使用率、未認証packet、未資金presealをそのまま難化票にする。
- モデル別D、モデル数比例の予算増設、固定claim/court件数で全体負荷を制御する。
- 全claimを先に重く検査し、Final後のW/receipt抽選で入口負荷も制御したと扱う。
- D合格だけで容量予約、Final、報酬、モデル生成権、carrier permitを得る。
- 未処理cursor・seed欠落・service欠落を、完全な低需要epochへ読み替える。
- 既受理courtに新Dを要求する、資源不足を算術有罪へ変換する。
- 全世界の同一promptを永久禁止する、output/署名/route/model aliasの変更を無料の別trialにする。
- 旧Wと新Dを同じstateやdomainへ混ぜる、採択後にUAI1とCAD1のDを二重適用する。

## 5. 残余リスクと完成条件

Dは確率的な候補量調整であり、全ticketが通る場合にも安全性を保つのはRFC-0055のhard予約とNode内部上限である。約240〜360秒の反映遅れと倍率更新の遅れがあり、式だけで閉ループ安定性や達成処理量を実証したとはしない。

形式・資格・資金を満たす虚偽結果は前段では排除できず、有料の負荷操作は残り得る。実費、同時資本lock、有限trial/control予算、全工程責任、後続Panel/courtで費用と影響を制限する。ticketを高価な物理計算の証明と扱わない。canonical化前の検閲や、未知seed・供給・包含の前提不成立に対する無条件の公平性も約束しない。

正式採択を検討する際はRFC-0056の差分範囲を明示し、設計凍結の手続きに従う。native接続では全新規route・quoteの前段gate、TrialSlot/Preseal/資金の一回性、観測とjournal、D停止中の既受理救済を確認する。実LLMの正常系と不正系を別々に、各経路内で起源claimの同一性を保って検証する。最低Nodeで混合負荷、未完了分母、backlog、空slot、復旧を含め評価し、旧義務を保つ移行と公開activationは別記録にする。

[実装接続表](../design/palw/integration/claim-admission-difficulty-implementation-plan-2026-11-17.md)、[ソース監査](../maintenance/revisions/2026-11-17-admission-difficulty/SOURCE_AUDIT.md)、[レビュー](../maintenance/revisions/2026-11-17-admission-difficulty/REVIEW.md)、[改訂と検証](../maintenance/revisions/2026-11-17-admission-difficulty/README.md)を証拠の所在とする。文書検査・抽象モデル・native実装・実機測定・activationを分け、RFC-0055の既存抽象試験をCAD1の実装合格へ流用しない。本ADRは提案記録であり、採択・実装・有効化の完了宣言ではない。
