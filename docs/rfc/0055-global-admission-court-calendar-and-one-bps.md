# RFC-0055 — 全モデル共通の受付難易度・資源予約・総搬送1 BPS

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: ADOPTED_DESIGN_NOT_ACTIVATED。設計の記述であり、実装・測定・公開有効化の完了ではない。
- 判断記録: [ADR-0223](../adr/0223-global-admission-court-calendar-and-one-bps.md)
- source基準: MISAKA-BTC/misakas pre@c3da7c6eaf5bf87f8e032d5b122c603d720a8398
- 新版: UnifiedAdmissionV1 / CommonCheckerProfileV1 / GlobalServiceBookV1 / UnifiedCarrierV1
- 主題別RFC20冊を維持する。本書は統合後の変更記録である。

## 0. 最新の指示と結論

ユーザーは、全モデルの新規提出を難易度で絞り、courtの容量を守り、全体1 BPSへ調整する一体の仕組みをRFC化し、設計を閉じるよう指示した。120秒の中の30秒で通常処理を行い、反証処理の余地を残す例の後、**一件・三件・五件等は説明例であり、court件数を固定せず、過剰な計算負荷でNodeが落ちない構造を求める**と明示した。本書は最後の指示を優先する。

本版の決定は以下である。

1. court/claimを「常に五件」「通常一件＋反証三件」と固定しない。**共通検査形式の安全な費用上界と、全モデル共通の残資源から受付可能量を導く。**
2. 120秒は計画・集計・difficulty更新区間とする。30秒の専用時間帯、claim一件/周期、court三件/周期を規範にしない。
3. 新modelごとのbenchmark・人手審査・model名allowlistを受付条件にしない。共通checker、codec、cost meter、Node共通経路を版ごとに較正する。
4. 同じ規則で表せる未知modelは、そのclaimのshapeから上界を導き、全合法継続を予約できれば自動受付する。
5. 共通難易度は新規需要を調整する。安全上限は事前の多資源予約、実行meter、Node内部のmemory/queue/work budgetで強制する。
6. 受理済みの適格notice・反証・応答・終端は元の保護予約を使う。新しい難易度へ通し直さない。
7. 全種類の外側carrierを共通の1 permit/秒で運ぶ。1 BPSだけをcourt容量やCPU安全性の根拠にしない。

設計閉鎖とは、形式、上界、受付、割当、停止、実行、精算、復元、移行の判定を本書で定義することである。機器性能を推測で採択したこと、実装が完成したこと、任意の機械/OS/ディスク障害でも絶対に停止しないことは意味しない。防ぐ対象は、許可入力や大量提出が管理下の仕事・allocation・待機状態を無制限に増やすことである。

## 1. 原則・変更範囲・経済的機会

- Minerはcanonical整数規則で計算し、初回に全traceと分解位置へcommitする。
- Panelは全体検証、Nodeは有界な認証・局所計算・native状態検査を行う。全モデル取得・全推論・GPU fallbackを追加しない。
- 局所positiveだけでFinalにしない。独立承認、検査/異議機会、Q解消、依存・資金・原期限を維持する。
- 適格な一人の反証を他者の先着占有や新規需要から保護する。未知第三者全員に無制限容量は約束しない。
- 暗号学的計算証明は採用しない。hash、署名、opening、既存beaconは同一性・認証・乱数に使う。
- tier1.0/0.3、モデル審査不採用、追加model burn停止、一つの合意authority、延期機能を維持する。

本RFCは、Final後の既存P_m、CCU、モデル生成権、有限ExecCreditの配分・消費規則を変更しない。原受付はこれらの権利を発行しない。**モデル別の実受理率、Final到達率、実ブロック数は候補供給・資源要求・既存予約に依存し、P_m比例を保証しない。** 新しい均等なblock配分や一claim一blockを追加しない。

新profileに限り、旧RFC-0048/ADR-0216のEXEC専用外側1 BPSを、全kind共通carrierの1 BPSへ改定する。旧wire・署名・Final・DAA・既受理責任は遡及変更しない。

## 2. 共通profileとclaim固有descriptor

### 2.1 CommonCheckerProfile

共通checker profileはmodel一覧ではなく、以下を定義する。

| 対象 | 規範 |
| --- | --- |
| 数値意味 | 整数幅、積和/還元順序、丸め、飽和/overflow、量子化、乱数状態 |
| 演算 | 対応kernelと複合演算を有界な局所関係へ分解する構成規則 |
| 証拠 | codec、wrapper、署名、opening、source位置、項数/深さ/parts/展開量の上限 |
| 完全性 | WellFormed / Init / Step / End、値の出どころ、状態接続、出力・停止・仕事量 |
| 費用 | parse・hash・署名・演算・lookup・書込み等の安全上界関数 |
| planner | 有限graph、探索/索引/workspace/エラー処理の上界と費用 |
| 版 | kernel/schema/cost-meter/compiler/checker hashと互換性 |

