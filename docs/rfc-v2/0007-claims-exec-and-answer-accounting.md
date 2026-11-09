# RFC-v2-0007 — Claim・EXEC・仕事量・回答会計

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。一件の完全承認は直ちにcreditを発生させない。単一claimでは20の成功条件を満たした仕事だけを一回計上する。任意のatomic batchではbatch全体がFinalを満たした後に全leafの非重複仕事を一回計上し、部分Final・部分creditは認めない。batch共有や複数承認で原仕事を増幅しない。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。成功gateの未成立・反証・取消を全仕事報酬とcredit consumerが読み、未消費権利へ整合して伝播する。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

新profileでは、20 §5.2の同batch反証・未解決caseのholdを先に反映したstateから、発行・予約・schedule有効化・permitの新消費を判定する。下記のconsumed保全は当該batchより前にcanonical受理済みの分だけであり、同batchで反証される起源のunitを先にconsumedへ移して取消を逃れることを許さない。Final後に受理された適格caseも、Qが解決するまで起源の現在の使用適格性をholdする。数量Nや既消費provenanceを増減させる新規則ではない。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

新profileではResultCommittedと成功Finalを分離する。[20](0020-verification-gated-final-and-adversarial-models.md)のCertificationReady成立・公開異議条件・依存条件を満たすまで、仕事報酬、合意credit、成功仕事量を計上しない。資料不足によるAbortedUnverifiedを成功Finalや計算不正へ読み替えない。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0008](../rfc/0008-palw-claim-backed-consensus-blocks.md)、[旧0038](../rfc/0038-palw-fp-answer-accounting.md)。

