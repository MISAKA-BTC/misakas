# RFC-0057 — Court nonce・Panel固有チャレンジと提出の一回性

> **2026-11-18 PALW-AN1追補（DRAFT_NOT_ACTIVATED）:** [RFC-0059](0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md) に基づく元実入力・元claim・局所反証の接続は本書§17による。Minerのowner/nonceは実入力へ束縛し、Panelのnonceは提出責務へ束縛する。Panel完全再実行＋Courtを維持し、Groth16等の計算validity proofへ置換しない。旧署名・旧claim・過去の試験結果は保持する。

> **2026-11-17 監査対応改訂:** [RFC-0058](0058-staged-work-commitments-and-consensus-eligibility.md) / [ADR-0226](../adr/0226-staged-work-commitments-and-consensus-eligibility.md) の新SWC1 profileとの接続は§16による。nonceはCourtの提出責務を結合するが、全推論の物理的実施やPoW費用下界を証明しない。以前の49件の抽象試験は改訂前snapshotの記録として保持する。

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**。ユーザーのRFC記述依頼を具体化した設計案。正式採択、native実装、実測、公開activationとは区別する。
- 判断記録: [ADR-0225](../adr/0225-court-nonce-and-panel-challenges.md)
- 基準source: MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14。直前のRFC-0055/0056文書成果を継承する。
- 新policy呼称: CourtNonceBindingV1 / CPN1。既存wire ID、署名domain、CaseId、WorkId、旧claimの意味を上書きしない。
- 規範入口: [RFC-v2-0017](../rfc-v2/0017-noninteractive-positive-verification.md)、[0018](../rfc-v2/0018-interactive-negative-court-and-defaults.md)、[0019](../rfc-v2/0019-model-free-node-and-bounded-evidence.md)、[0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)。

## 0. 目的と保証の境界

Courtを別claim・別Panel・別roundの提出物として使い回すこと、同じ役務を別nonceや別署名で再度数えることを防ぐ。positiveにはPanelごとに導出する予測不能な局所challengeを追加し、本人の肯定承認を数える条件へ接続する。

nonceは**提出と責務のbinding**である。元Minerのcanonical整数計算、execution root、Merkle leafへPanel固有saltを混ぜない。Nodeがモデル本体や全traceを取得する構成、モデル別benchmark・審査、計算正当性の暗号学的証明を追加しない。

共有できるopening・重み断片・局所演算資料と、署名者の承認責任・提出の一回性・報酬は区別する。nonceが違っても抽出対象が重なることはある。対象空間が小さければ全Panelの対象集合が一致し得る。同じ証拠bytesという理由だけで拒否・有罪・コピー認定を行わない。

本書は、各Panel本人のCPU/GPU使用、全体再計算の実施、非共謀、外注不在を証明しない。公開資料から自分の局所対象だけを処理して虚偽の完全承認を出す残余リスクは、nonceだけでは消えない。

AN1のMiner試行nonceはこのCPN1 nonceと異なり、§17で定める実入力の一部へ変換する。CPN1のPanel nonceを元Minerの実行へ混ぜないという上記規則は維持する。AN1における誤実行の阻止は、能動的な正直Panelの全scope検査、資料取得、期限内の有効反証包含とFinal gateを条件とする。

## 1. 現行設計と変更点

| 既存契約 | 維持するもの | CPN1で具体化するもの |
| --- | --- | --- |
| 0017の元claim/root/profile/ruleset/drawへの署名binding | 元contextとscope、完全承認と局所検査の区別 | nativeな義務slot、外側nonce、完全な署名subject、一回性 |
| 0017の公開乱数とcanonical局所coverage | 共通の必須対象集合、同一資料の共有、被覆の重複排除 | 各署名者のpositiveに追加する個別challenge |
| 0050のPublicOneHonest | 完全承認最低1件、全選出者の完全検証責務、全体検査/異議期間、Q解消 | 本人の個別challenge完了をその承認の計数条件にする |
| 0018の単独反証・有限case | sample外の不正も反証可能、適格第三者、元時計 | negative/応答のactor・case・phase・step nonce |
| shared court/material/service | 正しい資料の共有、共通quota・費用・offence once | nonce・Panel・relayで共有quotaや債権を作り直せない規則 |
| 0055/0056 | 全工程予約、Node上限、全新規claim共通D、外側合計1 BPS | nonce/selector/追加資料/保持/復旧の費用を元計画へ含める |

既存sourceにも署名scope、永続nonce、canonical共有要求、conviction/receipt一回性がある。ただし、それぞれの用途をCPN1全経路の完成と扱わない。旧EXACT_STREAM positiveのPanel所有rootを、新profileの元Miner rootへ読み替えない。[監査](../maintenance/revisions/2026-11-17-court-nonce/SOURCE_AUDIT.md)に実在箇所と不足を記録する。

既存challenge/beacon実装はcanonical work由来の乱数であり、「BFT委員会が発行するnonce」という実装事実は確認していない。本書は既存profileが認証するpost-commit beacon/anchorへ接続する。新しいBFT委員会を設けない。

SWC1の追加能力契約は0058 §6に従う。そこでのthreshold beaconはDNS-BFT廃止・PQ-only方針との判断を要するDISABLEDな参照候補であり、本書への追記だけでは採択も実装もされない。既存beaconが段階ごとの未知性を満たすと推定しない。

## 2. 三つの同一性と別の支払い台帳

| 名称 | 意味 |
| --- | --- |
| ObligationKey / ChallengeSlotKey | native状態から決まる一つのactorの論理義務。一回性を判定する |
| CourtSubmissionId | その義務に対するcanonical unsigned提出内容。署名はこのsubjectを認証する |
| EvidenceMaterialId | 元root・対象・型・値/opening等に基づく共有資料の同一性。nativeの既存material IDを保持する |
| native Reward/Receipt/OffenceOnce | 支払い・役務・制裁の一回性。CPN1のIDから新たな権利を生成しない |

異なる署名bytes、relay、carrier、batch、fragment、表示model名を、別の義務・票・報酬へ変換しない。完全承認を数える主体はnative bond等の既存principalであり、keyの再発行や同一bondの重複draw位置を独立主体へ増やさない。

一つの義務には一つの完成した提出を受理する。同一提出の正当な再送は元receiptの参照または無効果とし、二重承認・時計再開・資源予約・支払いを発生させない。実際の搬送・検査コストが消えるとはしない。Nodeのlocal入口上限と既存の認証済み役務費規則で扱い、第三者のpacket replayを新たな被害者債務へ変換しない。