既存kernelで表せる未知modelの名前・重み・層構成の追加に、個別qualificationを要求しない。未対応kernel、意味を保つ分解がない数値規則、上界不明のshapeはUnsupportedShape/UnsupportedKernelとする。model品質・公開状況・保有者数の審査ではない。

### 2.2 NodeCapacityProfile

最低要件機器とbinaryについて、checkerに加え、通常取引、state/index/root、journal/undo、cold I/O、compaction、再起動・reorg・IBDの**同時混合負荷**から供給量を定める。checker単体のmicrobenchmarkだけでNode全体の容量を決めない。

必須欄は、有限のflow/stock容量、safe margin、system/control/recovery予約、時計/包含条件、carrier burst、最大登録/未完了状態、測定記録である。各Nodeの瞬間CPU使用率や空きRAMを合意値にしない。

### 2.3 ClaimResourceDescriptor

登録programの認証された構成、job geometry、入出力/文脈長、trace方式、actor義務、期限から、claim固有のdescriptorをnativeに導く。提出者の「軽い」という申告を信じない。descriptorを原claim・共通profile・料金版へ束縛する。

共通profileは版ごとにqualificationする。claim descriptorは共通規則で自動検査する。実LLM試験は変換器・記録・Panel・Nodeの接続を見るために行い、将来modelの個別性能審査へ転用しない。

## 3. 大きな計算を有界な裁定形式へ変換する

MatMulやAttentionという演算名だけで一courtと扱わない。共通の項数上限B、整数幅、source read数、認証経路、workspace上限で分解する。Bは共通checkerの較正済み上限であり、modelごとに緩めない。

内積は最大B項とcanonical累積値の前後を一つの局所関係にする。softmax、正規化、argmax、routing、MoE、KV/再帰状態、token選択、停止も、算術と接続を漏れなく裁定可能にする。

~~~text
ValidTrace = WellFormed AND Init AND AND_i(Step_i) AND End
~~~

Minerは最初のcommitで、分解規則、位置、前後値、source、状態、終端を固定する。異議後にPanelが作った部分和をMinerの元記録として裁かない。

分割数が大きいからといって、正常時に全区間を全Nodeへ送らない。Panelの全体検証、必須の有限normal検査、異議時の局所courtを分ける。構成規則に穴があるshapeは、署名が集まっても成功させない。

## 4. 件数を資源から導く会計

### 4.1 flowとstock

処理jの費用は単一gas点数だけでなく、以下のvectorで扱う。

~~~text
q_j = (deterministic_work, signature_checks, hash_bytes,
       parse_and_expand_bytes, wire_bytes, io_read_bytes,
       io_write_bytes, io_operations, control_operations, carrier_space)
~~~

RAM、queue、unfinished parts、live claim/case/metadata、retained disk、journal、IBD/WAL/compaction、GC backlogは、占有期間付きstockで別計上する。CPU workの内訳にhashを含めても、hash/bytes等の独立上限を同時に検査する。費用単価と物理上限は別である。

全itemに非ゼロの固定受付・参照・索引・wrapper費用を課す。算術workゼロの空packet・重複・controlでも、memory/record/parse/control費をゼロにしない。有限decode record/parts/visited-object数は、資源保護として共通形式に残すが、一律の「court五件」へ置換しない。

### 4.2 共通予算

区間I、資源rについて、Cをqualification済み供給、Sをsystem/control/EXEC/復旧の先行予約、R_oldを既受理claimの未消化最悪継続、R_newを候補の全責任とする。

$$
S(I,r)+R_{\rm old}(I,r)+R_{\rm new}(I,r)\le C(I,r).
$$

新規受付は全資源・必要な全区間でこの条件を満たす場合だけ許す。stockも全時点で同じ形の上限を守る。正常処理だけを先に受け付け、反証の余地を後で探さない。queue分離だけで物理供給が増えた扱いにしない。

### 4.3 説明用の件数と実際の判定

同一費用qの独立処理だけなら、説明用に次を使える。

$$
N_{120}\le\min_{r:q_r>0}\left\lfloor\frac{C_{\rm free}(120,r)}{q_r}\right\rfloor .
$$

少なくとも固定受付費の次元は正である。RAM・保持・依存・deadlineは別に検査する。異なるcourtの混在では各q_jの和と具体的な予約を使う。

