# RFC-0017 — モデル停止・解除・BMR1恒久廃止

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**ADOPTED_DESIGN_NOT_ACTIVATED**。BMR1の恒久廃止と、MOE3の全モデルburn-pauseを統合する。
- 判断理由：[ADR-0017](../adr/0017-model-control-and-retirement.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 二つの制裁を分離する

**BFTはモデルの採用・存続を判断し、Courtは個人の不正・責任を裁く。** モデル恒久廃止のためにBFTが個々のPanelの正当性を判定する制度を作らない。

| 対象 | BFTモデル恒久廃止 | Courtの個別不正認定 |
| --- | --- | --- |
| 対象モデルの登録者責任bond | 全残額burn | 既存の個別責任も適用 |
| 対象モデルに拘束された採掘bond | 全残額burn | 既存の個別責任も適用 |
| Panel検証bond | 一律に一括burn対象外 | 虚偽承認等が成立すればslash可能 |
| 旧契約のGate C検証bond | 一括burn対象外 | 元の責任期間・違反規則で個別裁定 |
| 他モデルのbond・未割当資金 | 対象外 | 当該個別違反だけを適用 |
| 正当に成立した検証報酬 | 元条件でMISAKA支払を維持 | 元契約の責任・holdは維持 |

検証bondの保護はBFT一括没収からの保護であり、Court免責ではない。不正の疑い又は既知の違反フラグによってBFT側の保護を切り替えない。BFT恒久廃止はgenesisを含む全モデルに共通で、登録者不在genesisに架空の登録者bondを作らない。

## 2. 通常報酬と没収は別原資

最新の通常coinbase配分はMiner等POSITION50%、LP用POSITION購入20%、MISAKA永久LP20%、PanelMISAKA5%、BFT MISAKA5%である。通常coinbaseの10%BFT案・15%検証案は置換済み。原資Cの一回分割、端数、役務planはRFC-0016に従う。

BFT5%は、元coinbaseの認証配分record、当該期間の正規roster・継続役務資格・支払planに結合する。modelごとにさらに5%控除しない。廃止のYES/NO、件数、没収額、発見報酬と連動させず、廃止成功報酬は0とする。

5%内の受取人と配分は、当該期間より前に認証された有限のBFT役務planで固定する。未設定の受取先・資格を後から選ばず、未成立分は元の有限予約・期限で精算する。廃止を理由に新たな報酬発行、手数料免除、他モデルからの補填を認めない。

**BFT恒久廃止で没収する資産は100%burn。誰も受け取らない。** §7のCourt49%発見報酬をこのburnへ適用しない。

## 3. 用途別bond台帳

各bondを `funding_source / lot_id / model_id / purpose / amount / owner_identity / liability_until / hold / sequence` に結合する。purposeは少なくともMODEL_REGISTRANT、MODEL_MINING、PANEL_VERIFY、および旧契約に残るGATE_C_VERIFYを区別する。アドレス又はアカウント全体から制裁額を決めない。

同じ資金atomsを複数purposeやmodelの全額担保として重複計上しない。複数モデルへ分ける資金は、認証された相互に重ならないsub-lotへ分割する。`sum(sub_lot.amount)<=funding_source.unspent_amount`を守り、合計を超えるreserve、未知の用途、重複lot、未認証の自己申告を拒否する。

同じ参加者が対象モデル採掘6,000 MISAKAとPanel検証4,000 MISAKAを拘束している場合、前者6,000だけをBFT廃止でburnする。Panel資格を持つだけで採掘lotまで保護される経路を作らない。反対に、Miner兼任を理由に検証lotを巻き込まない。

既存責任期間・hold中のlotはmodel/purposeを変更できない。unbond要求を出しただけで用途を消さず、元の責任が切れるまで対象タグと残額を保持する。認証された廃止caseのfreeze後は対象登録者・採掘lotの移転、用途変更、早期解放を禁止する。検証lotの通常の期限精算を追加で凍結する理由にはしない。

既に適法に解放済みで責任も終わった資金を、任意の過去wallet残高から回収する権限は作らない。停止直前の回避防止は、正規caseの固定点と既存liability期間に依存する。未知の実質所有者まで判別できると主張しない。

## 4. 恒久廃止の状態と権限

論理状態は `LIVE / RETIREMENT_REVIEW / RETIRED`。可逆burn-pause、独立BFT safety stop、個別claim状態とは別である。

### 4.1 正規case開始

case開始は正規のnative認可・対象model/root・認証起点・有限review期限・権限rosterを必要とする。単なるproposal受信やmempool到着ではfreezeしない。MAD2の正規BFT前提 `n=3f+1, q=2f+1` の下で、専用のcase-open認可を確定する。人数・期限等の本番値は未割当。

canonicalな開始から新規採掘受付と未確定採掘報酬の支出、対象登録者/採掘lotの解放を止める。poolからの逃避と新規購入者の流入を避けるため、対象AMMの新規buy/sell・LP追加も停止する。既に確定した支出を遡及取消しせず、未決注文の外部escrowは元条件で精算・返却する。検証、Court、資料供給と正当に成立した検証報酬は継続する。

caseをREJECT又は期限切れで閉じた場合、case固有のfreezeだけを解除する。独立停止・元liability holdは維持する。未決をYESと見なさず、期限resetや同じcaseの無期限延長をしない。

### 4.2 廃止決議

専用MODEL_RETIRE承認にはnetwork/genesis、BMR1版、case ID、model/root、canonical開始anchor、期待状態/sequence、対象用途lot集合と上限、poolと原資参照、roster/epoch、burn policy、期限を束縛する。正規q YESと現在stateの全条件を満たす場合にのみ、native台帳で決定論的に実行する。

採用YES、MODEL_STOP、SAFETY_RESOLVE、BurnResume又はclaim承認を廃止票として流用しない。BFTはchain finalityやreorgの権限を持たない。decision対象と実行stateの不整合を無視せず、元case条件に従い拒否又は期限内に新しい正規承認を要求する。

RETIREDは同じmodel ID/rootについて恒久終端。新規採掘、AMM再開、再登録aliasによる同一rootの復活、BurnResume又は通常BFT再採用による解除を禁止する。旧root/登録者/名前を変えただけの状態移植も認めない。異なるモデルの実質的同一性を完全判定する制度は導入しない。

## 5. BFT廃止でburnする資産

### 5.1 永久poolのMISAKA

対象は当該非償還poolに**現存するreal MISAKA reserve rの全額**。過去のLP20%拠出累計ではなく、過去の内部購入・通常売買等を反映した現在額である。virtual reserve、receiptの累積値、未入金予定額を含めない。

外部注文escrow、検証/BFTの役務予約、既確定の支払出力はpool残高に混ぜない。対象rを供給から一度だけ消却し、終了市場のspendable reserveを0とする。通常live市場のseed下限はこのburn専用終端に限り適用を終了する。元のseedと拠出履歴は監査用に保持し、LPの引出し又は第三者への払出しには変換しない。

pool内POSITIONは閉鎖市場の凍結在庫として保持する。既確定holder POSITIONも数量を没収しないが、同poolのswapと換金はできなくなる。虚構のvirtual reserveから売却額を払わず、他モデル資産で補填しない。未確定採掘POSITIONだけは次の区分でretireする。

### 5.2 登録者・採掘bond

当該モデルへ責任が残るMODEL_REGISTRANTとMODEL_MINING lotの**現存残額を100%burn**。Court等により既に消費した同じatomsを再度burnしない。資金源とlotを重複なく列挙し、他model、未割当、PANEL_VERIFY、GATE_C_VERIFYを除外する。

登録者不在ならその登録者bondは0であり、BFT又は管理者へ架空のbondを請求しない。新規登録で必要な登録者bond額・責任期間は別の事前認証policyに固定し、今回の100%は没収率を示すもので、新しいbond額を捏造しない。

### 5.3 未確定採掘報酬

| native状態 | 廃止時の処理 |
| --- | --- |
| 未発行の採掘予算・権利 | 全取消。実際の通貨burn額とは別計数 |
| 未確定採掘用の実MISAKA escrow | 全残額burn |
| 未確定採掘用の実POSITION escrow | 全数量retire。時価へ換算してMISAKAを作らない |
| 未commitのquote・受取予定 | 予定を取消。存在しないPOSITIONをburnしない |
| 既にrへ入っている購入/LP資金 | §5.1で処理済み。元R/Cの額面を追加burnしない |
| 既確定の採掘報酬出力 | 本項の一括burn対象外。個別Court責任は残る |
| Panelの成立報酬・裁定待ち専用予約 | 採掘報酬と別管理し、一括burn対象外 |

未確定/確定の判定は元のasset別award・権利確定境界へ結合し、単なる未引出し又はliability hold中というだけで既確定支払を未確定へ戻さない。状態境界が未定義なproducerをBMR1で有効化しない。

## 6. 検証者保護とCourt責任

Panelの全検証lotは、BFT廃止で一律に対象外。BFTは「正常か」を分類しない。既存の虚偽承認、資料義務違反、期限違反その他の個別offenceは、Court・反証・slashの元条件で裁く。

正当に成立したPanelのMISAKA報酬権は支払を維持する。争われている検証役務の資金も専用escrow/予約へ残し、Courtの結果に従って元契約で精算する。廃止により検証報酬を自動的に「不正」としない。

BFTとCourtが同じ資金を参照する場合も、native順序・remaining lot・一回消費を用いる。罪の成立、未回収債務、回収済みslashを別recordとして保ち、資金がなくても架空の回収・報酬を発行しない。

## 7. Court slashの49%発見報酬・51%burn

### 7.1 原資と条件

正規Courtで個別offenceが成立し、元の責任上限内で**実際に回収できたMISAKAのpenalty**だけをSとする。単なる告発、未決・却下・虚偽case、未回収債権、quote額、未発行取消、POSITION時価、BFT廃止burnはSに含めない。元の返還元本や未使用escrow返金はpenaltyとは区別する。

\[
Q=\lfloor490S/1000\rfloor,\qquad Burn=S-Q.
\]

累計実回収MISAKAの49%をfloorした額が適格な発見・立証者全員の報酬枠である。残差をburnする。通常のPanel役務5%は別原資とする。

### 7.2 受取資格と配分

元offenceの有効な証拠と提出者identityを認証して受取planを固定する。選出外の提出者も同一条件で適格となる。受理順、重複排除、重み・端数・期限を事前の版付きplanに結ぶ。自己申告、証拠の複製、受取人の事後差替えによる支払を拒否する。

被告と同じcanonical責任identity及び認証済みaliasは、そのoffenceの発見報酬を受け取れない。関連鍵・実質所有者の全てをchainから判別できるとは主張しない。資格・配分planが成立しない場合は架空recipientを作らず、有限の専用予約で正規解決を待つ。期限までに有効な受取権が成立しない部分はburnし、BFTやMinerへ付け替えない。

有罪判定・slashの回収自体は適格な発見者の存在に依存させない。自白・自動検出又は不正な受取申告でも客観的責任は残り、Qの支払だけを別の資格判定で制約する。Qの受給資格・分配をnativeで認証できるまで新49%経路を有効化しない。複数の有効発見者がいる場合も合計Q以内であり、同じ証拠の複製、再提出、別ticket、同じoffenceの別IDで49%を重複請求できない。

### 7.3 分割回収と保存則

同一offenceの回収総額をS_e、発見報酬総枠を `Q_e=floor(490*S_e/1000)` と記録する。新たにΔSを回収した際は、`ΔQ=Q_new-Q_old`、今回burnは `ΔS-ΔQ`。各回に49%floorを独立適用せず、回収分割で総額が変わらないようにする。

複数受給者へ追加払いする場合は、今回の増分ΔQだけを固定planで配る。累積Qを毎回最大剰余法で再配分し、既払額を減らすことはしない。又は有限回収終端まで専用escrowへ積み、一度だけ配る版付きplanを用いる。

collection ID・資金源・元lotを一回だけ消費し、offence・責任上限・受取planは固定する。原資回収、Qの支払又は専用予約、残りのburn、累積会計を同じjournalでcommitする。未確定報酬を繰り返し「slash済み」と申告するだけで原資を増やさない。

回収S=100 MISAKAの成立caseならQ=49、burn=51。S=0なら両方0。自己所有の別identityで発見者を装うSybilを完全に識別する保証はなく、不正利益・委任bond・外部取引もあるため、この率だけで全ての共謀が不採算になるとは主張しない。

51%残差は回収時のinitial burnである。正規受取者が成立せずQの予約が期限でburnされる場合、そのterminal burnを別計数する。常に `S=paid_finder_reward+remaining_reward_escrow+total_burn` とし、未受給分を消しただけで会計から失わせない。適格者が受け取らない場合の最終burnは51%を超え得る。

## 8. 原子性・供給・再生

廃止のstate終端、pool real reserve burn、対象lot消費、未確定報酬処理、認証済み凍結在庫・tombstone、一次キーを同じnative journalでcommitする。列挙漏れ、重複source、用途曖昧、上限超過又はoverflowなら全体未commitとして拒否する。途中burnだけを残さない。

`BurnedMISAKA`、`RetiredPositionAtoms`、`CancelledUnissuedReward`、`CourtFinderRewardMISAKA`を別計数する。初期残高から実資産を消費するか正当な支払へ移す保存則を守り、未発行取消を供給減へ混ぜない。BFT受取及びモデル廃止発見報酬は常に0。

Court回収が先に確定したSと成立済みQは後のBFT一括burnへ戻さない。BFT廃止で先に消費したlotは後のCourtで再回収済みと数えず、残責任を記録できても実回収0なら追加報酬0。同一blockのnative順序とCourt専用予約を固定し、両purposeで同じ資金を消費しない。

同じretirement/court collectionを二度消費しない。canonical reorgではreceipt、資金、lot、支払、burn累計、モデル状態を同じjournalで正確に戻す。旧署名のpurposeを新廃止又は49%請求へ再解釈しない。

## 9. 有効化の条件

旧資産・既存契約への適用範囲は[移行契約](../governance/model-retirement-transition.md)で固定する。設計採択だけで既存poolを焼却しない。新state/domain、完全なpurpose別lot分離、旧市場の保全、検証報酬予約、Courtの実回収ledger、受給資格、case期限、source一回性、reorg/IBDと全consumerの接続が必要である。

参照検査は用途別の抽象会計と失敗原子性を示す。実BFT投票・署名・Court判定・native資金・本人性・network処分を完了した証拠ではない。

## 10. 可逆burn-pauseと独立安全停止

Pauseは正規支払者署名と規定停止burnを要する。Resumeはnative登録者の正規認証又は専用BFT BurnResume承認に加え、規定解除burnを要する。creator不在genesisにも専用BFT経路を使い、通常のモデル採用YESを流用しない。

burn-pauseのsequenceと前操作、旧確定PAUSED状態を保存し、旧pendingは元policyでdrainする。BurnResumeは独立MODEL_STOP、元claimの期限・責任、RETIREDを解除しない。burn額をゼロや任意値へ変更しない。

独立BFT safety stopは正確なmodel/artifact/profile/rule scope、原因、直前record、generationへ署名する。正規解除には現在権限と対象StopId集合を必要とし、同batchの新停止を優先する。元genesisを含む全モデルに同じ規則を使う。