## 3. claimから義務を導く

### 3.1 authoritativeな期待値

Nodeは受理済みclaim、AcceptedOrigin、元Miner commit/root、profile、ruleset、draw snapshot、assignment/duty、actor資格、Case状態、計画、資金/責任、元期限から期待値を導く。packetの自己申告だけで期待root・Panel・roundを決めない。

actorはnativeに解決したbond/principalとroleである。実際の署名keyはそのroleのnative認可から得る。key rotation、同じbondの複数抽選位置、relay代理搬送は元principalへ正規化する。選出Panelと第三者の資格を同一に偽装しない。

round、action_kind、stepはnativeの有限状態機械が許す値のみである。初回positiveのroundを送信者が増やす、無応答・失敗・期限切れで抽選をやり直す、phase名を変えて別義務を作ることを禁止する。新roundが必要な別契約は、claim受付時に有限の上界・費用・時計・遷移を確定している場合に限る。

### 3.2 ObligationKey

H256はSHA-256、LP(x)=u32le(len(x))||x、NR(tag,v,x)=LP(UTF8(tag))||u16le(v)||LP(x)とする。NR内のnative bytes・ID幅・absence encodingは元の版を保持する。

~~~text
ObligationKey = H256(
  LP("MISAKA/PALW/CPN1/OBLIGATION") || LP(network_id) ||
  NR("claim",...) || NR("origin",...) || NR("actor",...) ||
  NR("duty",...) || u64le(round) || u16le(action_kind) || u64le(step)
)
~~~

dutyは既存のassignment/Case/役務から導く一意の義務であり、clientが選ぶnonceではない。root/profile/seed/署名/bodyを変更しても同じnative義務を追加消費できない。それらの正当な期待値はContextHashで別途固定する。新profile・復旧世代へ移るだけで元義務の消費済み状態を失わせない。

完全承認と後続の適格negativeは別のnative義務である。positiveを提出したことを理由に、同じactorの訂正notice・反証を拒否しない。既に成立した肯定署名の責任は残す。

## 4. nonce・context・署名の正確な定義

### 4.1 ContextHash

次の順序を固定する。最初のschemaはu16le(1)、networkはLP。表内のnative fieldは示すtagのNRで包む。

~~~text
ContextHash = H256(
  LP("MISAKA/PALW/CPN1/CONTEXT") || u16le(1) || LP(network_id) ||
  NR("claim",...) || NR("origin",...) || NR("miner-commit",...) ||
  NR("execution-root",...) || NR("program",...) || NR("model",...) ||
  NR("input",...) || NR("output",...) || NR("profile",...) ||
  NR("ruleset",...) || NR("draw",...) || NR("assignment",...) ||
  NR("actor",...) || NR("role",...) || NR("duty",...) ||
  u64le(round) || u16le(action_kind) || u64le(step) ||
  NR("plan",...) || NR("fee-liability",...) || NR("deadline",...)
)
~~~

nativeに非適用なdraw等は、当該route/型のcanonical absenceを使う。第三者を架空のselected Panelへ変えない。fee-liabilityは署名した料金scope、原資、責任担保、解放条件に束縛し、deadlineは既存の時計/有効期間を含む。これらの実preimage・資格・資金・同意も検査し、hash一致だけで済ませない。

NR("plan")は箇所抽出前に固定するPlanSpecであり、対象空間・認証index・selector版・k/R・共通coverage規則・最悪費用上界を含む。導出後のnonce・個別target一覧・TargetSetDigestを含めない。fee-liabilityも事前契約の原資・上限・責任条件を束縛し、抽出後の実費結果は別台帳へ記録する。assignment等のnative adapterにCPN1導出結果への逆依存がある場合は不適合とし、Context→nonce→target→Contextの循環を作らない。

新digestは32 bytes。LP最大長、総bytes、要素数、深さ、署名本数、展開量、参照回数には共通profileの有限上限を設定し、allocation前に検査する。u64/u32/u16のoverflow、未知field、非canonical整数、曖昧な文字列連結を拒否する。旧native IDの切詰めは禁止する。

### 4.2 positiveとnegativeのnonce

~~~text
PositiveNonce = H256(
  LP("MISAKA/PALW/CPN1/POSITIVE-NONCE") ||
  ObligationKey || ContextHash || NR("beacon-anchor",...) || NR("beacon-seed",...)
)

ActionNonce = H256(
  LP("MISAKA/PALW/CPN1/ACTION-NONCE") ||
  ObligationKey || ContextHash
)
~~~

positiveは元commit後の認証seedを使う。negative・資料要求・応答・終端のreplay bindingにはActionNonceを使い、新しい乱数待ちを追加しない。nonceの予測不能性が必要なのはpositiveの検査箇所選択であり、既存stepの再送防止には一意のscopeと署名・台帳が必要である。

beacon-seedは認証したnative codec/versionと全bytesをNRで包む。監査したchallenge実装のDigestは64 bytesであり、その全体を保持する。32 bytesへの切詰め、任意seed変換、旧beacon contextを外した自己申告seedは不可。CPN1がH256で新たに作るnonce/digestの32 bytesと、元seedの幅を区別する。

送信者が指定するnonce探索、任意salt、独自seed、block hash・署名bytes・local乱数へのfallbackを認めない。nonceはNodeも再計算できる。CPN1 nonceをCAD1の受付ticketや既存REAL nonceへ流用しない。

### 4.3 SubmissionSubject

~~~text
TargetSetDigest = H256(
  LP("MISAKA/PALW/CPN1/TARGETS") || u32le(count) ||
  concat(NR("target",...) in canonical order)
)
BodyDigest = H256(LP("MISAKA/PALW/CPN1/BODY") || LP(canonical_body_bytes))
SubmissionSubject =
  LP("MISAKA/PALW/CPN1/SUBMISSION") || u16le(1) ||
  ObligationKey || ContextHash || CourtNonce ||
  u16le(statement_kind) || u32le(body_length) || BodyDigest || ChunkRoot || TargetSetDigest
CourtSubmissionId = H256(SubmissionSubject)
~~~

CourtNonceは当該actionに対応するPositiveNonce又はActionNonce。statement_kindは新codec/profileで登録するscopeの識別子であり、旧global wire番号を割り当て直さない。意味は少なくとも全体肯定、局所関係、適格notice/反証、応答を区別する。型の既定値で完全承認へ昇格させない。