同じ120秒で小さい証拠を多く、重い証拠を少なく扱ってよい。30秒を通常処理に使い、残りへ二件・三件・五件を置く説明は例に限る。安全な最大量はmodel数/名前ではなく、共通規格・Node容量・既存予約・claim shapeから決まる。

## 5. clock・120秒計画・原期限

120,000msを計画/集計/difficulty epochとする。RateClockのoriginと認証clock規則をactivation recordへ固定し、epoch eを[origin+120000e, origin+120000(e+1))とする。境界で予約をゼロに戻さず、未完了義務とstockを持ち越す。

これは120 DAAではない。原claimのa、notice a+3,120、hard a+10,000と保持期限は元の意味を維持する。carrier一秒をDAA一増加にしない。

ClockBridgeは、canonical cursor、lead cap、包含規則、許容遅延、故障モデルから、taskの最遅releaseと最早deadlineを保守的に導く。1 DAA=1秒/120秒という等式で代用しない。橋渡し不能なshapeは受付不能である。

task完了上限はfinish<=deadline_ms。元DAA条件がstrictならそれを保つ上限へ変換する。service区間は半開、終端で完了してよいが終端から開始しない。stopwatchは較正/監視に用い、Minerの正誤や過去blockの永久validityを決めない。

## 6. 全モデル共通の新規受付難易度

### 6.1 全新規route

新profileのAttempt、FreePrompt、TFO3、atomic batch等をUnifiedAdmissionV1へ統一する。共通D、形式、資金、全体予約を必須とする。経路未接続・未知profileはdefault deny。

既存のbond/経済的機会/route資格は追加の必要条件として維持する。Dに通ったことはP_mの生成権や報酬ではない。旧W・Final後receipt抽選の履歴を上書きせず、新規受付前の容量保証の代用にもしない。

### 6.2 presealと未知seed

heavyなPanel/court義務の前に、bounded presealでmodel/program/weights、canonical入力/生成条件、出力/trace、work identity、producer/bond、shape、profile、実費/資金同意、署名の最終期限を固定する。

target epoch eのpresealはe−2中に受け付け、Start(e−1)で集合をfreezeする。e−1中のfreeze後の既存beaconから、集合root/network/policy/targetへ束縛した未知seedを得る。Start(e)までに成立しなければ、そのtargetの新規受付を行わず、既受理courtを継続する。

bootstrapでは負のepochの集合を捏造しない。epoch 0/1の新規admissionは空とし、既受理court・control・carrierは継続する。epoch 0でtarget 2用presealの収集を開始する。旧版presealの暗黙移行や即時のseed後受付は認めない。

D_eはpreseal開始前に固定する。seed後の集合追加/threshold変更、別seedへの無料carryを禁止する。署名bytes、RPC id、carrier txid、任意nonceの変更は別仕事ではない。

preseal自体のglobal control/bytes、署名、順位索引、planner、資本、fee、metadata/保持も予約する。件数はその残量と非ゼロ費用から導く。既存の同一資本/canonical-work/複数登録上限を全model合算で入口にも適用する。modelや鍵を増やしてglobal予算を増やさない。

### 6.3 ticketと候補順

H256はSHA-256、数値は固定幅little-endian、可変bytesはu32長付きcanonical encoding。全長/countをdecode前に検査する。

~~~text
PresealId = H256("MISAKA/UAI1/preseal" || canonical_fields)
ticket = uint256_be(H256("MISAKA/UAI1/ticket" || network ||
                         policy_hash || target_epoch || PresealId || future_seed))
T(D) = floor((2^256 - 1) / D)
eligible = ticket <= T(D)
admission_rank = (ticket, PresealId)
~~~

ここへtier/CCU/P_m係数を二重に掛けない。共通Dは新しい検証責任の候補資格に限る。生成機会と有限credit/receiptは既存の適用版で別途判定する。受付比率をP_m目標達成と報告しない。

同一workのcarrier交換・再署名・別routeで追加抽選を作らない。既存canonical仕事identity/資本sequenceと認証台帳で一回性を保つ。dedup情報の保持/索引/pruningを予算化し、削除によって旧workを再び新規扱いしない。

ticketは計算の正しさや「必ず高価な推論をした」証明ではない。虚偽commitの安価なgrindingは、bounded preseal、実費/資本、global ingress、検証/責任と併せて扱う。

### 6.4 件数を固定しない決定的受付

closed_preseal_root、seed、D、開始時book/basisを固定し、eligible集合をadmission_rankで一巡する。順位付け/索引/走査自体にも予算を確保する。未検査候補を無制限に集めてからfitしない。