**関連RFC:** [01 推論・生成Job・入出力](0001-inference-and-generative-jobs.md) / [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [08 Claim依存・Prefix生成元・未解決責任](0008-claim-dependencies-and-liability.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [14 Court統合契約](0014-court-master-contract.md)。

## EXEC常設経路の追加採択 — 2026-11-08

**状態: ADOPTED_DESIGN_NOT_ACTIVATED。** [RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)・[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)により、認証仕事から期限付きExecCreditを発行し、事前固定した未来期間のExecScheduleと、そのslotのExecPermitでEXEC_TXを生成する経路を常設する。本書は仕事の一回性、credit量、予約・消費台帳を所有する。ScheduleId、時刻、slot割当、包含順序、attachmentは[10](0010-dag-consensus-and-native-settlement.md)が所有する。

採択目標は**1秒slotでglobalに原則一slot一正規EXEC_TXブロック、持続1 BPS**である。BASE／Heartbeatを加えた総ブロック数、取引件数、1秒での決済確定を意味しない。既存preset、wire、署名domain、fence、有効化高さ、credit換算値・TTLを本書だけで変更しない。

本追加採択は、上の検証先行Finalを弱めない。Nodeはモデル本体を持たず、正常positiveは非対話、negativeは対話局所化と客観裁定を用いる。資料不足を成功へ変えず、global選出者の誠実性前提と侵害時の保証限界は[20](0020-verification-gated-final-and-adversarial-models.md)に従う。以下は新claim／EXEC版に適用し、原付録と旧署名の意味は保持する。

## 1. 仕事・取引資格・支払いを分ける

一つの計算から複数のcarrier、slice、receiptが生じても、同じcanonical workに二重の発行・支払いを作らない。Claimの受理、完全な検証、成功Final、成熟、ExecCredit発行、取引資格の予約・消費、現金の支払いは別の事実である。

ExecCreditは**譲渡可能なcoinでも、未払い報酬残高でもない**。指定されたproducer／bond／operatorと起源workに結び付く、期限付き・一回限りのEXEC_TX資格である。取引で売買、送金、質入れ、別ownerへ付け替える機能を設けない。許された署名鍵管理をcreditの所有権移転として使わない。

EXEC_TXの取引feeは実際の取引受理から生じる。credit発行やschedule作成だけでfee、追加の有用仕事報酬、fork-choice weight、DAA貢献を発生させない。既存の拘束資本・共通期間・報酬上限は[11](0011-bond-budgets-fees-and-payments.md)に従う。

## 2. 論理型と責務

以下は新版の論理型であり、同名のRust型、codec、署名domainが実装済みという宣言ではない。

| 型 | 必須の意味・binding | 責務 |
| --- | --- | --- |
| Claim | 元job、canonical work identity、Minerのoriginal roots、入力・出力・rules、範囲、依存、署名、native状態 | 16〜20に従い検証し、成功・未検証・反証を区別する |
| ExecCredit | 一つのMintRecord、unit ordinal、owner／bond／operator、数量、policy版、有効開始・失効境界、起源の取消状態 | 本書の一回発行と数量台帳を読む。Claimやsliceの自己申告から作らない |
| ExecSchedule | 固定された候補credit集合、ScheduleProposalId、後続seed、割当root、ScheduleId、未来slot範囲 | 10のschedule contractに従う。予約済み権利を超えてpermitを作らない |
| ExecPermit | ScheduleId、現在slot、割当producer、使用するcredit unitと検査に必要な有界情報 | 10から導出するproducerの読み口。未認証claimの代替承認ではない |
| ExecBlock | permitとtransaction batchを持つEXEC_TX、または独立資格のEXEC_SLICE | 排他的subtypeを認証し、10のcanonical acceptanceで台帳・取引状態を適用する |

Claimからscheduleまでの生成・検査は事前処理で完了させる。producerのslot処理は、有効なExecPermitと通常の取引状態を読み、claim詳細、LLM実行、モデル取得、court資料取得を開始しない。現在の取消情報や必要な認証stateが不明なら、そのpermitを使用せず同期する。

## 3. ExecCredit発行の単一gate

新版の全発行経路は、同じnative predicateを通る。概念上の条件は次のすべてである。

```text
exec_credit_issuable =
    new_version_accepted && native_success_final
    && current_pre_final_certificate_valid
    && canonical_work_and_output_accounted
    && maturity_satisfied && dependencies_credit_ready
    && capital_bond_and_common_budget_conditions_satisfied
    && source_rights_live && !refuted && !revoked
    && issuance_window_open && !already_minted_for_canonical_work
```

`Final`という文字列、Panel receipt、局所positive一件、古いSecurityCertified、provider custody、役務feeの支払いでこのpredicateを代用しない。[20](0020-verification-gated-final-and-adversarial-models.md)の成功Finalと有効な新版認証を両方確認する。旧claimの既存権利を新版に二重mintする入口は設けない。

maturityは成功Finalの条件と別に満たす。必要な依存、資本・bondの拘束、共通上限、信用取消、責任holdを同じnative snapshotで判定する。未成熟の権利、保留中の候補、まだ存在しない将来creditを予約へ渡さない。

発行時の判定を永久の有効性証明とみなさない。予約・schedule有効化・permit受理にも、起源と権利の現在の適格性を反映する。これらはclaim全体を毎秒再読する処理ではなく、認証済みのcredit台帳・取消index・schedule状態をnative foldが更新して行う。古いcacheだけで取消後のpermitを使わせない。

## 4. root全体の発行量を一回だけ固定する

### 4.1 MintRecordと仕事の同一性

元rootのcanonical workと既存の重複規則から一つの発行主体を決める。署名封筒、carrier、再送、bond key、slice分割、schedule期間の違いで、同じ仕事を別の発行主体にしない。重複Finalの代表はcanonicalな受理順序から決め、producerが都合のよいFinalを選ばない。

MintRecordには、canonical work identity、代表Finalと受理根拠、認証参照、非重複範囲、量の換算policy、適用資本上限、成熟・失効境界、端数抽選の根拠、発行総数N、ownerとunit identityの導出条件を固定する。発行record、重複防止index、初期数量台帳を原子的に作る。

量の固定点は、版付き規則が定める最初の発行適格なcanonical状態から導出する。呼出し時刻や請求者、予定表を変えて、別の量子・seed・cap snapshotを選ばせない。端数抽選の結果N=0になった場合も、その決定と一回性recordを保存し、後の期間で再抽選しない。

root prefixと各sliceのcanonical範囲は互いに重複させず、合計を元jobの認証済み仕事量以下にする。root全体が成功Finalを満たした時点の一つの集計からのみ量を導出する。rootとsliceをそれぞれFinal creditとしてsnapshotに投入しない。未完了予算、carrier数、回答文字数、自己申告FLOPsから発行量を増やさない。

### 4.2 量の換算と端数抽選

既存のcanonical workとbudgetを読む版付き換算規則で量を決める。仕事一件をcredit一個に固定する規則ではない。大きな認証仕事は複数unitを得られるが、資本・共通期間・root・networkの各上限を越えない。

量子換算を用いるpolicyでは、上限を適用したcanonical仕事量をW、同じ単位の正の量子をQとすると、概念上の量は次となる。

```text
base = floor(W / Q)
remainder = W mod Q
N_raw = base + RoundOnce(original_work, fixed_rounding_anchor, remainder / Q)
N = min(N_raw, applicable_issuance_cap)
```

Wへの換算、Q、capの適用順序、端数抽選の整数表現・分布・上限は新版policyで固定し、既存休眠コードの定数を本番値と扱わない。丸め関数が未確定のprofileを有効化しない。overflow、量子0、過大Nを切り詰めた配列生成で救済しない。

端数抽選は、原仕事を固定した後の一意なcanonical seed anchorに束縛し、**同じrootについて一回だけ**行う。後のschedule seed、再予約、失敗slot、再送、別Finalを理由に再抽選しない。発行量を決める抽選と、未来slotを決める抽選は別domainとする。未確定seedから発行済み扱いにしない。

Nを固定した後、未選択の持越しや期限切れでNを増やさない。capで発行されなかった分を、後の空き予算で同じrootから追いmintしない。canonical reorgによる根拠のundo・再導出は§8に従い、利用者が選ぶ抽選のやり直しにしない。

## 5. 数量台帳と保存則

各発行lotのunitは、元のNとordinalから一意に識別する。実装はrange圧縮できるが、部分範囲の重複を検出できなければならない。異なるlotの値を単なる整数残高として相殺し、起源・期限・責任を失ってはならない。

| 区分 | 意味 | 通常の遷移 |
| --- | --- | --- |
| available | 未予約・未消費のunit。現在の適格性を別途満たす場合だけ予約可能 | reserved、expired、revoked |
| reserved | 一つの正規proposal／scheduleに排他的に拘束中 | consumed、条件付きavailable、expired、revoked |
| consumed | canonicalに受理されたEXEC_TXが使用したunit | 通常は終端。正規reorg undoだけ別扱い |
| expired | 元の失効境界で未使用のまま終了したunit | 終端 |
| revoked | 失効前に客観的取消により未使用権利を失ったunit | 終端 |

canonical stateの各lotについて、常に次を満たす。

```text
N = available + reserved + consumed + expired + revoked
all quantities >= 0
unit categories are pairwise disjoint
one unit has at most one live reservation
```

予約や返却はこのNの内部移動である。署名・送信回数、ScheduleIdの数、queueの数からNを増やさない。reservedの内訳はproposal／ScheduleIdに束縛した範囲の和と一致させる。

一時的なsource holdは新しい数量区分やmintにしない。適格性indexで使用を止め、正式な解除・取消・失効をnative規則で処理する。availableという区分だけで適格性を保証しない。

consumedの起源claimが後から取消されても、消費済みunitを再発行・返却しない。consumed数量を保ち、取消された起源・既消費額・責任対象を別の認証recordで追跡する。expiredの後の起源取消も二重にrevokedへ加算しない。同じ処理点で取消と失効が競合する場合の優先順位を版付きfoldで固定する。取引状態・決済・責任への効果は10・08・11に従い、単に古い取引を消して帳尻を合わせない。

## 6. 予約・選択・返却

### 6.1 未来期間の予約

10のScheduleProposalIdは、後続seedの確定前に候補集合とauthorityを固定する予約先である。適格credit集合のfreezeと、対象unitのavailableからreservedへの移動を同じnative状態遷移で行う。proposalの作成だけではpermitを作らない。ScheduleId確定後は同じ予約をそのscheduleへ結び付け、新しい予約数量を追加しない。

予約は所有者・bond・policy・起源状態、元expiry、対象範囲、未予約性、各予算を確認する。slot開始だけでなく、10が定義する当該permitの最遅包含まで権利の有効期間で覆う。予定表を長くするためにcreditの寿命を延ばさない。

競合proposal、重なるordinal、別namespaceへの転用、複数未来期間への二重予約を、nativeのcanonical適用順序で解決する。最初に成立した正規予約だけを生かし、後の競合を拒否する。範囲の一部だけ成功させる暗黙の部分受理をしない。再送は同じ予約結果の参照であり、別のreserveではない。

10のglobal EXEC_TX schedule namespaceと一意割当を使う。owner、モデル、個別scheduleごとに別の「一秒一ブロック」を増やさない。部分予約・候補数・future horizon・予約回数・台帳行数・総bytesは有限のprofile上限に従う。

### 6.2 未選択creditの持越し

候補として予約したunitが割当で未選択なら、10のcanonicalな割当完了記録に基づいてのみavailableへ戻せる。別枝で選ばれた、local cacheに見当たらない、producerが不利と判断した、という理由では返却しない。

持ち越すのは同じunit、owner、MintRecord、元expiry、累積試行履歴である。後続の正規未来期間へ再予約できるのは、未取消・期限内・未消費で、予約上限・持越し回数・残り包含時間・予算を満たす場合だけとする。未選択を理由に量の再抽選、期限更新、同じ期間の別seed選択を認めない。

### 6.3 選択済みだが未使用だった枠

選択済みunitは、slotの時刻を過ぎただけで返却しない。10が定義する包含期限・許容された遅延を終え、当該permitを後からcanonicalに消費できない終了事実と、未消費の事実が同じstateで成立した場合だけ予約を解く。

その時点で元creditが有効かつ累積再予約条件内ならavailableへ戻す。既に期限切れならexpired、取消済みならrevokedへ移す。proposal失敗やschedule非成立についても、10の終端記録なしにlocal timeoutでreleaseしない。古いpermitは閉じたままとし、次の予約は別の正規proposal／scheduleに従う。同じ権利を旧permitと新permitの両方で使用できてはならない。

## 7. canonical消費とproducerの二重署名防止

EXEC_TXの生成、署名、送信、受信、候補保存だけではcreditをconsumedにしない。10のcanonicalなBASE側の受理で、permitの一回性確認、reservedからconsumedへの移動、取引状態・feeの適用を原子的に行う。無効packetや負けた枝に対するlocal観測から二重消費を作らない。

同一permitを複数のblock／attachmentが提示しても、正規の順序とone-use規則で一つだけを受理する。重複、別slot、別ScheduleId、期限後の提示を別creditに振り替えて救済しない。予定表の全体再抽選や1秒slotの追加で不足を埋めない。

producerは署名前・送信前に、10の署名衝突単位に対応するPermitIdと署名対象digestをdurableなanti-equivocation journalへ記録する。再起動後に同じPermitIdで異なるblockを署名しない。同じ署名済みblockの再送は新しい消費ではない。

このlocal journalは合意の消費台帳と別である。journalはconsumedの証拠にせず、chainのreorg undoで削除して二重署名を許さない。reorgで権利がavailable／reservedへ戻っても、同一PermitIdに既に異なる署名があれば再署名を止める。新しい正規permitへの適格性は10から導出する。

## 8. restart・reorg・取消の復元

MintRecord、work重複index、unit区分、予約、schedule対応、消費tombstone、失効・取消、依存・予算を同じcanonical世代でcommit／undoする。quantityだけ戻し、permit-useやreserveを残す復元、またはその逆を禁止する。[12](0012-authenticated-state-storage-and-ibd.md)の認証された保存・IBDへ同じ事実を運ぶ。

正規reorgで消費blockが外れた場合は、その直前のcanonical予約状態へ戻し、新枝の期限・取消・schedule状態を再適用する。常にavailableへ戻す単純返金にはしない。MintRecordの根拠自体が外れた場合、そのlotと派生予約・permitを新枝で使用させない。新枝の正規導出は一つだけ行い、古い枝のNと合算しない。

通常のtip伸長、restart、provider交代、snapshotの再読ではMintRecordや端数抽選を再生成しない。schedule変更、seed anchorのcanonical性、取消permitと他のpermitの扱いは10に従う。適格性が失われたcreditだけを無効にし、残る割当を都合よく再抽選しない。

消費・失効・取消tombstoneの削除は元workの再発行を可能にしてはならない。保持・集約・pruningは責任期間とreorg／IBDの認証条件を満たす場合だけ行う。無制限のlive行数を正当化せず、発行lot数、N、予約、索引、復元workの上限を受理条件にする。

## 9. EXEC_TXとEXEC_SLICE・回答tokenの会計

EXEC_TXは成熟済みExecCreditを使う高速取引経路である。EXEC_SLICEは進行中rootの認証可能なwork範囲を運ぶ。初期の排他的subtypeを維持し、sliceだけでExecCredit、TxPermit、取引fee、fork-choice weightを発生させない。

sliceの受理・positive検査・計算完了と、root全体の成功Finalを混同しない。失敗したsliceを別rootへ持ち込んで既存仕事の一回性を回避しない。rootがFinalになった後も、§4の一つの集計だけを発行元にする。EXEC_TXは既に得た資格を消費するため、別の進行中sliceの速度や停止を毎slotの生成条件にしない。

free-promptで生成した回答を支払済み位置として記録し、その回答を次のpromptに連結しただけで同じ位置を再びprefill報酬や仕事量にしない。回答行、根拠claim、実際に受理されたcarrier、価格計算を結び付ける。成立させたcarrierの事実を使い、自己申告や現在tipの推測で完成扱いにしない。

取引資格とreceipt／仕事報酬の量子は別物である。同じ認証仕事から既存規則が認める異なる役割の権利が生じても、互いの台帳を代わりに消費したり、新しい発行原資として循環させたりしない。

## 10. 1 BPSの持続条件と有限在庫

1ブロックにつき1unitを消費するprofileでは、1 BPSを持続するために**平均1unit／秒の実消費を支える適格credit供給**が必要になる。仕事一件／秒を要求する意味ではなく、§4の換算後のunitで数える。失効・取消による未使用損失があれば、その分も供給量へ含める。

観測区間の発行量をM、消費量をC、未使用の失効・取消量をD、未消費在庫をS=available+reservedとすると、正規reorgを除いた同一canonical履歴で次を満たす。

```text
S_end = S_start + M - C - D
long_run_issuance_rate >= target_consumption_rate + unused_loss_rate
```

持続1 BPSの評価ではtarget_consumption_rateを1unit／秒とする。長期供給率、認証・成熟の遅延、credit寿命、scheduleの先行期間、欠落slot、損失率を別に測る。最初の有限在庫だけで短時間1 BPSになった結果を、継続供給の成立と扱わない。

供給停止時に使える在庫がS unitなら、他の損失がなくても1 BPSを支えられる時間は最大S秒である。実際には未来割当・期限・取消によりさらに短くなり得る。creditが尽きたslotは10に従い欠落として扱い、未成熟claim、未認証Final、EXEC_SLICE、無制限mintで埋めない。BASE／Heartbeatの進行はこの在庫に依存させない。

## 11. 定量受入とfailure cases

以下は新版の必須受入条件であり、今回測定済み・PASSという報告ではない。使用policy、binary、機器、試験時間、負荷、観測対象のcanonical枝を記録する。

| ID | 入力・条件 | 必須の検査・定量結果 |
| --- | --- | --- |
| E07-01 | N unitを発行し、L個のglobal slotへ予約・使用 | 消費はmin(N,L)以下。各slotの正規EXEC_TXは最大1。各lotの保存則誤差0 |
| E07-02 | 同じrootをF個のFinal／carrier／slice分割で提示 | MintRecordは1つ、端数抽選1回、発行量は同じN。Fに比例して増えない |
| E07-03 | 未成熟、AbortedUnverified、無効認証、取消、依存未成立、bond上限超過 | 発行量0。既存の無関係creditを失わせない |
| E07-04 | 同じordinalへ同時proposal、部分重複、再送 | live予約数はunitごとに最大1。後続競合を拒否し、数量の部分移動なし |
| E07-05 | 未選択carry、未使用slot、後から届く旧permit | native終了前の返却0。元expiry・N・試行予算を維持。旧新permitの二重消費0 |
| E07-06 | mint／reserve／consume／expiry／revokeの境界でcrash | 途中世代を復元しない。全区分合計N、重複unit数0 |
| E07-07 | 消費block、schedule anchor、mint根拠を順にreorg | 新枝のnative状態再生と台帳一致。旧新枝のN合算・二重fee適用0 |
| E07-08 | consumed後に起源取消、expiryと取消の競合 | consumedを再使用不可。Dへ二重加算せず、責任recordを保持 |
| E07-09 | 署名後crash、reorg後に同一PermitIdで別blockを要求 | durable journalが拒否。合意undoを二重署名の許可にしない |
| E07-10 | cold producerでscheduleを事前用意しL slot運転 | claim詳細／モデル／court資料のhot path fetchは0。permit取得・取引処理の遅延分布を測る |
| E07-11 | 認証・成熟済み供給を継続する定常負荷 | M/秒、C/秒、D/秒、在庫、slot充足率、予約滞留を測り、1 BPSと有限在庫を両立 |
| E07-12 | 初期在庫だけで開始し新規認証を停止 | 在庫尽き後の無資格生成0。欠落を計測し、BASE／Heartbeatの進行を確認 |
| E07-13 | Q=0、overflow、最大lot・ordinal・予約・復元量 | allocation前に不正値を拒否。受理上限内のCPU・RAM・bytesを計測 |
| E07-14 | EXEC_SLICE flood、same-work回答再利用、copy carrier | slice由来の即時mint0。root集計と支払済み位置の重複計上0 |

定量profileには、最大lot／unit／予約数、量子・cap、成熟、TTL、最大持越し回数、未来期間、保存・復元量、長期供給能力、producerの遅延目標を記載する。未確定値はUNKNOWNとし、値がないままlive既定値を補わない。EXEC_TXの1 BPSとcourt／資料処理の容量試験は別の計測とし、[14](0014-court-master-contract.md)・[19](0019-model-free-node-and-bounded-evidence.md)の有界性を同時に満たす。

## 12. 接続・移行・他RFCへの出口

workspaceの既存source snapshotにあるexecution_quanta／execution_laneは、workの重複排除、量子、未来割当、成熟、消費の参考となる。最新main、採択済みTTL／換算値、新しい五区分台帳の実装、1 BPSの達成を示すものではない。旧sourceの測定記録を新版の合格実績へ移さない。

新policy・codec・署名domain・migration・fence・network presetを別に確定し、旧creditと新版creditを二重計上しない。旧権利の移行が必要なら、起源・残量・期限・責任を保つ一意な移行と旧側の使用停止を規定し、既存権利を文書編集だけで延長・削除しない。

仕事入力・推移的責任は[08](0008-claim-dependencies-and-liability.md)、ScheduleProposalId／ScheduleId・slot・permit・attachment・canonical順序は[10](0010-dag-consensus-and-native-settlement.md)、物理支払・返金・総予算は[11](0011-bond-budgets-fees-and-payments.md)、保存・IBDは[12](0012-authenticated-state-storage-and-ibd.md)、courtは[14](0014-court-master-contract.md)と16〜20に従う。本書から別の時刻や順序を定義しない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0008 | 1–153 | [RFC-0008: PALW EXEC lane — claim-backed work slices and transaction execution](#source-v1-0008-L00001) |
| 旧0008 | 193–361 | [7. 実装境界とactivation gates](#source-v1-0008-L00193) |
| 旧0038 | 1–49 | [RFC-0038 — free-prompt の回答トークンを支払済み計算として記録する(FPA1 carriage)](#source-v1-0038-L00001) |

---

<a id="source-v1-0008-L00001"></a>

### 旧RFC-0008 / 原文1–153行

出典: [0008-palw-claim-backed-consensus-blocks.md](../rfc/0008-palw-claim-backed-consensus-blocks.md)。配置理由: 主題別統合。

### RFC-0008: PALW EXEC lane — claim-backed work slices and transaction execution

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Revised Draft, 2026-10-10後続改定 — immutable identity, model acquisition non-interference, model-bond allocation and Rule E; MISAKA Torrent/Seeder rewards abolished; new implementation/economics/activation pending; historical evidence unchanged

Original proposal: 2026-10-03. Stable filename retained for existing links.

Spec closure 2026-10-15: R8-* requirement groups are closed or parameterized in text by [仕様閉鎖 — 2026-10-15](../rfc/0008-palw-claim-backed-consensus-blocks.md#spec-closure-2026-10-15). This is a document revision only; no implementation PASS, activation or fence height is implied.

> **設計の優先規則:** heartbeat・BASE-0を弱めず、現行mainのliveness構造を正とする。claim-backed sliceは既存ROUND/EXEC laneへ統合する。[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)の公開prosecution要件も維持する。文書の改定は実装・activationを意味しない。

#### 0. 要旨と旧提案の置換

**既存EXEC laneを、Final済みclaimから得たtransaction permitだけでなく、進行中claimのauthenticated work sliceも運べるPALW execution laneへ拡張する。** block classは増やさず、REAL / EXEC / HEARTBEAT / BASE-0の4役とする。同じlaneを使っても、round creditとslice work creditは別の意味・別の台帳を持つ。

旧RFC8の「sliceを新しいalgo-11のselected-parent / consensus BLUE / DAA候補にする」「sliceでchain clockを進める」「floorのmerge admissionを変更してモデルBLUE shareを上げる」という方針は撤回する。本改定はheartbeatの条件・重み・頻度、BASE-0の資格・fallback・anchor duty、120秒chain cadence、現行mainのclock cursor・REAL tick・GHOSTDAGを変更しない。slice到着数や実行中sessionの存在を、heartbeatやBASE-0を止める根拠にしない。

旧algo-11設計本文は削除し、試験結果・未実施項目だけを[v0試験記録](../evidence/palw/implementation-records/rfc-0008-v0-test-record.md)に残す。[ADR-0169](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md)は新設計への参照に置き換える。新しい実装対象は[EXEC統合仕様v1](../design/palw/execution/rfc-0008-implementation-spec.md)である。旧試験結果をEXEC sliceの合格実績として引き継がない。

#### 1. mainを基準とする4役

本改定のコード確認基点は`MISAKA-BTC/misakas` main `282355ba9`。現行roundのalgo idは10で、[ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)と`palw_execution_lane_v1`のFinal credit / schedule / permitによるfee-only laneである。新しいEXECという呼称だけで、[ADR-0168](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md)の未導入のanchor trailerや着色方式を実装済みと扱わない。

| 役割 | payloadと資格 | chainでの責務 |
| --- | --- | --- |
| REAL | 新しいordinary PALW useful work。短い推論のcomplete claim、または長いclaimを開くroot useful work | 現行の資格・weight・clock規則でchain consensusを担う |
| EXEC | `EXEC_TX`: Final済みcredit由来のTxPermit。`EXEC_SLICE`: 進行中root claimの非重複work | chainにanchorされる高速execution lane。selected parentにならず、weight 0、DAA 0 |
| HEARTBEAT | 現行mainのheartbeat | 現行のclock/livenessを継続する |
| BASE-0 | 現行mainのemergency fallbackと必要なanchor duty | 長期的なREAL不在など、現行条件で復旧を担う |

EXECの全subtypeについて、raw blue work、PALW `safe_weight` / `immature_weight`、consensus blue score、DAA tick / retarget contribution、pruning hierarchy contributionを**追加しない**。EXECのFinal work accountingからfork-choice weightへ戻す経路も作らない。REAL root自身の現行規則で認められたworkだけがREALのweightになる。

EXECのcanonical acceptanceとconsensus BLUEは別の事実である。legacy roundがREDとして格納されることと、将来のRPCでEXEC classを表示することを混同しない。EXECの大量発行が通常blockのk anticone budgetを消費したりREALをREDにしたりしないことを、共通lane実装の受入条件にする。表示を変えるだけではこの条件を満たさない。

#### 2. 同じEXEC class、独立した2つのpayload

将来の共通envelopeは概念的に次の形とする。型名は設計名であり、現在のRust APIではない。

```rust
PalwExecV2 {
    version,
    network,
    anchor,
    subtype,
    tx_permit: Option<RoundPermitV1>,
    work_slice: Option<ClaimSliceV1>,
    payload_root,
    executor_bond,
    signature,
}
```

**初期releaseは`EXEC_TX` / `EXEC_SLICE`の排他的subtypeを採用する。** `EXEC_TX`にはTxPermitとtransaction batchを、`EXEC_SLICE`にはWorkSliceとwork material commitmentを載せる。どちらも同じEXEC class・chain非進行laneであり、第5のclaim-backed block classを作らない。

初期releaseでは両fieldが存在するenvelope、両fieldが欠けたenvelope、subtypeとfieldが一致しないenvelopeを拒否する。`EXEC_SLICE`はユーザーtransaction batchを運ばず、WorkSliceの資格だけでtransaction fee収入や取引枠を得ない。fee/bondの資金確保は、chainで受理済みのroot claimまたは既存の資金経路から行う。

将来、1 EXEC blockに両payloadを載せる拡張は可能だが、別のversioned仕様と受入試験を必要とする。両条件を独立に検証した上でblock全体をatomicに受理/拒否する。consensusの「block部分受理」は初期版に入れない。

##### 2.1 EXEC_TX: 過去のFinal creditによる高速取引

```text
過去のclaimがFinal → 既存credit / schedule → round permit
→ EXEC_TXでtransaction batch → chainで受理 → fee収入
```

既存permitのspan、round、index、operator/domain、bond、payout、one-use、quota、parity、gas budgetを維持する。WorkSliceがあることをTxPermitの代わりにしない。TxPermit消費は新しいLLM計算・claim reward・work weightを生成しない。

##### 2.2 EXEC_SLICE: 現在のclaimに属する新しいwork

```rust
ClaimSliceV1 {
    root_claim_id,
    slice_index,
    class_id,
    canonical_job_id,
    kernel_version,
    plan_root,
    canonical_range,
    predecessor_state_root,
    result_state_root,
    evidence_root,
    da_root,
    executor_bond,
    signature,
}
```

active root claim、認められたexecutor bond、canonical plan/range、直前state、証拠・DA、署名を検査する。rangeはPALW-TIR/kernelの決定的な境界で導出し、credited workはcanonical IR costから計算する。自己申告CCU/PWU、任意の極小slice、slice数、header数からcreditを算出しない。

署名はnetwork/version/subtype、anchor、block payload commitment、root、index、range、前後state、evidence/DA、bondを拘束する。root/plan/class/kernelへのbindingが不完全なsliceを受理しない。rootはchainで受理済みのcanonical claimであり、未受理のroot宣言だけではsliceを発行できない。

#### 3. REAL rootと長い推論

```text
短い推論: REAL → complete claim → 現行の検証/challenge → Final

長い推論: REAL (root claim / session open)
           ├─ EXEC_SLICE 0
           ├─ EXEC_SLICE 1
           ├─ EXEC_SLICE 2 ... N
           └─ claim complete → 検証/challenge/DA条件成立 → root claim Final
```

REALはchain-level claim anchor、EXECはそのclaimの内部計算を運ぶlaneとする。rootはclass/job/input、bounded total work、slice plan、初期state、DA、executor authorization、bond/exposure、expiryをcommitする。

**sessionを開く宣言自体を、実行済みuseful workとして扱わない。** root REALは現行REALのticket、実計算、admissionを満たす必要がある。rootに実計算prefixを含める場合、そのrangeを共通work台帳に登録し、後続sliceで再度creditしない。未実行の全session予算からrootのweightや即時rewardを得ない。root-openを現行REALにどう適合させるかは実装gateであり、任意のtx-carried session objectをREALに昇格させる抜け道を設けない。

初期版ではsliceの検証結果をroot claimへ集約し、**root claimがFinalになって初めて、そのclaimのslice work rewardを確定する**。各sliceが独立claimとしてFinal、permit、reward、weightを獲得する旧v0の方式は採用しない。completeは全canonical rangeと最終stateが揃った事実であり、Finalと同義ではない。

#### 4. Accounting: permitとworkを分離する

| 台帳 | キー・意味 | 何を発生させるか |
| --- | --- | --- |
| TxPermitUse | 既存`(span, round, permit_index)`。Final creditに基づく取引資格のone-use | 有効transactionの受理とfee。新規work reward/weightは0 |
| WorkSliceUse | `(root_claim_id, slice_index)`とcanonical range。pending / verified / final / voided work | root Final時の一度だけのwork reward。取引permit消費・weight・clockは0 |
| RootWorkBudget | job/rootのcanonical total、root prefix、pending/final ranges、escrow、settlement marker | rootとsliceの重複禁止、claim単位の一回の精算 |

全historyで次のconservationを満たす。

```text
root_credited_work + Σ credited_slice_work <= canonical_claim_work
credited ranges are pairwise disjoint
Σ settled_work_reward <= the claim's funded reward allocation
EXEC fork-choice weight = 0
EXEC DAA contribution = 0
```

rootとsliceに同じworkのrewardを二重払いしない。（2026-10-15補足: 全positive legのroot native Final前無支払、bound kernel feeのhold、受理前Q/R予約、fee-only Roundの`ExecutionCap`、v1 market feeの`R_max`外の扱いは末尾の仕様閉鎖節R8-BUDGETを規範とする。）既存のclaim allocation・per-DAA発行上限・escrow資金制約を維持し、slice blockごとのcoinbase subsidyを追加しない。slice数を増やしてもclaim allocationは増えない。Tx feeは既存の取引会計に属し、work rewardとは別である。

sliceの受理・positive verification・complete・Final・報酬解放を分ける。Final済みcanonical workが将来の既存scheduleへ入る場合も、**root全体の一つのaggregate Final creditから一回だけ**導出し、既存のeligibility / quota / capを維持する。rootと各sliceを別々のFinalとしてsnapshotに投入しない。EXEC_TXが消費したpermitを再生成しない。

root expiry、missing DA、verification backlogではpending workをtimeout/voidの規則で処理し、検証の沈黙をFinalにしない。rootやsliceのfraudは該当するstate依存の後続sliceを無効化する。証拠・義務に基づくconvictionのみでslashし、verifier不在やlocal timeoutをproducer arithmetic fraudにしない。

#### 5. Anchor、reorg、livenessの分離

EXECは既存laneと同じchainへのattachmentを持つ。sliceは**root claimのanchor**と**carrierのchain anchor**を別々に拘束し、canonical branch上のactive rootのみを対象とする。reorgでroot/anchorが外れたsliceは新しいbranchのworkとして自動採用しない。再attachmentはversioned再検査を必要とし、同じworkを二度settleしない。

共通EXEC acceptanceは、chainのparent stateから決定的にcovered set・順序・資格を検証してfoldする。v2のanchor closureはconsensus-parent walkから分離し、stale headやslice burstがchain template・merge depthを塞がないようにする。既存v1のmergeset経路と、将来のanchor closureを混ぜず、具体的なwire commitmentと互換性は実装仕様で固定する。

不正なEXEC blockは拒否し、laneで資格を失ったcarrierはそのlaneの受理対象から外す。producerが勝手に送った不正sliceだけでchain blockやrootを無効にしない。chain blockが宣言したanchor commitment自体が不正なら、既存のblock commitment検査に従う。slice fraudはworkの検証・精算を止めるが、独立したEXEC_TXのpermitや確定済み取引をsliceの結果に依存させない。

heartbeatとBASE-0はsliceのpending数、検証結果、header種別、node-local producer観測を読まない。court、DA、verifier、長い推論の全てが停止しても、現行mainと同じ規則でchainとdeadline machineryが進む。bounded queues、reserved chain-validation資源、EXEC_SLICE独立quotaを設け、slice floodでheartbeat、BASE-0、REAL、EXEC_TXをstarveさせない。

例: 20分のsessionから2分ごとに10 slicesを運べる。それは**DAA +10、10倍fork-choice power、10倍rewardを意味しない**。clockは現行REAL/heartbeat等のactive規則で進み、EXECの進行は0のままである。

---

<a id="source-v1-0008-L00193"></a>

### 旧RFC-0008 / 原文193–361行

出典: [0008-palw-claim-backed-consensus-blocks.md](../rfc/0008-palw-claim-backed-consensus-blocks.md)。配置理由: 主題別統合。

#### 7. 実装境界とactivation gates

| 境界 | 新しい実装対象 |
| --- | --- |
| `palw_execution_lane_v1`とversioned EXEC envelope | 同じlaneのTX/SLICE subtype、domain-separated payloadと独立資格 |
| root claim / PALW state / fold | canonical plan、one-use range、root aggregate Final、escrowと一回のsettlement |
| header / GHOSTDAG / difficulty / sync | 全EXEC subtypeがselected-parent/weight/DAAに入らないこと。consensus anticoneからの隔離 |
| chain template / execution acceptance | bounded anchor closure、parent-state verdict、stale-head recovery、atomic carrier acceptance |
| producer / relay / RPC / explorer | root REALとslice scheduler、独立quota、class/subtype/lifecycleの明示 |
| verification / DA / court | public material、boundary checks、positive verificationとobjective conviction |

この表は設計対象であり、現行コードをこの文書だけで変更する指示ではない。新しいversioned EXEC fenceとroot lifecycle/accountingが必要だが、slice clock fence、BASE-0を排除するmerge-admission fence、新しいalgo-11 chain classは導入しない。既存wire v1・schedule・fingerprint・active network presetsはこの改定で変更しない。

activation前に、少なくとも以下を独立に実証する。

1. **共通lane隔離:** TX/SLICEとそのburstがselected parent、blue score/work、PALW weight、DAA/retarget、k anticone、pruning hierarchyを増やさない。
2. **liveness同値:** producer/verifier/DA/courtの停止、RED spam、slice flood、遅い推論でもheartbeat・BASE-0の資格、clock、anchor duty、復旧が基点mainと一致する。
3. **work/reward保存:** prefixとsliceの重複、skip、別root/job/branch replay、duplicate Final、再起動、reorgで二重credit・二重reward・二重permitが起きない。
4. **資格独立:** invalid WorkSliceが有効TxPermitを変えず、invalid TxPermitがwork creditにならない。初期版のmixed payloadをatomicに拒否する。
5. **public prosecution:** slice/boundaryの各不正、DA default、false Validを有界materialから区別し、公開bondで裁定できる。未解決disputeのままFinalにしない。
6. **資源・移行:** pending depth/work/bytes、open roots、closure walk、retention、queuesをboundedにし、activation境界、旧node、IBD、pruning、同時到着順で同じ結果を示す。

旧v0のalgo-11、slice-clock、floor排除の試験はこの受入を証明しない。2026-10-15の仕様閉鎖節が各gateに対応する規範規則と名前付きパラメータ（USER DECISION REQUIRED）を追加した。未解決gateは実装未完了として列挙し、placeholderでactivationを許可しない。供給/検証能力は実測で判断する。

#### 8. 観測と段階導入

まずmainのliveness・EXEC_TX基準値を固定し、root/slice accountingとlane隔離をshadow replayする。次にisolated drillでroot REAL → EXEC_SLICE → positive verification → root Final → 一回の精算を実行する。migration、fraud、DA停止、reorg、restart、pruned join、floodを確認した後に、別途readinessに基づくactivationを審査する。このRFCのmergeでlive規則は変わらない。

観測はREAL / EXEC_TX / EXEC_SLICE / HEARTBEAT / BASE-0別のcarrier数、accepted/pending/verified/final/voided work、rootのcanonical/credited work、Tx fee、work reward、clock進行、空slot、queue、DA/court latencyを分ける。EXEC carrier数をselected-chain shareやconsensus BLUE shareへ足さない。

旧proposalのP2観測（8k attempt 72本中BLUE 1、0.19本/120秒slot等）は過去の運用報告であり、本改定で再測定した値ではない。slice化は計算を細かく運べるが、同じ一台の計算能力を増やさない。本RFCの合格条件を「model BLUE share 90%」から、**非重複workを安全に運び、現行livenessと取引laneを維持し、一つのclaim予算内でFinal/報酬を完結すること**へ変更する。

#### 9. 関連設計との優先関係

- ADR-0125のFinal credit / round permit / fee-only transaction経路を維持して一般化する。
- ADR-0168のchain非進行、parent hygiene、anchorの方向は参照する。ただし§10.3のroundへの`safe_weight`配分は本RFCのEXECには適用しない。TX/SLICEともweight 0で、rootのweightをEXECへ再配分しない。
- 旧algo-11設計、per-slice Final/weight/reward、slice clock、floor排除の設計本文は削除する。ADR-0169は置換の案内とし、過去の試験結果だけを独立記録に残す。
- ADR-0140/0142/0165のmain上のactive heartbeat・clock・BASE-0構造を維持する。旧RFC8のfuture slice-clock/class-aware floor変更を、この改定の前提にしない。
- ADR-0173 / RFC14の公開prosecutionと、RFC15のdeferred activationは維持する。

本RFCと[EXEC統合仕様v1](../design/palw/execution/rfc-0008-implementation-spec.md)が2026-10-08以降のRFC8実装対象である。既存の実測・旧分岐実装の記録を、現行mainの実装状態や新設計の合格実績として扱わない。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

root/receipt/権利償還と対象slice/rider/batchを同じproducer bond予算へ束縛する。一claimから派生する全block・報酬・Final weightを受理時に予約し、行使・Finalでも検査する。
容量拡大で総権利を増やさず、Final/retired/reversalを含む全writer/readerとundoを統一する。EXEC_TX/EXEC_SLICEのweight/DAA=0と既存発行・work一意性を維持し、未解決claimから不可逆settlementを許可しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

#### 改定 — 2026-10-12（モデル配分とブロック生成機会の接続：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3A.5。

ADR-0177 2026-10-12改定のモデル配分（Miner＋Panelの適格claim bondと1.3〜2倍の係数、提案）をブロック生成機会へ接続する場合、coinbase配分だけの変更として済ませず、ブロック機会の帰属・上限・移行を本RFCで明示する。物理blockの総頻度、DAA/難易度、fork choice、Final weightを同時に変更するとは決定しておらず、特にFinal weightへ係数を自動乗算しない。必要な変更は別のversioned review・試験・activationを要する。

<a id="spec-closure-2026-10-15"></a>

#### 仕様閉鎖 — 2026-10-15（設計監査の要件索引 R8-* に対する規範文）

**状態と読み方:** 本節は[設計監査の要件索引](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引)のR8-*群について、本RFC本文に欠けていた規範規則・未定義パラメータ・後続設計記録との矛盾を閉じる文書改定である。上位は[PRINCIPLES](../PRINCIPLES.md)とADR-0173/0176/0177/0178であり、衝突時はそれらと後続のユーザー判断を優先する。本節は実装完了・検証PASS・activationを意味しない。公開fenceは全て不在のまま、release高さを変更しない。dormant codeに入っている数値はmechanics/fixture値であり、政策承認ではない。mechanicsの成功やprivate acceptanceを独立review・経済証拠として扱わない。詳細は[EXEC統合仕様v1](../design/palw/execution/rfc-0008-implementation-spec.md) §§12–15と各設計記録に置き、本節はその規範的要約である。

「USER DECISION REQUIRED」は、値の決定にユーザー判断を要する名前付きパラメータを示す。推奨値は既存記録から根拠を示せる場合だけ記す。

##### R8-LANE — 排他envelope、carrier、header carriage

1. §2の排他subtype規則を維持する。加えて、同じwork/claim/rootを運ぶcarrierの数、同じsliceの再送、header carriage内の追加認証byteを、consensus position、permit、fork-choice weight、DAA、retarget、乱数source、独立検査の数として数えてはならない（MUST NOT）。
2. **PAV3 header carriage（`palw_monetary_material_v1`配下、休眠）:** 新しいnative attemptはPAV2の署名済みattemptを不変のまま内包し、事前に署名済みのmonetary-material sessionへ結び付ける。規範条件は次のとおり。
   - 選択するsessionは、未結合で、class/artifact/executor bond/operator/consensus導出job policy/受理区間がattemptと一致し、未chargedの**異なるprovider operatorを2以上**持つものの中で、session root鍵が最小のもの（canonical oldest-key）である（MUST）。producerは別のfunded sessionを選んで追加のdrawを得てはならない。producerとconsensusは同じ選択関数を使う。
   - Readyはsession/consent/claim/execution key/manifestと未chargedのprovider roster（鍵順）を拘束し、各providerはReady idへ署名する。providerは2以上8以下（8はcarriage上限65,536 byte内に全署名・sealを収めるための構造上限）。
   - executorはReady id、attempt id、全provider署名をroster順に含むsealへ署名する。sealはheader段階でattempt自身の鍵によりstatelessに検証する（MUST）。これはprovider署名byteを書き換えて一つのsolveから第二のheaderを作るmalleabilityを閉じるためである。provider鍵・executor鍵が登録bondのものかはstatefulなadmissionで検査する。
   - provider署名とsealはLayer-0 ticket tagの外にあり、価格付け・lottery entropyにならない（MUST NOT）。block identityには含まれる。re-sealでticketを得られない。
   - fence未満では、PAV3を新しい規則で判定してbytesを拒否してはならない。旧releaseとの混在で分裂しないよう、新しいheader形式は自身のfenceまでA-2共通規則（decode不能と同じ扱い）に従う。
3. `PalwMaterialSessionMinProviders`（名前付きパラメータ）: 一sessionに要求する異なるprovider operatorの最小数。不変条件 `2 <= min <= PALW_MATERIAL_MAX_PROVIDERS_V3 (8)`。休眠codeは構造上の下限2を実装している。**value: USER DECISION REQUIRED**（推奨: 2を下限として維持。理由: 1 providerではmaterial保持が単一点故障になり、2は実装済みの構造下限である。これ以上の値は実測したprovider不履行率から決める）。

##### R8-WORK — 同単位のwork、prefix/suffix scope

[実装仕様 §12](../design/palw/execution/rfc-0008-implementation-spec.md#12-work-design-correction--execution-cost-is-not-lottery-weight-2026-10-10-audit)を規範とする。

1. REALの抽選込みweightとexecution costは別の量である。credited workは、登録IR、一つのwhole-job facts、絶対positionから求める10次元execution cost vectorとし、prefixと各suffixを同じ単位・同じplanから導出する（MUST）。自己申告CCU/PWU、slice数、carrier数からcreditを導出しない。
2. class登録DAAがcost policyのsnapshotを固定する。登録後のcost table変更は既存classへ遡及しない。
3. 宣言された`total_work`/`boundaries`は導出planと一致しなければならず、生成数は宣言costと照合する。prefix bindingでは、jobの最大長と実際の生成長の両方をREAL contextの非零exact decodeと照合する（MUST）。range input commitmentは実prompt由来とする。
4. 内部演算はchecked arithmeticとし、node cost `< 2^54`、各次元 `< 2^120`、10次元合計 `< 2^124` を超える入力を拒否する（§12.5）。共有DAGの走査はvisited setで各構造nodeを一度だけ訪れる。
5. 履歴の再実行（restart/replay/reorg）を新しいuseful workとして数えない。
6. `ExecWorkReleaseResourceBounds`（名前付き）: 最悪graph、root population、256-root restartのlatency/memory上限。**value: USER DECISION REQUIRED**（release閾値。既存記録に推奨値なし）。

##### R8-IDENTITY — useful workの一回利用

1. `job_work_id`はclass、REAL job authorization、認証済みinputだけから導出する（MUST）。cuts、output依存の初期state、verification metadata、continuation extent、root id、owner label、bondに依存させてはならない（MUST NOT）。任意のinput hashでkeyを変える経路を拒否する。
2. 一つのauthorizationで複数rootを開けない。記録が無いことを再利用の許可と扱ってはならない（MUST NOT）。新しいauthorizationは正当な同一inputの再実行を許す。
3. 有限authorization（§12.6）: root openingは `max(source_daa, accepted_daa) <= opening_daa < source_daa + L_auth` の間だけ許される。`L_auth`は休眠v1で100,000 DAA固定、job-use履歴は最大100,000行・1 blockあたり最大256件の期限切れ削除で、期限内の行を追い出さない。これらはv1構造profileであり、release memory/latencyや経済admission capの承認ではない。`L_auth`・履歴上限の本番値: **value: USER DECISION REQUIRED**。
4. 元のclockはnative claim受理時に捕捉する（§12.7）。後のheader可用性や呼出側bytesで変更できない。legacy claimは明示的なcarried original-header proof（§12.8）でのみ移行する。source/acceptanceの順序を仮定しない開始条件とする。
5. Remaining（観察、未検証）: 全eligible legacy headerのcoverage/transport可用性、実DAG/multi-process pruned replay。

##### R8-BUDGET — 予約、Final前無支払、実debit

1. **Final前無支払:** slice leg、bound kernel fee、Panel fund、EXEC executor fund、riderを含む全てのpositive legは、root claimのnative Final前に支払ってはならない（MUST NOT）。bound kernel claimの計算Finalはreward 0とし、job escrowをconsumer所有のhold（kernel table 31）に保持する。同じjobの別producerのFinalもholdを迂回できない（§13.1）。
2. **root Final精算:** root Finalで一つのallocationのexecutor share内から実際のfeeを支払い、同額のnative issuanceを減らし、未使用escrowをposterへ返す（MUST）。actual debit、payout、root marker、refundを同じdeltaに記録する。void/expiry（ready rootの有限expiryを含む）は全holdを無支払で閉じる。既払prefix/sliceの後付けは拒否する。
3. **受理前予約:** accepted claim、fee、bondの書込みより前に共有Q/R claim budgetを予約する（MUST）。standalone kernel claimはQ/Rだけを予約し、BとFを与えない。round drawは各bondの残存権利から候補を作り、一つでも予約が失敗すればdraw全体を取り消す（原子的）。
4. **fee-only Round（2026-10-11ユーザー判断）:** fee-only Roundは`B_max`に数えず、資本比例の独立`ExecutionCap`で権利割当時に制限・課金する。失敗・未実行で権利を回復させない（MUST NOT）。`ExecutionCap`の資本比率: **value: USER DECISION REQUIRED**。
5. **market fee（2026-10-11ユーザー判断）:** v1ではmarket feeを`R_max`に含めない。ただし期待手数料収入（fee利得）は経済安全性・抑止条件の計算に含める。fee額そのものの上限を証明したとはしない。（設計監査「追加受入条件と政策判断」の「market feeとissuanceの合計を`R_max`に含め、約定前にfee最大額を予約する」という記述は、[readiness matrix](../design/palw/integration/activation-readiness-matrix.md)に記録されたこのユーザー判断と矛盾するため、v1については本項で置き換える。）
6. **予算window `W_budget`（ADR-0176 D4）:** 400 DAAはdrill暫定値である（2026-10-11）。条件は対象profileが`L <= W_budget`とslot上限を満たすこと。長いG14 staged経路へそのまま適用しない。RFC12のwork量`W`とは別物である。本番値: **value: USER DECISION REQUIRED**（推奨: 400 DAA、上記条件付き。理由: ユーザーがdrill用に採用した値であり、それより長い経路には別の値が必要）。
7. **allocation freeze:** retained EXEC rootがleadを名指しした後、riderによるescrow再配分を拒否する（§15.7）。
8. **状態（2026-10-15、lead記録）: closed-as-provisional。** R8-BUDGETの仕様は次の値を**drill暫定（ユーザー判断10-11）、本番値はcensus/実測待ち**として閉じる: 予算window `W_budget` = 400 DAA（対象profileが`L <= W_budget`とslot上限を満たす場合のみ。長いG14 staged経路には適用しない）。fee-only Roundは`B_max`の外で、資本比例の独立`ExecutionCap`に従う（比率の値は未決）。v1のmarket feeは`R_max`の外。presetは変えず、fenceは武装しない。

##### R8-WINDOW — 依存claimの責任・material・post-Final

1. **依存担保の床:** bindingの時点で `H_exec = expiry + max(native_window_court, kernel_liability) + kernel_court_deadline + kernel_proof_grace` をchecked arithmeticで依存claim行に固定する（MUST）。計算Final、pre-Final default、root fee精算、cancelでこのhorizonを短縮してはならない（MUST NOT）。支払済み・failed・予約ゼロ・期限切れのworkの後付けを拒否する。restoreは全依存のbase floorを再導出する。
2. **Settled後の有罪:** Settled後の依存claimのconviction/defaultは、rootのnative Final、safe weight、native evidence、未解放vesting、payout全体へ一貫して伝播する（MUST）。kernelは自身のcollateralを処罰し、伝播だけを理由に他actorを追加slashしない。`void_claim`だけで巻戻しを済ませない。
3. **native紛争期限:** native DA/courtの延長は、依存pathの最短の署名済みhorizon内に収まらなければならない。超える延長は拒否する（§15.4）。既存termを越える延長は、新しい署名・新しい資金による明示の同意だけで行う（未実装）。
4. **X3（2026-10-12ユーザー判断）:** claim単位の絶対期限（固定点d0＝claim受理時）を延長しない規則は新しいversioned fenceでのみ適用する。live t12の旧規則は変えない。
5. **workerの同意:** `nonce_v2 = H(slice-job-nonce/v2; nonce_v1 || expiry || H_exec || executor_exposure_sompi || policy_root)` を署名済みkernel claimで拘束し、最初の署名済みsliceでだけ予約する（§15.6）。allow-listは同意ではない。
6. **Panel署名lifetime（inner 66/65、aux 121、`palw_panel_lifetime_v2`）:** producerはadmission前に絶対受理区間・service/liability endpoint・retry/pause予算・最大PWU/金額へ署名し、native birthで実statementを捕捉する。retryは同じendpointを共有し、pauseはcounterをresetしない。expiryは時間内のcourtを中和せず、獲得済み現金を消さない（MUST NOT）。名前付きパラメータ`PalwPanelLifetimePolicyV2 {maximum_service_daa, maximum_pause_daa, maximum_retries, maximum_enrollments, maximum_enrollments_per_producer}`（不変条件: `0 < maximum_service_daa < u64::MAX`、`maximum_pause_daa <= maximum_service_daa`、`0 < per_producer <= maximum_enrollments`、security/economic review scopeが非零）: **value: USER DECISION REQUIRED**。休眠codeのprobe値は政策値ではない。
7. **monetary material covenant（outer 240–242、aux 120、`palw_monetary_material_v1`）:** 全provider・sponsorが完全なroot/依存、capacity manifest、担保、自己資金fee、物理的支払window `W_mat`、payment-relative義務に署名する。liabilityまたはqueueが残る間、holdはMAXとする。materialの解放は**最後の実支払 + `W_mat`**以後であり、release blockが現在のselected chainでfinalizedであることを物理GCの条件とする（MUST）。sparse DAAやqueue滞留でproviderの新しい義務は期限切れにならない。欠落delta、古いtip、未完了import、generation変化、契約の不在はGCを許可しない。名前付きパラメータ`PalwMonetaryMaterialPolicyV1 {max_contracts, max_contracts_per_provider, max_bytes_per_contract, max_bytes_per_provider, minimum_collateral_sompi, minimum_fee_sompi, window_daa (= W_mat), response_window_daa, max_enrollment_daa, cost_report_root}`（不変条件: 全て非零、`max_contracts_per_provider <= max_contracts`、`max_bytes_per_provider >= max_bytes_per_contract`、`cost_report_root`は承認済みreview由来）: **value: USER DECISION REQUIRED**。固定前払feeを未知の支払・回復時間の安全な対価と表示しない（`PaymentAndRecoveryTimeUnknown`）。
8. paid material: native paid契約の`material_until`は署名済みEXEC retention horizon以上でなければ、fee/R移転・root/work-use書込みの前に拒否する（§15.11）。
9. Remaining（観察、未検証）: 実nodeのnative root Final後の公開告発と支払回復、承諾付きhorizon延長、pruned join、workerの最低対価・資本費用。
10. **承諾付きhorizon延長（2026-10-15、休眠実装）:** `palw_horizon_extension_v1`（kernel inner 68、aux 126）。EXECに束縛されたkernel claimの生きている`liability_until`を、worker（claimのproducer）とsponsor（jobのposter）が同一termsへ各自署名して延長する。全員の署名が揃った時点で原子的に適用し、(a) 延長だけ（Final・court窓を短縮しない）、(b) claim固定のhard deadline（claim受理`d0`からの`maximum_claim_horizon_daa`、EXEC rootのnative claimにX3が適用される場合はその`dispute_hard_deadline`との`min`）を越えない、(c) 期限切れhorizonを再開しない、(d) 獲得済み現金を取消さない、(e) EXEC income holdを同じhorizonまで延長する、(f) transferred claimのproviderは自身の署名済みlease（tag 150）が新horizonを既に覆うことで同意する（leaseを書換えない）。名前付きパラメータ`PalwHorizonExtensionPolicyV1 {maximum_extension_daa, maximum_consent_window_daa, maximum_claim_horizon_daa, maximum_open_extensions, require_price_contract, review scopes}`: **value: USER DECISION REQUIRED**。

##### R8-SLICE — 重複・再利用・一精算

1. 同一executorの支払はslice毎にfloorせず、executor毎のworkをchecked arithmeticで合計してから一度だけfloorする（MUST）。slice分割の変更で同じexecutorの支払額を変えない。
2. 新しいleg全体は未使用残高で制限し、不足時はexecutor毎に比例配分して整数端数をroot legに残す。advanceがallocation全体を既に超える場合、新しい支払は0。
3. 公開prefixの再利用、同じ計算Final行の再適用、claim/revealの再送で二度精算しない。rootは一度だけ精算する。
4. Remaining: native root Final／精算とその再起動・reorgまでの閉包（validation-only）。

##### R8-NEUTRAL — 順序・分割の不変性

1. admission、ticket、allocation、payoutの結果は、operator id、genesisでの順序、class/bondの登録順を入れ替えても変わらない（MUST）。tie-breakは内容由来のcanonical keyで行い、登録順やoperatorの身元を優先しない。
2. 同じ資本をbond、model、claim、root/slice/riderに分割しても、Q/B/R/Fの合計、ticket、モデル配分（ADR-0177 D3の`S_m`）を増やしてはならない（MUST NOT）。
3. 登録operator idを独立した経済主体の証明として扱わない。同じ実質所有者が複数operatorを持つ可能性を残余リスクとして明記する。
4. per-bond ticket上限は`B_max`から導出したものを使い、別の値を定義しない。
5. Remaining: 全callerでのswap/split試験（validation-only）。

##### R8-RECOVERY — 世代一致、restart/reorg/pruning

1. budget（carriage tail `0xEF`、delta 190/191）とEXEC（tail `0xEE`、delta 180/181）、reward vesting（`0xF4`）、actor extension（`0xF5`）、job-use履歴（`0xF2`）は同じ世代でcarriage/復元する（MUST）。片方を捨てる統合を禁止する。
2. native clock（F3/table 8）と明示的migration proofはrestore・undoで同じ結果を与える。rootはlocal header lookupに依存しない。
3. material GCは、active profileがcommitted generationを読めない、sinkが読取中に変わる、pruning importが進行中・不安定・読取不能・epoch変化の場合、`Some(u64::MAX)`でholdを保持する（MUST）。読取errorを「義務なし」と解してはならない。
4. pruned joinは、同じ認証済みauthority、source/failure/fee、material commitmentとjournalを完全に受け取らなければならない。snapshotに収めるために履歴のfailureや失敗promiseを落としてはならない（MUST NOT）。
5. Remaining: settled native root、OS crash、migration履歴の全coverage、multi-process P2P/実pruningの証拠（validation-only）。

##### R8-ECON — 収支基準

1. [§15.7](../design/palw/execution/rfc-0008-implementation-spec.md#157-worker-price-criterion-and-allocation-freeze)の基準を規範とする: `p_collect_lower * M_net_min >= C_upper`。`p_collect_lower`はnative Finalとrelease時の回収可能性の**同時**事象の証明済み下限であり、独立性の根拠なしに周辺確率を掛けない（二つの周辺下限a, bからは`max(0, a+b-1)`）。資本は各時点で一度だけ数える。
2. 費用・回収floor・funded minimumの証拠が欠ける場合の判定はUNKNOWNであり、肯定判定にしない（MUST NOT）。未実装の上限を実装済みとして比較しない。
3. 名前付きパラメータ `capital_rate`、class別`C_upper`の測定値、`M_net_min`を拘束する最低価格契約: **value: USER DECISION REQUIRED**（既存記録に推奨値なし。最低価格契約は未実装）。
   （2026-10-15補足: 最低価格契約は休眠fence `palw_minimum_price_v1`（kernel inner 69、aux 127）で実装した。workerとposterが同一termsへ署名し、jobのGAP-5 escrowが`M_net_min`を賄い、まだworkerが予約されていない時だけ受理する。`C_upper` = class別測定値 + `capital_rate`×資本時間面積（一度だけ）、`required_M = ⌈C_upper / p_collect_lower⌉`未満の申出、未測定class、`p_collect_lower = 0`（UNKNOWN）、u64を越える見積りを拒否する。契約が支配するjobは契約workerのclaimだけを受理期限内に受理する。名前付きパラメータ`PalwMinimumPricePolicyV1 {capital_rate_ppm, capital_rate_period_daa, p_collect_lower_ppm, class_cost_upper, maximum_contracts, maximum_contract_daa, cost_report_root, economic_review_scope}`: **value: USER DECISION REQUIRED**。精算時の最低額強制（settlement enforcement）は未実装。)
   （2026-10-15 round 3補足（休眠source、未compile・未test）: **精算時の最低額強制**と**期限切れ契約・古い同意のsweep**を実装した。(a) `load_ledger`は署名済み契約ごとに、そのjobを保持する契約workerのclaimへ`funded_minimum_sompi`をpayout floorとしてkernel ledgerの`consumer_minimum_payouts_v1`（consumer注入、row/carriageではない、restoreで空）へ入れる。(b) kernel ledgerはFinal（およびEXECの`finish_exec_reward`）でjob escrowから`paid = min(escrow, posterの残bond, consumer cap)`を計算し、`paid < floor`なら**少なく払わず**、契約結果をneutralとする: producerへのescrow支払いは0、escrowは全額posterへ`ReleaseJobEscrow`で返す（`paid ≥ floor`なら従来どおり）。fenceより下ではfloorが空で、支払いはbyte同一。(c) kernel routeのclosing stepの先頭（そのblockのobject適用後、block DAA）で決定的sweepが走り、aux 127の契約行（とjob index）を次で退役させる: 全署名前に`accept_by_daa`を過ぎた同意＝StaleConsent、`retention_until_daa`超過＝Expired、`accept_by_daa`超過かつ契約workerのclaimなし＝Unused、job escrow消滅（支払・neutral返却・idle返却）＝Settled。aux 126の延長行は未適用で`consent_by_daa`超過（古い同意）、または適用済みで`new_liability_until_daa`超過で退役する。退役はcapacityを解放するだけで、現金・claim・horizonを動かさない。(d) restore fenceは、route ledgerのDAAで既にsweepが退役させたはずのStaleConsent/Expired契約と古い延長行が残っていれば拒否する。式: `floor(claim) = funded_minimum_sompi`（署名済み契約、`job_claims[job]`のproducer = worker）; `paid = min(amount, collateral_poster, cap)`; `paid < floor ⇒ paid := 0, ReleaseJobEscrow(amount)`。新しい値は導入しない。neutral時にworkerへ何も払わない規則の妥当性（契約上の不払い責任の所在）は**USER DECISION REQUIRED**。)
4. Remaining: 需要・cap前後・window飽和時の実測比較（validation-only）。

##### ALL-MODEL

§6.1とADR-0177を維持する。追加で、2026-10-12のユーザー判断に従い、モデルbytesを強制するcourt経路はRFC6の2026-10-15節に記す休眠の制限fenceで閉じ、新しいdemand unitはclaim固有のwitness/state/traceに限る。公開検証できない証拠経路（N4）は新fence以後のclaimを新規報酬・weight対象から除外する。

##### 本文との関係

§4の会計表と§7のgateは本節で補完される。旧記述を削除せず、衝突する場合は本節と[実装仕様](../design/palw/execution/rfc-0008-implementation-spec.md) §§12–15を優先する。設計監査の旧記述「実際に回収可能なmarket feeとissuanceの合計を`R_max`に含める」は、v1では2026-10-11のユーザー判断（market feeは`R_max`外）で置き換えられたものとして扱う。

#### 整合性修正 — 2026-10-19(RFC 全件整合性監査)

- R8-WINDOW(依存 path の最短の署名済み horizon を越えて延長しない)は維持する。RFC-0014 2026-10-19 改定の係争延長(最大 `a + 10,000`)とは、**新しい original claim の受付時に、依存契約が `a + 10,000` と必要な後続責任を既に覆っていることを検査し、覆わない claim を受け付けない**ことで両立させる([RFC-0026](../rfc/0026-court-policy-master-contract.md) §3.3、F07)。延長のたびに依存先の同意を取り直さない。
- clock machinery が court 停止中も進むこと自体は、応答機会があった証明にならない(RFC-0026 §4.1)。

---

<a id="source-v1-0038-L00001"></a>

### 旧RFC-0038 / 原文1–49行

出典: [0038-palw-fp-answer-accounting.md](../rfc/0038-palw-fp-answer-accounting.md)。配置理由: 主題別統合。

### RFC-0038 — free-prompt の回答トークンを支払済み計算として記録する(FPA1 carriage)

Status: **Draft** (2026-10-22, compute-claim audit lane compute-exec)。[ADR-0205](../adr/0205-a-decoded-answer-is-not-paid-twice.md)。
fence: `palw_fp_answer_accounting_v1`(dormant、testnet-12 full-activation list の entry、arming record が pin されるまで
`validate_palw_v2` が arming を拒否)。

#### 動機

ADR-0145 §6 は「class が既に支払った prefix は再び支払わない」と定めるが、chain が記録するのは **prompt** だけで、
支払済み claim が **decode した回答** は記録されない。決定的な因果モデルでは位置 `t` の計算は `0..=t` のトークンの関数
なので、

* 支払済み C0 =(P, D)の回答 g を続けた C1 =(P ‖ g, d)は C0 が計算済みの位置を「prefill」として再度受け取る
  (testnet-12 で live: `palw_fp_derived_work` は genesis、decode rules は dormant)。decode rules 下でも
  C_k =(P ‖ g[..k], D − k)の梯子で Σ(D − k) ≈ D²/2 の credit を P + D の計算で得る。
* 同じ(P, D)を別 bond で commit すると(`fp_work_id_v1` は bond ごと)decode 全体が再度 credit される。leaf の値は
  bond 非依存で、別 bond の context で再 hash するだけ(hash コスト)。

いずれも正直な実行なので SEAT-R の全再実行も court も拒否できない。価格だけが拒否できる。

#### 仕様

1. **carriage**: `FPA1 ‖ borsh(Vec<u8> inner) ‖ borsh(Vec<u32> answer)`。`inner` は回答なしで載る payload
   (plain V3、`TFP1`、`HFP1`)。先頭 magic なので plain payload(先頭 2 byte は version)や他の envelope と衝突しない。
   入れ子の FPA1 は拒否。fence を ruleset に持たない network では isolation が拒否(PayloadUndecodable)。
2. **fold**(fence 以降、claim の受理 DAA で判定): compute-priced commitment は `decode_tokens_executed` 個の回答を
   native ingress から受け取る。無い/長さ違いは `FreePromptAnswerNotAccountable`(報酬・weight・credit なし、carrier は
   通常 fee)。
3. **記録**: prompt row の横に `prompt ‖ answer` row を別 key(`palw_fp_answer_row_key_v1(claim)`)で書く。expiry は
   prompt row と同じ(100 epoch、epoch 境界で sweep)。
4. **価格**: `c` = 有効な支払済み row のうち、同じトークンで既に計算された先頭位置数の最大
   (prompt row は全位置、sequence row は最後の回答トークンを除く位置)。`c ≤ P` なら `c` が accounted prefix、
   `P < c < P + D − 1` なら credit は位置 `c..` のみ(prompt `c`、decode `P + D − c` として導出)、
   `c ≥ P + D − 1` は `ZeroQuanta`。
5. **回答の拘束**(arming 前提): seat は replay で claim の回答を知る。carried answer が claim の `output_root` を
   claim の context で再計算しない claim には licence を出さない。嘘の回答の claim は licence されないので、
   嘘で後続の coverage を縮めることはできず、植え付けた coverage は実際に計算した列にしか一致しない。

#### 互換性

fence 未満は byte-identical(answer row は書かれず、価格関数は `answer = None` で従来と同一、FPA1 は undecodable)。
fence は Some-only で両 fingerprint と fork id に入る。

#### arming 前提(未実装)

* processor の native ingress: FPA1 を剥がして `extras.fp_claim_answers` を claim id で埋める(TFP1/HFP1/TFO1 内側も)。
* isolation door: ruleset に fence があるときだけ FPA1 を受け、inner を従来どおり検査。
* seat の回答照合(仕様 5)。
* producer(worker / fp-submit / gateway / CLI)が FPA1 を出す。