署名はSubmissionSubjectを認証する。署名bytes・carrier・relayはCourtSubmissionIdに入れないが、署名の正当性と現在のnative認可は必要である。複数roleが必要な処理は各roleの同意を元契約どおり要求する。集約署名だけでrole別責任を消さない。

bodyのcanonical codec、targetへの資料対応、chunksの順序/長さは共通profileで固定する。canonical body_lengthから有限part数を導き、任意のfragment分割で新しい論理提出IDや役務を作らない。bodyのhashだけでopening・算術・全必要対象を検査済みとしない。

ChunkRootは署名された輸送内容の32-byte commitmentであり、flatなBodyDigestとは別である。profileが固定するchunk幅B>0によりn=max(1,ceil(body_length/B))個へ分ける。最後以外はB bytes、最後は残長、長さゼロなら一つの空chunkとする。全体のbytes/part数上限を先に検査する。

実leafはH256(LP("MISAKA/PALW/CPN1/CHUNK")||u32le(i)||LP(chunk_i))、n以上のpadding leafはH256(LP("MISAKA/PALW/CPN1/CHUNK-PAD")||u32le(i))。最小の2冪leaf数へpaddingし、親はH256(LP("MISAKA/PALW/CPN1/CHUNK-NODE")||left[32]||right[32])とする。n=1のrootはそのleaf。openingの方向はindexから導き、厳密な木の高さとchunk長を検査する。別bundleのchunkやindexを混ぜない。

全chunk完成時にはcanonical順に連結し、body_length、flat BodyDigest、codec、対象対応も照合する。片方のdigestだけで完成にしない。chunk木は輸送bytesの認証であり、元Minerのtrace木やモデル算術の木を変更するものではない。

## 5. post-commit乱数とPanel固有challenge

元claim/root、profile、対象空間、pool snapshotとdraw規則をchallengeに使うseedより先に固定する。実際のPanel割当は既存native drawから一意に導き、確定後のassignment/actor/dutyをContextHashへ結合する。

既存drawと同じ認証seedを使うprofileは、Panel抽選と箇所抽出のdomainを分離する。同じ乱数源を使うことから無条件の統計的独立性を主張しない。別seedを使うprofileは、その唯一のanchor選択規則、到達期限、伝播・証拠生成・包含の余裕を受付時に予約する。

SWC1で共有できるのは、受理済み対象集合／poolを閉じた**後**のR_panelである。先に公開したR_jobやR_ticketをhashし直してこの未知性を代替しない。R_ticket判定により受理対象を選ぶことは正規順序であり、禁止するのはR_panelを知ってからその対象集合・pool・roundを選び直すことである。

元commit前に予測可能なseed、送信者が都合よく選べるanchor、失敗後に候補seedを選び直す構成は不可。seedの予測不能性・bias resistance・native lockの実接続はactivation gateであり、H256を置いたことだけで成立したとしない。

seed不成立時はそのpositive計画を利用不能/未検証として扱う。無料の再抽選、同一義務の別round移動、Panelの算術的有罪、既受理negativeの停止へ変換しない。元時計を再開しない。他の適格な承認と共通coverageで既存Final条件を満たせる場合まで、無回答者のために新しい全員待ち条件を作らない。

## 6. 有限の箇所選択

profileは認証された対象空間N、canonicalなindex→局所関係の対応、必要な個別箇所数k、range抽出の試行上限Rを定める。1<=N<=2^64-1、1<=k<=N、1<=R<=2^32-1とし、さらに実測済みの小さい共通資源上限を必須にする。この型の最大値を実運用予算として採用しない。

Nと対応関係はprogram/shape/trace構造から有界に検査する。Nodeへ全モデル/全traceの走査を要求するrank/select実装は不適合。model別審査は設けず、共通kernel/checker版ごとにこの能力を検証する。

無制限の「重複なら引き直す」loopを避け、疎な部分Fisher–Yatesを使う。

~~~text
M = empty finite map; selected = []
for i = 0 .. k-1:
    n = N-i
    limit = 2^256 - (2^256 mod n)
    for a = 0 .. R-1:
        v = uint256_be(H256(
            LP("MISAKA/PALW/CPN1/DRAW") || PositiveNonce ||
            u64le(i) || u32le(a)
        ))
        if v < limit: accept v; break
    if no v accepted: SelectionUnavailable
    j = i + (v mod n)
    selected.append(M.get(j,j))
    replacement = M.get(i,i)
    if j != i: M[j] = replacement
    M.remove(i)
return sort(selected)
~~~

2^256とlimitの一時表現にはcheckedな256bit超の固定幅又は等価な境界処理を用いる。2^256をゼロへwrapしない。各drawのhash試行は最大R、全体最大kR、mapの要素数はO(k)。比較・map・sort・対象導出・opening・演算の最悪費用も計上する。map実装のCPU上界はO(k)空間だけから推定しない。

拒否法はrange mappingのmodulo biasを避ける。均等なhash出力という仮定の下で、完了した抽出の条件付き分布を評価する。有限Rで失敗する可能性をゼロと扱わない。SelectionUnavailableは同じseedで一意に再現し、無料のround/seed変更や有罪判定を起こさない。残りRをresetする再送も認めない。

sorted indexからcanonical targetを導き、challenge_indexはこの順序の位置とする。同じPanelの対象は重複しない。別Panel同士の重複は許す。k=Nなら全員が同じ対象集合になり得る。nonceの相違は内容の相違を保証しない。

## 7. 共通coverageと本人の肯定承認

既存0017のclaim共通の必須局所coverageを残す。CPN1個別対象は、そのPanelの新しい完全承認を数えるために追加する。共通coverageを個別sampleへ置換して省略しない。

~~~text
AdmissibleNewPositive(actor) =
  native selected eligibility and correct full-replay statement scope
  AND valid signed CPN1 subject and unspent obligation
  AND all that actor's required challenge targets locally verified
  AND native funds/liability/deadline/other existing approval conditions

CountablePositiveRecord =
  authenticated AcceptedPositiveRecord
  AND matching atomic once-consumption of its ObligationKey/SubmissionId
  AND still-valid native approval/liability/claim conditions
~~~

未消費条件は新規受理遷移の前提である。受理時に義務消費とAcceptedPositiveRecordを原子的に作り、その後のFinal/再送/復旧では対応する既受理recordを数える。受理後に未消費を再要求して承認を消さず、受理済みrecordの自己申告コピーを新しい票へも変換しない。native上の資格・責任の有効期間を保持し、単なる署名key更新で過去の認証済み承認を別主体へ移さない。