AdmitBatchは保存cursorから有限の次prefixを処理する。候補ごとに同じcanonical prefixの実資金、既存資格、共通descriptor、全工程予約を検査する。成功した予約を後続候補へ反映し、不成立なら理由を固定して同epochで別順序の再試行をしない。大きい候補の不成立後にも、後続の小さい候補を検査できる。

batchの訪問数はplanner/parse/lookup/書込みbudgetから導き、一訪問の最小費用をゼロにしない。carrierに入るprefixだけを処理し、残りcursorを次へ持ち越す。全model表を走査しない。

epoch終了時の未処理/未受理presealはExpired。新seedの無料抽選を得ない。受理件数は固定せず、予約が成立する分だけとする。零件のepochもある。

### 6.5 資源負荷に連動するdifficulty

Dは1〜2^120−1、比率は1024単位、中間はchecked uint256。overflow/state不整合は新規受付停止とし、wrapで合格させない。

epoch b開始時に直前完了epochと現在の全未来予約から計算し、preseal開始前のtarget b+2へ適用する。target b/b+1は変更しない。bootstrapのb=0では直前epochを仮定せず、D_2=D_initial、low_run=0を初期化してから収集する。b>=1から更新則を実行し、D_previous_publishedは直前に公表したD_(b+1)とする。D_initialも1〜D_maxを満たす。

- U: stock占有率と、有限horizonの各120秒区間のflow予約率の最大。ceil(1024*reserved/capacity)。未開始の最悪合法継続も含む。
- demand: 形式・資格・preseal費用を満たしたeligible新規候補が存在したか。
- congested: 単独なら規格/期限に入る候補が既存予約との競合でNoCapacityになったか。
- unavailable: 共通profile不完全、canonical履歴で確認される未解消のservice不足、合意状態の復元不整合などで受付不可か。各hostの遅延・CPU/RAM・ローカル復旧状態を合意controllerへ投票しない。

unsupported shape、資金不備、invalid packetを混雑成功と数えない。host CPU/free RAM/RPC申告を入力にしない。供給ゼロで需要正の次元は受付不能、双方ゼロなら使用率0とする。

~~~text
high = demand AND ((U >= 768) OR congested)
low  = (U <= 256) AND NOT congested AND NOT unavailable

if unavailable:
    D_future = D_previous_published
    low_run = 0
elif high:
    D_future = min(D_max, ceil(9 * D_previous_published / 8))
    low_run = 0
elif low:
    low_run = min(3, low_run + 1)
    if low_run == 3:
        D_future = max(1, floor(8 * D_previous_published / 9))
        low_run = 0
    else:
        D_future = D_previous_published
else:
    D_future = D_previous_published
    low_run = 0
~~~

固定件数を目標にしない。需要ゼロ、容量ゼロ、候補はあるがeligibleゼロを別に記録する。需要がない間は既存予約だけで難化させ続けない。急増・追従遅れ・最大Dでもhard admissionが超過を拒否する。

## 7. 全工程予約と原子的受付

### 7.1 予約対象

~~~text
R_original =
  normal_verification + every_qualified_actor_remedy + finite_outsider_routes
  + legitimate_defense + terminal_and_financial_close + bounded_recovery
  + material_retention_and_gc + carrier_and_control
~~~

最短の反証一件分では足りない。最後のnotice、最遅応答、悪意多数の合法な虚偽case、InspectionPending、防御・終端・保持を共通テンプレートとshapeから導く。

nativeに相互排他性を検査できる枝だけ予約を共有する。共有bytesの節約でactorの異議機会を統合しない。未知第三者の有限資格・累積予算も原profileへ固定する。

Nodeの局所checkerと別に、cold Panelの認証付きmodel/入力取得、完全再実行、比較・局所化・opening構成、providerの実供給を役割別service容量へ計上する。共通kernel/cost式、資料bytes、claim shape、既存の供給同意から上界を導き、新model個別benchmarkは要求しない。同じ物理資源を兼用する役割は供給を重複計上しない。Nodeが軽く検査できるだけでPanel readinessや期限内供給が成立した扱いにしない。

### 7.2 planner

taskはid、actor/role、release、finish_deadline、predecessors、flow vector、stock期間、carrier/control需要を持つ。全stepに上界がある有限graphを先に確認する。

既存予約を動かさず、全predecessor割当済みのtaskから(deadline, release, original_id, actor_id, task_id)順で選ぶ。最遅releaseと先行完了後、期限内で全flow/stock・carrier/controlに空きがある最初の許可区間へ置く。

時間粒度、horizon、preemption/checkpoint、探索・workspaceは共通profileの有限欄とする。一taskでも割付不能、循環graph、無上界task、planner予算不足なら候補book全体を破棄する。保守的な拒否を許すが、後発案件のため既存の約束を弱めない。

120秒の合計だけでなく、各期限区間と因果順序を確認する。最後の一秒へ全処理が集中する計画は、同区間に入らなければ拒否する。

### 7.3 admission

~~~text
AdmitOriginal =
  common_difficulty_eligible AND canonical_cursor_matches
  AND signature_identity_and_existing_route_eligibility_valid
  AND supported_common_checker AND bounded_resource_descriptor
  AND original_funds_fully_backed AND all_legal_continuations_scheduled
  AND all_flow_and_stock_reservable AND carrier_control_deadlines_feasible
~~~

original、実資金/escrow、全予約、actor義務、履歴起点、仕事一回性、cursor/counterを同じcheckpointで保存する。失敗で部分claim/予約を残さない。bounded入口の実費は別契約で計上する。

quoteはbasis/parent/policy/clock/資金形状に束縛された見積もりで、予約権ではない。同じnative計算をactual prefixで再実行し、古いquote bookをinstallしない。

## 8. 受理後の通知・court実行

適格notice、MaterialRequest、Q holdは全時間帯に保護control経路で受け付ける。30秒枠や次epochまで通知を待たせない。controlも認証・bytes・record/state費用を負担する。

重いcourtは本人のtask・資源・期間を使う。先着三人/五人を選ぶ方式にしない。新規D・新しいPoW・ExecCreditを既受理救済へ要求しない。

~~~text
ExecuteCourtTask =
  original_and_task_live AND actor_role_task_generation_match
  AND canonical_predecessors_complete AND authenticated_material_ready
  AND current_service_interval_permits_execution AND task_not_consumed
  AND packet_case_original_interval_global_costs_fit
  AND original_deadlines_and_funding_valid
~~~

debit、予約消費、case/Q/terminal、once markerを同じcheckpointで更新する。再送で二重実行/支払をしない。

無効署名・他人の予約IDのpacketはingress費を消費するが、他人の保護予約を使わない。適格主体の実行は本人の実行資源を使い、不合格でも実CPU費用を消さない。EvidenceRejected、資料不足/MaterialPending、正規の局所検査で関係成立を確認したLocalPredicateHoldsを区別する。偽異議terminalはnativeの終結条件を満たした当該CaseIdだけに適用し、無効packetや不足資料で既存Q・他case・他actorの救済を消さない。

mempoolに証拠がないだけで将来予約を貸さない。正規terminal、義務が合法に発生しなくなった確定境界、または全残責任を保つ原子的再予約でだけ解放する。

## 9. Node内部の過負荷防止契約

### 9.1 検査順

1. 有界な固定headerを読み、版・length/count/depthをchecked arithmeticで検査する。
2. process全体の入口budgetからparse/認証と最大decode memoryを確保する。
3. 原claim・actor・予約との対応を有界に認証する。
4. 共通cost関数で展開後量、work、I/O、stock、失敗/undo費用を導く。
5. packet/case/original/carrier/区間/globalの全予算を確認する。
6. 取得済みleaseとmeterの範囲でdecode、計算、保存する。

巨大展開・全state cloneの後に上限を見ることを禁止する。preflight/planner、エラー文字列、ログ、圧縮解除、再構成、再試行、undoも有限にする。無上界ループを実行してから実時間を測る方式は採らない。

### 9.2 leaseの所有

buffer、decode object、worker、I/O、clone、journal workspaceへleaseを所有権とともに移す。別objectへ同じleaseを複製しない。cancel/reorg/timeoutの通知だけで返却せず、実データとworker/I/Oが終了してから返す。

bounded arena、checked length/count、固定上界のiterator/深さを使う。meterが事前上界を超えたら未commit処理を停止/rollbackし、checker/profile不整合として新規受付を止める。規格を緩めて通さず、実装不具合をMinerの算術有罪にしない。

### 9.3 ingressとbackpressure

未認証bytes、同時decode、接続別/全体queue、署名/hash、candidate、equivocation、cold read、未完成parts、同期要求をそれぞれ有界にする。新規/未認証流入を先に絞り、既受理actorの認証serviceを保護する。

無効packet五件で正当courtも止まるcount制御にしない。機材不足、cgroup制限、I/O停止では保留/同期/再取得/復旧へ進む。local資源拒否を永久block-invalidやMiner有罪にしない。同期中・未復元Nodeの能力を新規保証供給へ数えない。