このpredicateが成立しても、Nodeが本人の全体再計算を観測したことにはならない。全体肯定は責任付き署名命題であり、局所一致はその対象についての検査結果である。

共通coverageは認証されたtargetの集合で数え、同一targetの複数opening・複数Panel署名で未検査対象を埋めない。個別sampleと共通targetが同一なら、同じ認証資料を両方の局所条件に参照できる。両者の条件を別記録とし、同じ演算を二重の仕事量・権利へ変換しない。

PublicOneHonestの完全承認最低1件と、担当者全員の完全検証責務を維持する。全員分の個別sample提出をFinal条件にしない。未提出の他Panelだけを新しいQへ入れず、過去の無回答契約がある場合はそのnative規則で別に扱う。最初の承認で全体検査/異議期間を短縮しない。

全選出actorが正当に提出する場合の潜在費用は事前予約する。一人分しか予算を確保せず、残りの適格な反証・承認を資源不足で排除する構成にはしない。

## 8. Nodeの受理順序と部分提出

1. サイズ・版・count・展開量・参照上界をallocation前に確認し、bounded ingress leaseを取る。
2. 受理済みclaimとnative義務を解決し、期待Context/nonce/actor/期限、署名・role同意・資金認可を検査する。
3. 同一義務の状態を確認する。受理済みの同一subjectは再送として扱い、別subjectはSlotConflictへ分類する。
4. 必要な個別targetを有界に導出し、対応する認証資料・元rootへのopening・canonical整数演算を検査する。
5. 共通coverage、本人の個別完了、完全承認の計数資格、資金・責任・既存receipt条件を照合する。
6. 一回性、必要資源debit、material参照、承認、責任担保、既存receipt、journalを同checkpointで確定する。

認証前のpacketや不正openingで、他人のObligationKeyを消費しない。未認証資料はboundedなlocal隔離領域だけで扱い、canonical票・資金・義務・Qを変更しない。

大きいbundleの輸送では、profileで固定したcanonical chunks、body digest、長さ、chunk index/openingを用いる。authenticated header受理時には有限のstaging予算・期限を確保し、同義務に保持できる候補header数も有限とする。headerの受理だけで完成票や報酬を作らない。未完成の別bundleを混ぜて完成扱いにしない。

完成したvalid bundleだけが義務を消費する。適格な本人によるstagingの取消/置換は元native契約の有限条件と費用内に限り、nonce・target・元時計は維持する。検査失敗が資金/処理を無限に再利用する経路にならないよう、認可した役務機会とlocal予算を別に計量する。

同一slotでbody digestが異なるだけでは、相互排他的な署名命題が成立したとは限らない。拒否・競合記録と、署名された矛盾を用いるequivocation裁定を分ける。誤署名、規格不適合、後日の訂正を一律のコピー罪へ変換しない。

## 9. negative・資料要求・応答・終端

適格な不一致notice・直接反証は、本人のpositive sampleに含まれない対象でも受理する。negativeは既存局所化、元root、資格、料金、case/phase/step、期限の規則で対象を決める。

各受理operationの外側へActionNonceを結合し、別Case・別actor・過去stepへの移植を拒否する。初回noticeでCaseIdがまだないrouteは、元claimとnativeが認証するnotice義務/固定対象から導く。架空CaseIdやclient nonceを新しいauthorityにしない。Case生成・nonce消費・原資・予約を同checkpointで行う。

共有済みの有効な反証はrelay・再参照できる。署名者・原提出者・既存の受益者を運搬者へ付け替えず、同じ客観的反証から二重のbounty/convictionを作らない。新しい正当な第三者のcase参加は、既存の資格・有限共有予算・帰責規則による。

positiveの再提出、別nonce、多数署名でQ(C)を消したり、ComputationRefutedを取り消したりしない。同じcanonical処理区間のnotice・裁定・期限・Finalの順序は0020を維持する。nonceを理由に新seed、CAD1 ticket、新ExecCreditを既受理の救済へ要求しない。

## 10. 肯定署名の責任と報酬

| 観測できる事実 | CPN1での扱い |
| --- | --- |
| 過去claim/別Panelの封筒を移植 | native scope/署名/nonce不一致として拒否 |
| 同一義務の同一subjectを再送 | 再承認・二重報酬なし |
| 正しい公開資料を自分の対象に使い正しく署名 | 局所条件は検査可能。コピーや独立実行の有無を推定して罰しない |
| 全体肯定を受理した後、同じ元claim/root/scopeへの有効な反証が確定 | 事前に署名したnative責任契約に基づく虚偽肯定の客観的帰責 |
| LocalHoldsと別対象への反証 | 局所命題を全体肯定に拡大して罰しない |
| root不一致の申告だけ、資料不足、seed停止、service不足 | それだけでMiner/Panelの算術的有罪にしない |
| 同じslotの異なるbody | SlotConflict。相互排他的な署名命題が立証される場合だけ既定のequivocation手続きへ |

全体肯定への責任は、署名した元subject、認可principal、同じnative claim/root、客観的反証、適用policy、責任期間・資本を照合する。未採択の罰金額、無限責任、勝手な全額slashを追加しない。事前profileに有限の責任額/担保、notice機会、処理期限、offence once、精算手順がないscopeは数えない。

担当Panelの完全再計算義務と、観測可能な虚偽肯定の制裁を区別する。正しいclaimをコピーして肯定しただけでは、手抜きを立証したことにならない。nonceによる追加検査のコストが全体再計算と等しいとも仮定しない。

完全肯定の責任担保は、全合法な最遅反証・帰責・精算に足りる期間保護する。countable承認の直後に資本を解放して反証時に回収不能となる設計を認めない。pending caseや未払dueがある間、元の責任floorを維持する。期間・額・保管・裁定費が不足する計画は新規受理しない。

CPN1を通したという理由で新しい発行・仕事credit・bountyを作らない。支払いは用途別の既存entitlement、実原資、recipient、満期、Final、onceを満たす場合だけである。共有のmaterial/service quotaをactor nonceやrelayごとに増やさない。訂正や後続negativeは受理できるが、過去の全体肯定責任を消去しない。

## 11. 保存・再送・reorg・移行