## 10. 全carrier合計1 BPSとDAG

### 10.1 共通供給

UnifiedCarrierV1は有限のcontrol、claim/court資料、model work、任意EXEC_TXを運ぶ外側envelopeである。REAL/EXEC/BASE/Heartbeat/court-only/空carrierに別々の毎秒枠を発行しない。内部の全費用を合算する。

permitは(network, policy_generation, global_slot)で一意、RateClock上で1,000msに一本releaseする。使用期間L slot、同時live数L、包含条件・候補保持量は共通capacity/clock profileの較正済み有限欄とする。court件数の例からLを決めない。未使用枠の再発行、連番reset、無制限catch-upは禁止する。

Lはpermit releaseから最終canonical包含までの総寿命であり、生成だけの期限ではない。slot gの使用区間は[origin+1000g, origin+1000(g+L))とし、別の包含猶予Hを追加して消費を遅らせない。履歴再生のローカル時刻ではなく、元のcanonical包含点で期限を検査する。

整数ms区間[t0,t1)、t1>t0の供給/適法消費の保守上限は、

$$
N(t_0,t_1)\le L+\left\lceil\frac{t_1-t_0}{1,000\ {\rm ms}}\right\rceil
$$

である。正常時の持続目標は全kind合計1 BPS。実到着burst、全fork、悪意packet、IBDまでこの数に収まるとは主張せず、別ingress budgetを適用する。空slot・不足credit・停止時に必ず1 BPSになるとはしない。

### 10.2 搬送資格と経済資格

外側は既存の成熟したproducer/BASE bondと搬送同意を資格とし、ModelBlockRight/ExecCreditを必須にしない。内側model work/EXECには既存の各経済資格が別に必要。搬送だけで未認証weightや仕事報酬を作らない。

事前scheduleはfreeze済み搬送資格root、既存beacon、slot、版付きdrawからowner/fallback順位・時間帯を固定する。整数prefix-sum索引と有限回数のrejection samplingを用い、全model走査や無制限抽選をしない。fallback人数/時間帯はprofile必須欄で、L・検証・伝播・包含へ収まる場合だけ有効。

保護court/control slotはservice bookから先に本人/署名承諾relayへ束縛する。published scheduleのownerを後付け変更しない。新originalは既存scheduleに整合する枠、または未公開将来枠だけに予約する。

seed/schedule欠落時も既受理bookの本人/relayと時間帯を維持する。未予約BASE維持slotはactivation recordから継続する成熟BASE資格/同意集合のcanonical順に対し(global_slot+fallback_rank) mod nで担当を固定する。zero-weight BASE/controlだけに使い、claim未知seedの代わりにしない。n=0または全担当停止なら空slotとする。

### 10.3 並列と一回性

異なるlive PermitIdの主体は同parentから独立生成できる。一producerの完了を全modelが待たない。canonical ordered foldで先行消費・予約を反映する。

各siblingの単独検査で残高があることはmerge後の全受理を保証しない。同permit/同資源は一回だけ消費し、未受理carrierからclaim/資金/credit効果を作らない。zero-weight/red候補も無料・無制限保存にしない。

再送は追加消費しない。受理済みcarrierは内部claim拒否・空payloadを理由にpermitを返さない。canonical reorgでだけ元消費をundoし、署名済み内容のjournalを保持する。

### 10.4 検証と包含期限

taskの最遅資料readiness、checkerの検証上界、伝播/包含guardが、taskとcarrierの両期限に収まることを予約時に検査する。120秒合計に入るだけで短いcarrier期限を満たした扱いにしない。

bounded staging・事前検査を使う場合、そのcanonical入力root、費用、保持、prestate、失敗時回収、結果適用点を固定する。cache結果を入力/checker/prestateへ束縛し、他workerの主張でNode検査を代替しない。cold/履歴再生は同じbounded検査を再実行する。

これで包含が成立しないprofile/shapeは受付不能。local検証終了時刻を過去carrierの永久validityにせず、canonical clock/close frontierで期限を判定する。通常伸長で過去prefixへ挿入しない。

## 11. Final・credit・反証優先

local positiveはNormalChecked。独立全体承認、必要機会、Q解消、依存・資金・原期限が揃ってFinalとする。

一claim一block、生成権一件/周期、均等model block配分を追加しない。既存P_m、CCU、モデル機会、有限credit/receiptを適用する。共通Dや予約だけで権利を発行せず、1 BPS維持のためcredit倍率を増やさない。