| 状況 | 必須動作 |
| --- | --- |
| 再署名/別relay/別carrier/fragment再送 | 元subject・義務・target・receiptを保持し、追加票や権利を作らない |
| 通常cancel・期限切れ・pruning | 費用/責任/消費済み義務を復活させない |
| restart/IBD | claim版、期待context、seed anchor、計画、消費/承認/責任/receipt、未完caseを認証stateから復元 |
| 復元資料不足 | 保留/復旧/有限未検証終了。ローカル空cacheを未消費と扱わない |
| 正規reorg | 義務消費・資金・承認・case・coverage・支払いを同じcanonical checkpointでundo/replay |
| policy更新 | 既受理claimは元profile。新nonce版を旧署名へ遡及要求しない |

消費証跡はclaimの有効期間・役務再生・IBDに必要な認証情報へ残す。raw evidenceを回収しても、その後に古いscopeを受理可能にしない。終端claimのauthenticated拒否状態、必要なtombstone/compaction、参照情報のサイズ・復旧・GCをstock予算へ含める。永久無制限setや全model走査を前提にしない。

正規reorgでundoされた履歴は新canonical履歴で一回性を再判定する。排他的forkをまたぐ永久local spent fileを合意状態の代用にしない。旧branchのworker結果・partial bodyは世代/anchorを照合して破棄又は再検査する。全forkの検査負荷はNodeのlocal上限にも従う。

新profileでCPN1を要求するclaimを、旧codec/別route/低いversionへ落として受理しない。旧claimの再生・既存救済は当時の規則で検証する。番号・domain・codec・actor責任の正式登録とactivation recordを別工程で行う。

## 12. 0055/0056・多数model・1 BPSとの接続

nonce導出、最大kR hash、target index、全選出actorの個別資料、共通coverage、署名、opening、局所演算、部分保持、失敗後処理、責任台帳、restart/IBD/GCをRFC-0055の全工程予約へ含める。positive費用がnegative/第三者/終端の保護枠を使い切らない。

RFC-0056のD_admitは新originalが責任を作る前に働く。CPN1は受理済みclaimの役務を認証する。CPN1の成立を新original受理の前提にして、AcceptedOriginとの循環を作らない。受付時には将来必要なCPN1役務を予約し、後から元contextに基づき確定する。

Panel固有nonceやchallenge_indexから追加の新規claim ticket、外側carrier permit、1 BPS枠を発行しない。全model共通の資源予算と外側総搬送1 BPSを維持する。court件数・Panel人数・sample数を本書で1/3/5等へ固定しない。

1万/5万modelでも個々のmodel別nonce台帳や別容量を無条件に増やさない。必要なactive claim/actorに限った認証索引を使い、その保持量自体を計上する。全modelを毎epoch走査せず、Nodeの全モデル保持・全推論へfallbackしない。

## 13. profile必須欄と採択範囲

| 共通profileの必須欄 | 未設定の場合 |
| --- | --- |
| native field/absence codec、domain、scopeとrole認可、全サイズ上限 | 新CPN1受付不可 |
| 既存post-commit beacon/anchor・lock・draw binding・ready期限 | positive計画不可。既受理negativeの新seed待ちは作らない |
| Nの導出/target index能力、k、R、必須共通coverage、計算/byte/保持上界 | 新claimの計画不可 |
| 全actorの最大役務数、round/step/fragment/staging上限・取消条件 | 新規の無制限責任を作らない |
| 資金・費用・責任額/期間・native once・原子的undo/復旧 | 完全承認の計数/支払い不可 |
| 全入口adapter・旧版共存・Final/Q/反証優先 | 未接続routeをdefault deny |

既存採択本文を今回の追記で変更済み・実装済みと扱わない。例外として提案するのは、新profileの署名scope/envelopeと本人別challenge計数条件、有限なnonce/義務台帳、その費用/責任接続である。global pool、元整数算術、モデル二階層、元WorkId、Panel完全検証責務、PublicOneHonest、単独反証、既存経済条件は維持する。

## 14. 必須受入試験

| ID | ケース | 必須結果 |
| --- | --- | --- |
| N01 | claim/原root/Panel/role/profile/round/step/seed/anchor変更 | 期待contextと署名/nonceに不一致、別権利なし |
| N02 | 同一subjectを再署名・別relay/routeで再送 | 一回の承認/receipt/資金効果 |
| N03 | 同slot別body、同一意味の別encoding | canonical違反又はConflict、bytes差だけの自動slashなし |
| N04 | 不正署名/未認証fragment/不正openingを先着 | 正直なactorの義務・資金・Qを消費しない |
| N05 | 対象空間小、別Panel対象重複、k=N | 共有資料を許し、coverage/仕事/報酬を水増ししない |
| N06 | 他Panelの封筒をコピー | actor/nonce/署名不一致で拒否 |
| N07 | 正しい公開openingを自分の対象へ利用 | 局所検査は通り得る。独立実行証明のPASSにしない |
| N08 | nonceをMiner trace/leafへ混ぜる | 元root/算術規則不一致として拒否 |
| N09 | bounded rejectionを強制的に全失敗 | SelectionUnavailable、kR上限、無料reroll/有罪なし |
| N10 | 候補nonce/任意round/seed差替え | native義務からの期待値に不一致 |
| N11 | partial bundle混合、途中restart、欠損、取消 | 未完成を計数せず、資源・時計・onceを保存 |
| N12 | 共通coverage未達で個別sampleだけ完了 | Final不可 |
| N13 | 正直な一人が完了、残り不提出 | 他の既存成功条件が揃えば正常Final。新全員待ちなし |
| N14 | sample外の有効反証、第三者、肯定後の訂正 | 独立の適格negativeとして処理、新seed/Dなし |
| N15 | 多数positiveと反証/Qが競合 | 単独反証/未解決Qを優先。肯定再送で復活しない |
| N16 | 完全肯定後に同scopeの客観的反証 | native有限責任・offence once。copy推定罰はなし |
| N17 | LocalHoldsと別箇所の反証、seed停止 | 局所scopeを全体有罪へ拡大しない |
| N18 | 責任解放前の最遅case、同じ反証の複数relay | 責任floor・原資・bounty/制裁一回性を維持 |
| N19 | prune/復元/reorg/policy更新/旧claim | 通常処分で復活不可、正規undoは原子的、旧版は元規則 |
| N20 | model/actor/fragment数・fork洪水の増加 | 共通vector/stock/ingress内。nonceで枠を増設しない |

## 15. 実装・検証の完了条件

[実装接続表](../design/palw/integration/court-nonce-implementation-plan-2026-11-17.md)で入口・owner・永続状態・資金・Finalの接続を追跡する。抽象モデルはcanonical codec、実hash/selector、局所opening/整数関係、一回性、部分提出、共有、反証/責任の境界を検査する。native鍵/選出/clock/料金/全工程plannerをfixtureで置く箇所を明記する。

native全route、署名domain登録、実資本・journal・snapshot、postcommit beaconの安全性、対象indexとcost meter、最低Nodeの敵対的混合負荷、実LLMから完全承認/単独反証/Final/権利消費、長期保持・IBD、実1 BPSは別の完了証拠を要する。nonceの導入や抽象試験だけから「全Panelが実際に再計算した」「必ず一人は能動的な正直者」「Nodeは任意負荷で落ちない」を結論しない。

[改訂記録](../maintenance/revisions/2026-11-17-court-nonce/README.md) / [独立レビュー](../maintenance/revisions/2026-11-17-court-nonce/REVIEW.md) / [検証記録](../maintenance/revisions/2026-11-17-court-nonce/VALIDATION.md)。

## 16. SWC1との仕事・抽選・責任の接続

新profileだけに以下を適用する。旧CPN1のContextHashや署名bytesを改名して別の意味にしない。既存のObligationKey、共有EvidenceMaterialId、native Reward／Receipt／OffenceOnceは維持する。

~~~text
SWC1PositiveContext = H256(
  LP("MISAKA/PALW/SWC1/CPN-POSITIVE-CONTEXT") || u16le(1) ||
  CPN1ContextHash || OpportunityId || ComputationalWorkKey || OriginalId ||
  PanelFreezeId || NR("panel-beacon-anchor",...) || NR("panel-seed",...)
)
SWC1ActionContext = H256(
  LP("MISAKA/PALW/SWC1/CPN-ACTION-CONTEXT") || u16le(1) ||
  CPN1ActionContextHash || OpportunityId || ComputationalWorkKey || OriginalId
)
~~~

新署名envelope／nonceの版はpositiveとActionの分岐を明示する。positiveはSWC1PositiveContextをCPN1のPositiveNonce／署名subjectへ渡す。negative・資料要求・応答・終端はSWC1ActionContextをActionNonce／署名subjectへ渡す。CPN1ActionContextHashは§4.1の同じnative duty／case／phaseを使い、まだ存在しないdraw／assignmentは元actionで正規に定まるabsenceに固定する。未来のPanelFreezeIdやR_panelを要求せず、既受理actionのcontextへ後からseedを追記して差し替えない。

旧CPN1 contextを送るdowngradeを新originalへ許さない。native IDとseedの全bytesを保持し、元rootをPanel自身のrootへ置換しない。新contextのdomainが別でも、同じnative義務に対する承認・支払いをもう一度発生させない。selected Panel専用の新義務は正規assignmentが存在してから生じるが、その待ちを既受理originalの既存negative／資料case／第三者の適格actionへ転用しない。

positiveの§4.2 PositiveNonceへ渡す外側beacon-anchor／beacon-seedは、SWC1PositiveContextが束縛した同じPANEL stageの参照・R_panelと一致させる。別の認証済みbeaconを追加saltとして選ばせない。

1. 元Original・WorkKey・受理集合・pool・責務をR_panelより先に固定する。claim破棄、別alias、取得失敗、任意round、再署名でPanelを再抽選しない。
2. 個別challengeはその人のpositive計数条件であり、全員の提出を新たなFinal条件へ追加しない。全選出者の完全検証責務と、最低承認1件のPublicOneHonestを区別する。
3. 正しい局所資料の共有を許す一方、別人の署名封筒・nonceは流用できない。公開資料を使った局所計算だけで全体再計算を証明したとはしない。
4. 元sample外を含む適格negative、資料case、応答、終端は新D・新seedを要求しない。多数のpositiveより単独反証／未解決Qを優先する。
5. 正直な身元が抽選されること、資料が届くこと、実際に全体を検査すること、反証が期限内に包含・裁定されることは別の前提である。BFT乱数やf^mの式だけでそれらを保証しない。
6. 誤った全体肯定は事前の有限責任と客観的同scope反証で扱う。資料共有、bytes一致、外注の疑いだけで制裁しない。正しいclaimへの不労承認をnonceだけで検出できるとはしない。
7. FullReplay承認の責任窓、最遅合法case、反証によるhold、成功資格のmaturity、費用返還・保持を同じ新profileへ接続する。成熟した仕事と台帳settledを区別する。

[今回の改訂・検査](../maintenance/revisions/2026-11-17-work-binding/README.md)と[脅威表](../maintenance/revisions/2026-11-17-work-binding/THREAT_MATRIX.md)を参照する。改訂前49件の試験結果を現在の3 RFC統合の合格証拠に流用しない。

## 17. PALW-AN1の元実行とPanel＋Court

本節は[RFC-0059](0059-palw-an1-input-binding-and-panel-court.md)のAN1 familyに限る**DRAFT_NOT_ACTIVATED**の追補である。検証方式は全scopeを完全再実行するPanelと、有界な認証・局所再計算を行うNode/Courtである。暗号学的な計算validity proofの提出又は検証を条件にせず、Panelの完全検証責務を削除しない。

### 17.1 二種類のnonceと二つの固定時点

| 識別子・nonce | 導出と用途 | 変更してはならない対象 |
| --- | --- | --- |
| Minerの登録nonce | owner/model/job/profile等とともに実行beacon前に登録し、sと実入力token列へ結合 | 登録後の実入力選び直し、同slotの別元claim |
| s | Cと認証済みsigmaから導出するAN1試行ID | 別Miner/別nonceへのclaim又はopeningの付替え |
| PanelのPositiveNonce | 元claimと受理集合/poolを固定した後の選出イベント、actor/dutyから導出 | 元実入力、Minerの出力/trace Root、AN1 Ticket |
| ActionNonce | 受理済み元claim、native case/actor/phase/stepから導出 | 新beacon待ち、新D、別trialへの救済付替え |

Minerの登録を実行beacon取得前に閉じる。Minerの署名済み元claimとその三Rootを、Panel選出に使う後続イベントより前に閉じる。前者は入力選別、後者はPanelの正しい再実行結果を見た後のRoot補完を防ぐための別条件である。一般公開より前でも攻撃者がseedを復元できる構成を、未知性が成立したものと扱わない。

元claim・pool・受理対象・draw規則を凍結した後に、正規の選出イベントを認証する。実行beaconのdomainを変えるだけで新しい未知Panel seedを得たとしない。selected actorの範囲・native principal・役務上限は既存のglobal pool契約と有限profileに従い、本書でPanel人数を固定五人へ変更しない。