発行待ち、Available/Reserved、Consumed/Expired/Revokedを起源work/original、policy、元expiryへ束縛する。発行/消費の双方で現在のFinal/Q/依存/反証/認証取消を再確認し、待機や再予約でexpiryを延長しない。未使用起源失効は取消し、既消費unitを再発行しない。

同じcanonical batchは次の順に処理する。

1. bounded構造・署名・費用・prefix検査。
2. 全期間の適格notice/資料と、当該予約期間に実行可能なcourt/terminal。
3. Q hold、反証、認証/依存取消。
4. 更新後stateでFinal判定。
5. 更新後stateでモデル生成権/credit発行・予約・schedule・消費。
6. 費用・残責任・保持・once marker。

後続の重い証拠検査待ちでもQを維持する。body前方EXECが後方反証より先に効果を出す迂回は禁止。起源取消とreorgを分け、過去全取引の独立一括rollbackを追加しない。

## 12. 資金・有限終了・解放

実原資で通常検証、全actor救済、防御、control/carrier、復旧、保持、Abort/精算まで予約する。名目bondを支出可能escrowと混同せず、将来EXEC fee/新claim収入を既存救済の必要原資にしない。

料金版、単価、上限、受取条件、relay費、refund/burn/slashを原署名へ固定する。混雑/難易度変化で追加資金を要求しない。spent+still_locked+refund+authorized_burnの保存と支払一回性を検査する。

Final/Refuted/AbortedUnverifiedは全解放ではない。service、保持、金融責任、journal、archive、GCに独立の有限release条件/once markerを置く。hard到達で全額返金・全資料削除をせず、Abort/返却自体にも容量を予約する。

実サービス不足は成功停止と元契約の有限未検証終了/残責任へ進む。NoCapacity、私的未受信、Nodeの遅さを算術有罪やChallengerDefaultにしない。新規受付を止めて既存義務を隔離する。

## 13. 多数model・保存・復旧

重みを持たなくてもregistry、program記述、claim/case、予約、保持はstockを使う。一万/五万modelでglobal予算を増やさない。

認証永続索引、差分counter、bounded calendar、期限索引、非空queueを使う。毎blockの全model/全claim走査、全state clone、全root再計算を必須にしない。残る全コピー等は最大state規模で計上し、範囲外へ適合を広げない。

registry/保存/GC backlogが満杯なら新規登録/新claimをbackpressureし、既存court予約を削らない。論理失効と物理削除を分け、GC完了前のdiskを空き扱いしない。

snapshotはprofile、時計、D/low counter、preseal集合/seed/cursor、仕事一回性、carrier消費、資金、case、flow/stock、expiry/releaseを認証する。bookを復元できなければ新規受付しない。

reorgは同じjournalで全stateをundo/replayする。old generation worker結果を新世代へ流用せず、local leaseは実終了まで保持する。IBDはactive/staging DB、WAL、spill、復元workspaceを同時に計上し、court予約を奪わない。

## 14. 共通qualificationと未知model自動受付

共通profileはDISABLED、QUALIFIED、RETIRED。QUALIFIEDはchecker/schema/cost-meter/binary/hardware_class/capacity/measurement/policy hashの組に束縛する。model IDを資格台帳の必須キーにしない。

必須検査は以下である。

1. 許可kernel/codecの最大合法shape、数値境界、認証、悪意入力、失敗/undoが有界。
2. 構造上界とmeterが一致し、allocation前に必要量を導出。
3. 最低Nodeでchecker、DB/索引/root/journal、carrier1 BPS、通常取引、preseal/control、登録、cold/reorg/restart/IBDを同時測定。
4. 独立最大値の寄せ集めを使わず、safe margin込みの同時供給点を使用。
5. 最後のnotice、唯一正直者、他actorの最大合法case、元期限、実包含を含む予約が成立。
6. 一万/五万登録と最大pending/retention/GCでstate規模費用を計上。
7. 未見model ID/weights/graphを既存kernelから作り、個別benchmark/allowlistなしで規格内claimを受理。
8. 規格超過、未知kernel、無上界shape、資金不足は高価な処理/allocation前に拒否。
9. 実LLM統合例で正常Final、不正反証、資金精算、生成権消費、carrier受理を確認。全拒否/Abortだけで代替しない。
10. 未完了分母、backlog、stock、deadline miss、空slotを含め持続1 BPSとcourt保護を評価。

checker/schema/meter、共通storage、対応機器/容量の変更は再qualification。同じ版で表せるmodel追加だけなら個別測定は不要。p99だけを最悪合法入力の上界としない。

## 15. 移行と実装完了