### 17.2 native由来の期待context

Nodeはpacketから任意Rootやmodelを選ばず、次を登録・元claim・native draw/義務から再構成する。

| 期待項目 | authoritativeな根拠 |
| --- | --- |
| chain、slot_id、owner/帰属、nonce、s | 凍結済み登録、認証済み実行beacon、AN1 codec |
| canonical入力xとR_x | InputDerive、固定byte-token表、登録J、token長と切詰め禁止規則 |
| R_y、R_tau、trace長/木形 | Panel選出前に固定したMinerの署名済み元claim |
| model root、program root、実行/算術profile | 元登録と共通kernel/checker版。Courtの自己申告を採用しない |
| actor、duty、positive抽出 | 後続の認証draw、固定pool、native assignment |
| case/phase/step、資金・期限・責任 | 受理済み元claimの有限状態機械と原book |

AN1用のCourt contextはこれらを型・幅・版付きで結合し、旧CPN1/SWC1のContextHashと別codecにする。新しい署名subjectには元claimの一意な参照とs/slot、三Root、model/program/profile、actor/duty、命題scope、期限・責任を含める。輸送chunkのRootやPanel自身の再計算Rootを元R_tauへ置き換えない。CPN1の32byte digestと既存native ID/beaconの幅を混同せず、切詰めない。

positiveだけが後続draw/assignmentと選出seedを要求する。negative、資料要求、応答、終端はまだ存在しないPanel drawをcanonical absenceとして扱えるaction版を使い、元claim受理後の救済を後続seed待ちで停止しない。事後にdraw欄を追加して同じactionのcontextやnonceを変えない。

owner又は登録nonceを変更すれば、hash衝突を除いてsと実token入力が変わる。全input/output/trace木はsへ束縛されるため、古いclaim又はopeningの直接付替えは期待contextかMerkle認証で拒否される。この性質は、古い値を利用して新しい正しいtraceを構成する計算や、本人以外からの資料供給を禁止するものではない。

### 17.3 局所反証に必要な認証

全traceの規範的関係は、初期境界、入力読出し、重みindex、演算・命令位置、状態更新、終端と出力の接続を含む。単に算術命令の値だけを検査するcheckerは不適合である。Nodeはprofileが規定する位置iから、必要な命令・参照座標とboundedな資料集合を導く。

| Courtの資料 | 必須認証と照合 |
| --- | --- |
| 命令・program位置 | 登録profileのprogram root。claim提出者が演算種別を選び直さない |
| 先行trace値 | 元R_tau、正確な位置・型・範囲・参照順。別trace又は未来位置の値を使わない |
| 入力token | 正規R_xとInputDeriveの位置規則。challenge部分のtoken欠落/UNK化を認めない |
| モデル重み | 元model root。認証された命令・token・座標からNodeが期待indexを決め、そのindexのopeningを検査 |
| Minerが記録した結果 | 元R_tauの当該位置。資料提供者の再計算結果をMinerの記録と混同しない |
| 終端出力 | 固定終端位置、元R_y、出力選択/生成長/停止条件の一致 |
| padding/構造 | profile指定のleaf数、木形、canonical padding、record種別・位置。任意paddingを別正規traceとしない |

重みは「同じモデルに含まれる値」で足りない。正しいmodel rootへのopeningでも、期待indexと違えばそのCourtは無効証拠である。モデルobjectのroot属性、cacheのverified flag、資料提供者の自己申告だけを信用せず、実bytesと正規encodingから認証する。Panelの全再実行でも実際に使う全重み・入力・trace資料からRootを再構成して元登録/claimと照合する。

一つの局所関係が未有界なfan-in、巨大reduce、非認証の重みlookupを要求するなら、共通profileの有界なmicrostepへ分解し、状態接続もtraceへ含める。局所反証の存在という数学的議論から、任意のLLMを最低Nodeで検査できる性能上界を推定しない。対象索引・opening・算術の有限費用と実装が未確立なら新claimを受理しない。

### 17.4 裁定結果を区別する

AN1 adapterは次の意味を持つ結果を区別する。既存wire enumの番号や旧意味を上書きせず、新profileの正式な対応表で接続する。

| 結果 | 条件 | Final・帰責への効果 |
| --- | --- | --- |
| InvalidEvidence | 署名、scope、opening、Root、期待index、codec等の認証が不成立 | 当該提出を拒否。これだけでMinerの計算不正・他actorのQ解消を確定しない |
| LocalHolds | 正しく認証された対象が規定の局所関係を満たす | その対象の局所結果。全trace正当性、Panel本人の全再実行又はFinalを意味しない |
| ComputationRefuted | 元claimへ正しく認証された値・構造・終端が規定関係に違反する | 元claimの成功を阻止し、既定の責任/反証手続きへ接続 |
| MaterialPending / InspectionPending | 必要資料又は適格な異議の終結条件が未充足 | 成功停止。ACKや時間経過だけでLocalHolds/成功へ変えない |

偽の値と古いpathを組み合わせた提出はInvalidEvidenceであり、正しくcommitされた誤値の反証とは異なる。一方、有効なopeningで認証された範囲外値・不正record種別・規範外padding等は、profileが客観的な違反を定めるならComputationRefutedの対象である。parserが読めない未認証packetと、Minerのcommitした形式違反を同一視しない。

有界Courtのbyte上限を超えるleaf等は、完全なopeningをその予算内で認証できないことがある。その資料を見つけたPanelは、無効なCourtを作って「反証済み」と報告せず、元claim・期待Root・期待indexに束縛した有限な資料要求と適格noticeを提出する。nativeは当該資料caseをMaterialPending/InspectionPendingとして保持し、規範的な資料・構造検証と全scope検査が完了するまで成功を止める。悪意ある生成器は正規Tree APIを使わずRootを作れるため、正規生成器のサイズ検査だけでこの経路を閉じたとしない。無効証拠の単なる到着は自動holdの根拠ではなく、既存の資格・資金・有限scopeを満たすnoticeの受理が必要である。

trace/output個数、opening index等のcanonicalな整数型・幅・範囲は、元claim受付、native context、Courtで一致させる。整数と等値比較されるfloat/booleanをnativeが受け付けてCourtの認証を不能にする実装は不適合である。byte上限内で正しく認証された不正recordと、認証できない過大packetとを、この型検査の不一致で取り違えない。

有効な反証はsample外でも適格な第三者でも受理できる。局所的に正しいCourtを何件コピーしても、別位置の有効な反証を打ち消さない。同じ反証のrelayは許すが、元の責任主体・beneficiary・offence/bounty onceを運搬者へ付け替えない。

### 17.5 完全再実行と一人の正直Panelの条件

担当Panelは、実資料の認証、正規入力の再導出、固定モデル/profileによる全scopeの完全再実行、元trace・出力・全境界/構造の比較を行う。差異があれば最初の差異又は他の有効な局所反証を構成し、元claimと結合した適格notice/Courtを提出する。少数sampleの一致や他PanelのLocalHoldsを、完全再実行の完了へ読み替えない。

有限な決定論的traceで全ての必要な局所関係と境界が成立すれば、先行値についての帰納法により正規実行と一致する。形式の整ったtraceが異なれば最初の相違位置があり、その先行値は正しいので局所関係違反を示せる。この議論は、全model種別へのmicrostep分解・認証・費用上界が既に実装されたという主張ではない。

誤実行の成功Finalを阻止する命題は、少なくとも一人の**能動的な正直Panel**が担当し、必要資料の取得又は不足によるsuccess holdを行い、全scopeを検査し、有効な反証をFinal前に包含・裁定できることを条件とする。他のPanelが虚偽承認やCourtコピーを行っても、届けられた有効な反証は多数票より優先する。

「少なくとも一人が選ばれた」だけでは、その人が実際に稼働すること、全資料が届くこと、反証が包含されることを保証しない。抽選の独立性・敵対割合・選別・停止・資料取得・検閲/包含・期限を条件として分ける。全員共謀、唯一正直者の停止、永久検閲を含む場合に無条件安全性を主張しない。

完全承認の最低件数と全選出者の完全検証責務を区別し、既存PublicOneHonestの最低1件を新しい「全員の署名待ち」へ変えない。一件の完全承認でも、予定された全体検査/異議機会、必要な共通coverage、未解決Qの解消、資料・原期限等を満たすまでFinalにしない。異議不在だけで、まだ終わっていない検査機会を省略しない。

### 17.6 Final gate・費用・一回性

AN1の成功Finalは、期待C/s/三Root/model/program/profile、認証beacon、元claim固定、Ticket、Panelの規定完全承認、必要資料と構造、全体検査/異議機会、Q解消、有効反証なし、現在のnative資格/依存/資金/期限/成熟、未消費slotを全て必要とする。packetの`verified=true`、任意のcache、ACK、多数署名、局所Courtの個数をこの結合条件の代わりにしない。

同canonical処理内の適格notice・反証・取消をFinal/credit発行より先に反映する。Final時には現在状態を再検査し、slot消費、SuccessFinal、報酬/credit/weight、once record、journalを原子的にcommitする。検証済み資料のcacheはexactな元claim、Root、profile/checker版と結合し、現在のQ、失効、依存、期限、slot消費を代替しない。正規reorgでは関連stateを一括undo/replayする。

新規登録・新originalのDはRFC-0056 §17で一回だけ扱う。既受理negative、資料要求、応答、終端には新D、AN1の新nonce、新seed、再実行Ticketを要求しない。全Panelの合法な最大役務、cold取得、全再実行、比較、局所化、反証搬送、最遅応答、保持・GCをRFC-0055の予約へ含める。positiveと共有資料の節約で正直な一人の救済予算を消さない。

正しいtraceのcache利用・共同計算・外注や正しい公開Courtの共有を、本人の物理労働を推定してslashしない。責任付きの全体肯定が客観的反証で否定された場合は、元subject・scope・事前の有限責任・原期限・offence onceによって処理する。資料不足やservice停止だけで算術的有罪にしない。

owner/nonceが実入力を変えること、Courtの直接付替えが失敗すること、条件付きの実行正当性は、通常LLM費用の下界とは別である。AN1にFULL1の値生成量下界やGroth16の安全性主張を移さない。認定量の未証明な換算をPanelの票数で補わない。

### 17.7 追加受入試験と移行

| ID | 反例・境界 | 必須結果 |
| --- | --- | --- |
| A57-01 | owner/nonceを変えて古いclaim又はopeningを転用 | s、期待入力、元claim又はRoot認証で拒否 |
| A57-02 | Panel nonceをMiner入力/trace/Ticketへ混ぜる | AN1/CPN役割違反として拒否 |
| A57-03 | 正しいmodel root内の別indexの重みを提出 | 期待index不一致でInvalidEvidence |
| A57-04 | model objectのroot属性と実重みbytesが不一致 | 実資料からのRoot再構成で拒否 |
| A57-05 | 古いpathに偽値を付ける／元traceに認証された誤値 | 前者InvalidEvidence、後者は局所違反が成立すればComputationRefuted |
| A57-06 | 出力と終端の不一致、規範外padding、誤った入力読出し | 元commitへ認証された有効反証が成功を阻止 |
| A57-07 | 複数の虚偽positive、正しい局所Courtのコピーと一件の有効反証 | 反証優先。多数票で上書きしない |
| A57-08 | 正直な一人は全検査完了、他は未提出 | 他の全成功条件が揃う場合は既存最少承認規則に従う。新全員待ちなし |
| A57-09 | 全scope未検査、資料不足、未解決Q、Panel seed停止 | timeout成功なし。適格negative/資料caseは新seedなしで継続 |
| A57-10 | 同slotの並行Final、再署名・再送・pruning・reorg | 原子的に一回。通常処分で復活せず、正規undoのみ全state復元 |
| A57-11 | 悪意ある生成器がbyte上限を超えるleafをcommitし、四承認と同batch Finalがある | 有効な有界反証又は適格な資料caseのholdを先に反映。未解決のまま成功・期限成功にしない |
| A57-12 | 元claim整数field又はopening indexにfloat/booleanを渡す | claimの不正型は受付拒否、証拠の不正型はInvalidEvidence。認証済みの有効整数型の誤traceは反証可能 |

新profileのwire/署名/domain/native adapter/期限/資源契約を別に登録し、旧CPN1/SWC1 claimは元規則で処理する。AN1を旧codecへ落として入力束縛・元claim固定・完全検証・反証優先を迂回させない。今回の記録は[2026-11-18 AN1改訂記録](../maintenance/revisions/2026-11-18-an1/README.md)に置き、旧49件やSWC1の過去結果を書き換えない。小型参照モデルのPASSを本番LLM、実ネットワークのDA/包含、最低Node性能、合意安全性の完了証拠にしない。