型、meter、preflight/lease、planner、D、carrierを未有効の版で実装し、共通profileをqualificationしてwire/domain/fingerprintを新規登録する。全新規routeとquote/Studio/RPCを同じnative受付へ接続する。

旧義務を列挙し、新共通budgetへ元期限内に収容する移行bookを検査する。不能なら新profileを有効化せず旧契約を履行する。既存責任を削る移行は禁止。

指定anchorで新規routeを同時切替し、旧carrierへのdowngradeで新規責任を増やす迂回を閉じる。旧署名/Final/過去credit/既得権を再解釈せず、継続は元契約と移行予約で履行する。

設計採択、文書検査、抽象モデル、native実装、実機性能、公開activationを別記録にする。現時点で実LLM通しや最低Node性能達成とは宣言しない。

## 16. 受入反例

| ID | 状況 | 必須結果 |
| --- | --- | --- |
| V01 | 小/大court混在 | 固定五件でなく実vectorと期限で件数が変わる |
| V02 | CPU内、RAM/bytes/I/O超過 | 超過次元で実行前拒否 |
| V03 | 巨大長さ/展開/overflow | bounded preflightでallocation前拒否 |
| V04 | 空packet/重複/control洪水 | 非ゼロ固定費、queue/metadataで有界 |
| V05 | parts/case/relayへの分割 | 元case/original/区間/globalで合算 |
| V06 | 一万/五万未知model | global予算不増、個別benchmarkなし |
| V07 | 未対応kernel/分解 | Unsupported、品質審査/全推論fallbackなし |
| V08 | 最後の一秒への需要集中 | 合計が入っても期限区間の予約失敗 |
| V09 | 他actorの最大合法caseが遅く開始 | 本人/防御予約を原期限内に保持 |
| V10 | 無効packet先着 | ingress課金、他人の保護予約不消費 |
| V11 | 任意時刻の最後のnotice | control受付・Q保持、固定時間帯待ちなし |
| V12 | quote後に残量変化 | actual prefix再検査、部分予約/資金なし |
| V13 | 巨大planner入力 | planner予算を先に検査 |
| V14 | siblingsの二重予約/permit | canonical一回消費、候補費も有限 |
| V15 | creditゼロ/seed停止 | 既存court/control/Abortは独立搬送 |
| V16 | 同batch反証とFinal/consume | 反証/Q優先 |
| V17 | cancel/reorg後worker生存 | leaseは実終了後に一回返却 |
| V18 | hard後に保持/精算残存 | 未検証終了と残義務を分離 |
| V19 | restart/IBDのbook欠落 | 空bookで新規受付再開不可 |
| V20 | D遅れ/上限/巨大流入 | hard flow/stock/ingressが上限維持 |
| V21 | microbenchmarkだけ高速 | 共通storage/最大state/混合未達なら不合格 |
| V22 | 既存規格で未見model表現 | 個別測定なしでdescriptor/資源だけ判定 |
| V23 | 全拒否/Abortだけ成功 | 実用完了不合格、正常Final/消費が必要 |

## 17. 所有者・根拠・作業記録

| 主題 | 規範所有者 |
| --- | --- |
| 共通形式/分解/完全性 | RFC02/16/17/19 |
| 全体資源/共通qualification | RFC05/19 |
| D/preseal/原子的受付 | RFC07/09/16 |
| 救済/反証/実行/精算 | RFC14/18/20 |
| 全carrier1 BPS/clock/DAG | RFC10 |
| 既存P_m/資金/有限権利 | RFC07/11 |
| 状態/復旧/移行 | RFC12/13 |
| 指示と限定改定 | PRINCIPLES / ADR-0223 |

確認したsourceは[共通W](../../consensus/core/src/palw_work_target_v1.rs)、[Attempt受付](../../consensus/core/src/palw_admission_v2.rs)、[Final後FP receipt](../../consensus/core/src/palw_fp_admission_v3.rs)、[元FP受付](../../consensus/core/src/palw_state_v2/objects.rs)、[court予算](../../consensus/src/pipeline/virtual_processor/processor/palw_accept.rs)、[実装台帳](../design/palw/integration/public-one-honest-implementation-ledger.md)。

ユーザーの2026-11-17添付補足は、未知modelごとの測定を求めず、共通bounded checker、初回trace固定、多段上限、全工程予約を具体化する方針として取り込んだ。説明用の1024項やcourt件数を実測/採択値へ変換していない。

検査・レビュー・変更台帳は[変更記録](../maintenance/revisions/2026-11-17-global-admission-one-bps/README.md)、本番接続は[実装計画](../design/palw/integration/unified-admission-implementation-plan-2026-11-17.md)へ記録する。
