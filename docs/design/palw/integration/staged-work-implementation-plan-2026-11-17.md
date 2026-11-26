# RFC-0058 — 実行束縛・段階commit・合意資格の実装計画

日付: 2026-11-17（Asia/Tokyo）
状態: **PROPOSED_DESIGN_NOT_ACTIVATED**
規範案: [RFC-0058](../../../rfc/0004-fow1-execution-and-ticket.md)
接続先: [RFC-0055](../../../rfc/0014-exec-schedule-and-capacity.md) / [RFC-0056](../../../rfc/0014-exec-schedule-and-capacity.md) / [RFC-0057](../../../rfc/0008-g14-panel-and-verification.md)
監査基準: `main@2d8212b634880b0967fbf24b00b41936c2322c14`
対応する攻撃・受入条件: [THREAT_MATRIX](https://github.com/MISAKA-BTC/misakas/blob/f839ef0dc0307b58ed6cc280e631b1e1d34f733c/docs/maintenance/revisions/2026-11-17-work-binding/THREAT_MATRIX.md)

## 1. 目的と到達範囲

新しいclaim版`SWC1`について、LLM実行、正規の実行commit、ticket、検証者抽選、court、成熟、信用と合意資格を、同じ仕事identityから追跡する。整数算術の完全一致を前提とし、元claim・入力・model/program・実行規則・完全traceのidentityを途中で取り替えられない経路を作る。

本書は実装する作業と受入条件であり、新経路の実装完了・本番稼働・実測達成の記録ではない。既存コードに存在する機構、提案する接続、残る理論上の前提を分ける。RFC-0058の署名schema、型名、phase名、deadlineと本書が異なる場合は規範RFCを優先し、実装前に対応表を更新する。

本変更が目指すのは、安価な可変fieldの変更によるticket再抽選、同じ実行の再包装による仕事の増殖、検査前の成功資格、取消を逃れる先行消費を閉じることである。次の三命題は、この接続だけでは成立しない。

1. 全ての有効な完全traceを作る最安アルゴリズムに、要求する実費以上の下界があること。
2. 大きな資料の虚偽未着と選択的秘匿を、モデル非保有Nodeが有界なnative手順で常に区別・終結できること。
3. 認証遅延・取消・分断・reorgを含むDAGについて、common-prefix、安全な収束、活性が証明されたこと。

これらを未確立のまま「PoW成立証明済み」「資料可用性保証済み」「DAG安全性証明済み」と表示しない。単なる設計採択、文書検査、抽象状態試験もその代わりにならない。

## 2. 維持する境界

- [PRINCIPLES](../../../PRINCIPLES.md)と[凍結手続き](../../../governance/design-freeze.md)を維持する。新RFC・ADR・レビューと明示採択、実装、検証、activationを別々に記録する。
- DNS-BFTは退役した設計であり、乱数・包含・fork choice・settlementの裏付けに復活させない。現行work由来beaconをBFT committeeが運営する既存実装として説明しない。
- 入力資料のBFT beacon案は、authority、membership、fault bound、PQC署名、乱数生成、移行を含む別の例外変更が採択されない限り**無効な候補**である。RFC-0058の段階commitを実装することから、その委員会の採択を導かない。BFTによる取引確定やcheckpoint権限も追加しない。
- PQ-onlyの方針を維持する。既存の承認された署名・hash・encodingに対応するadapterを使い、古典的な署名や別の閾値署名をbeaconの便宜だけで追加しない。具体的なschemeが未割当ならそのadapterは有効化しない。
- modelの無審査登録、1.0／0.3、年次指定、全modelへのclaim検証を維持する。新modelの実費benchmark、公開性審査、保有者数審査を登録資格やtier昇格にしない。
- Nodeはmodel重みや完全traceを保持せず、全体推論をしない。Panel／Minerの実行・資料保持と、Nodeの有界なopening・局所演算検査を分ける。
- 計算正当性の暗号学的証明を導入しない。hash／署名／Merkle opening／beaconは同一性・認証・乱数の用途に限る。
- 新たな報酬率、slash額、担保率、強制委員会人数を割り当てない。既存の署名済みscope、実資金、原資、責任窓に接続する。
- RFC-0055の全体容量とRFC-0056の全model共通受付D、RFC-0057の主体別義務nonceを維持する。30秒・120秒の例やcourt 5件を新たな固定容量規則にしない。DAAと壁時計秒を同一視しない。

## 3. 既存仕様と追加作業の対応

| 領域 | 既存の契約・実装上の足場 | RFC-0058で必要な追加接続 | この作業から導けない結論 |
| --- | --- | --- | --- |
| 初回実行commit | [RFC-v2-0016](../../../rfc/0004-fow1-execution-and-ticket.md)の元実行・完全trace・依存の束縛 | 段階commit、ComputationalWorkKey、ticket、後続Panel抽選の参照を一つにする | 実行の最小実費が証明された |
| 全claim受付 | 0055の全工程予約、0056のTrialSlot／Preseal／公表済みDと有限prefix | 新しいstageごとの先行費用・保持・再開・失敗終了を同じ予算へ入れる | ticket不合格packetの無限到着をネットワーク層で無費用化できる |
| Panelとcourt | [RFC-v2-0017](../../../rfc/0008-g14-panel-and-verification.md)の完全承認、0057のnonce／scope | 最終実行commit後の独立した抽選contextと、元rootへの肯定・反証を接続 | 全Panelが独立に再計算したと物理的に証明できる |
| 資料と検査機会 | [RFC-v2-0018](../../../rfc/0009-court-and-claim-final.md) §2.3–2.4、[RFC-0050](../../../rfc/0008-g14-panel-and-verification.md) | 元scopeの取得・検査・包含・裁定を元時計で完了できる具体的なnative手順 | URL、ACK、別人の受領証だけで当人の全検査機会が成立する |
| Finalと成功資格 | [RFC-v2-0020](../../../rfc/0009-court-and-claim-final.md) §5.2・§11A、0048 | 新仕事版のhold／反証／成熟を全credit・fork-choice consumerへ渡す | 旧wire Finalが新たな不可逆確定になる |
| Native settlement | `palw_native_settlement_v1.rs`の成熟変換、依存責任、canonical snapshot | 新WorkKeyと新責任時計を既存成熟変換・fact出力へ正しく写像する | 過去fork-choiceへの一時的不正weightの影響が証明済みになる |
| 取消とpruning | `palw_security_certification_v1.rs`のfrontier再計算とrollback bound | 新版の資格取消、歴史cut、保持、reorgとfloorの対応を閉じる | frontierの再計算だけで分断後の安全な収束が保証される |

参照名が似た旧型に、新版の意味を暗黙に載せない。足場の存在は新経路のruntime接続を意味しない。

## 4. P0 — 版・identity・安全性の主張を固定する

実装前に、新profileの`chain_id`、policy/checker/ruleset、model/program/input/dependency roots、実行長とshape、元主体と実資本、stage ID、`WorkOpportunity`、`ComputationalWorkKey`、`OriginalId`、epoch/cut、元expiryの対応を表にする。署名、carrier、nonce、再送先、timestamp、選択可能なpaddingから、新しい仕事を派生させない。

`WorkOpportunity`は経済的なTrialSlotと事前固定したowner/funding/beneficiaryを束縛する概念であり、RFC-0058の`JobRegistration`、`RegistrationId`、`OpportunityId`へ対応する。別の自由なwire IDを追加する意味ではない。`ComputationalWorkKey`は実model/program/content/rulesと実入力・初期状態・範囲を束縛し、外装を除外する。`OriginalId`はその仕事に対して提出した元実行rootを含む責任identityであり、Court nonceは選出主体・役割・round・scopeの義務identityである。本書で短くWorkKey／Originalと呼ぶ場合もこの区別を維持する。

同一仕事を複数の合法なcarrierへ運ぶことと、複数の仕事・信用・報酬を作ることを区別する。既存hash幅、canonical encoding、署名domain、integer overflow、vector順序、空集合、重複の扱いを明記する。

途中root、最終root、出力、局所openingのどれが何をcommitするかを固定する。単なる出力rootは完全trace rootと同じ意味にならない。実行が決定論的でも、証拠の圧縮・木形状・leaf順序・未使用fieldが任意ならroot候補を増やせるため、traceの正規形と検証方法が必要である。

**受入条件:** 独立実装が同じ入力から同一の規範bytes／仕事identityを得る。意味不変の再署名・再包装・別relayは同じ仕事となる。model/input/実行規則の意味が変わる場合は正規の新jobかを確認し、既存slotへは差し替えられない。出力／trace rootだけの変更はWorkKeyを作り直す理由にならず、既に固定したOriginalへのConflictとして扱う。hash衝突耐性の仮定と正規形一意性の規則を分ける。

## 5. P1 — 段階commitとticketの依存関係を実装する

順序は、`WorkOpportunity`と初期scopeの事前固定、`R_job`確定、実jobの導出と完全実行、`OriginalId`と元rootの固定、`R_ticket`によるticket判定、admission cohort固定、`R_panel`による検証者抽選とする。ここでいう段階は、この因果順序である。推論途中のsegmentごとに追加乱数を挿入する別方式や、任意の追加stageを本計画から導入しない。

三つの乱数contextはそれぞれの固定cutとdomainに結び付ける。`R_job`は実jobの入力・初期状態・規定された実行へ影響させ、hash用metadataにだけ書いて同じ推論を使い回す構成にしない。その導出はprofileの仕事意味と利用者入力の扱いへ明示的に接続する。`R_ticket`を知ってからrootを選べず、`R_panel`を知ってからcohortや元実行を選べない順序をnativeが検査する。

提出者が選ぶ最新tip、署名bytes、任意nonce、古いseed、複数beacon応答の良いものだけ、stageを飛ばしたshortcutを許可しない。新しい乱数が待てない場合に、ゼロ値・local RNG・任意block hashで埋めない。

ticketを得るためのハッシュ入力に、規範の最終実行commitとWorkKeyを含めるだけでなく、そのcommitが元実行の正規形であることを後続の検査責任に結び付ける。攻撃者は任意rootを安価にhashできるため、任意rootから得た当選が後から有効な仕事へ変換できない条件を示す必要がある。「hash入力にrootがある」だけを全実行の費用下界と扱わない。

0056の受付DはNodeが負う新規責任を減らすために使う。SWC1では0058のWorkTicketを一度評価してこのDへ接続し、別々の自由抽選や二重のDを追加しない。受付適格と、検証・成熟後のLLM仕事計数を別の状態にする。安価な前段header検査の通過、落選、偽rootの当選をLLM仕事として数えない。0056/0058の依存順序に合わせて全入力と状態効果をtraceする。

**受入条件:** `R_job`のmetadataだけへの使用、同じ経済TrialSlotからのowner/funding/beneficiary付替え、stage省略、commit後出し、`R_ticket`後の別root選別、cohort固定前のPanel集合既知、任意root当選後の差替えをnative状態から拒否できる。少数段階の抽象モデルで先読み経路がないことと、実LLMの完全trace構築費用を区別して報告する。

## 6. P2 — 乱数・試行回数・期限とbootstrapを接続する

使用するadapterは、認証されたcontext、branch、元commit、pool snapshot、policy、到達期限を検査する。現行work由来beaconの偏り・公開拒否・reorg・自己依存リスクを残したまま「完全に操作不能」と呼ばない。

同じ資本の別鍵、同modelの別登録、複数claim候補、workの包装変更、取消・再提出、seedを見た後の提出放棄を、試行台帳と累積予算のどこで数えるか固定する。単一drawの`f^m`を全運用期間の失敗確率にしない。有限の機会数を使う場合、その上限が実際のadmission・資本・epoch・再試行規則から導かれることを確認する。

public one-honestの仮定は、選出global poolに対する前提である。BFT committeeの人数・fault bound、stake比率、鍵の数を別のPanel poolへ移植しない。復元抽出の位置数とdistinctなnative主体数を分け、同主体の重複選出を独立した正直者として数えない。

最初のclaim、低供給、全新規受付停止、seed停止からの進行を明記する。認証anchor・資料・異議・court・schedule commitを、当該claim自身の未発行creditや未認証weightからのみ供給する循環は禁止する。既受理責任のserviceは0055で先に確保する。原時計内に進行不能なら既存の未検証終了を使い、期限更新と無償再抽選を作らない。

**受入条件:** bootstrapから正しいclaimが完了する実経路、seed停止中でも既存caseが進む経路、有限の失敗終了、再開後の一回性を示す。乱数の品質、honest pool前提、包含能力をnative unit testのPASSから推定しない。

## 7. P3 — 0055/0056の容量へ全stageを含める

原受理前に、正常系だけでなく、最大合法のstage commit、challenge応答、失敗候補、選出者、資料要求、localization、直接反証、終端、返還、保存とGCを資源計画に入れる。CPU、署名/hash回数、decode、evidence bytes、heap peak、I/O、保持stock、index更新、包含機会を分ける。

新規claimは全model共通のDとcalendarを通る。モデル別の並列候補は許しても、model数・Panel数・nonce数に比例してネットワーク全体の予算を増やさない。適格でもNoCapacityなら新責任を作らない。元の反証・資料要求・応答・裁定に新Dや新しい採掘当選を要求しない。

1 BPSは外側carrier全体の目標として扱う。claim Finalが1秒ごとに出ること、1秒ごとに不可逆settlementが得られること、120秒cycleに必ず一つしか成功できないことを意味しない。複数modelが条件を満たせば共通容量内で並行に進めるが、全体のservice予算を守る。

**受入条件:** 登録1万・5万modelのcold miss／cache hit、最大pending、quote、invalid packet、正規新規候補、継続case、通常取引、restart／IBD／GCを同時に測る。最低Nodeの仕様と使用限度を実測から採択profileに結び付ける。登録件数だけからRAM/GPUの必要量を断定せず、Nodeにmodel全量を置くfallbackを追加しない。

## 8. P4 — 元rootへの完全承認と単独反証を接続する

Panel選出の`R_panel`は、元の最終実行commitとadmission cohortを固定した後に確定する。署名はWorkKey、Original、元Miner execution root、model/program/input/規則、選出主体、round、本人nonce、scopeへ束縛する。正しい局所証拠を別のtraceから持ってきても、元rootへの完全承認の根拠にはならない。

PublicOneHonestの最低完全承認数1と、全選出者の完全再実行責務を維持する。残り全員の不署名だけで正しいclaimを停止させず、nonceごとの局所challenge完了を全員分要求する別のFinal条件にも変えない。承認者の完全再実行は責務・署名scopeとして扱い、nonceや局所openingから物理的な独立労働を証明したとは扱わない。

一人の適格な検証者が、完全資料を取得して元実行全体を検査し、元scope内の有効な反証を期限内に包含・裁定できれば、全positiveに優先して元claimを反証する。Panel個別challengeに選ばれなかった箇所の反証も受理する。正しい局所演算、誤ったopening、根の単純不一致、資料不足、計算不整合の裁定を別の結果にする。

誤ったFullReplay承認の責任は、当該署名と反証対象scope、原責任窓、徴収可能な実資金に基づく既存規則へ接続する。LocalHoldsだけの署名者、非署名者、relayerを同じ違反者にしない。同一offenceを別nonce・別証拠・別申立人で二重計上しない。

**受入条件:** 不正Miner、正しい局所資料をコピーしてFullReplayを署名する悪意Panel、元実行を検査する正直Panel一人を同じ実行記録で再現する。有効反証で新しい成功資格が消え、scopeに一致する責任だけが一回適用される。正しいclaimと非署名多数の成功系も必須とする。

## 9. P5 — 資料取得と検査機会の手順を成立させる

`Published`、`CustodyAuthenticated`、`ChunkAuthenticated`と、0018 §2.4の論理状態`InspectionPrerequisitesEstablished`を区別する。後者は特定の申立人に対する、元scopeの全資料・認証・残検査時間・提出包含機会の成立を必要とする。これは既存wire boolの名称ではない。

資料にはモデル静的資料だけでなく、元入力、token列、program/ruleset、依存状態、完全trace、必要なopeningの供給経路を含める。0053による追加modelの停止規則を、廃止された取得審査や保持者数審査の復活へ変えない。claim固有の必要資料と検査機会の欠如は、そのclaimの成功停止として扱う。

初回のMaterialRequestは、完全modelを持つことや算術矛盾を先に証明することを条件にしない。元scope、CaseId、義務者、要求範囲、全工程の期限と費用を固定する。資料が届いた後の完全検査・局所化・反証は、同じCaseIdの合法な継続であり、notice窓後の新規異議へ追い出さない。

一部chunkの回答はその部分の履行だけを記録する。検査前提が成立しないまま、quota消尽、別人のACK、BFT署名、DAA経過からchallenger defaultを導かない。前提成立後の客観的な放棄と、供給義務違反と、service不足を区別する。原期限で回復できなければ、成功不可の未検証Abortへ進み、通信失敗を計算有罪に変換しない。

**受入条件:** 正常供給、選択的秘匿、部分供給、遅延供給、虚偽未着、偽ACK、最後の合法要求、資料取得後の不一致を同じnative手順で有限に処理する。現行0018／0050の`PROFILE PROCEDURE NOT ESTABLISHED`を解除するには、Nodeの有限資源内でその手順が成立する証拠が必要である。未確立なら当該profileは有効化不可であり、経済罰や最小承認数を代替にしない。

## 10. P6 — Final・成熟・credit・合意consumerを同じ資格へ接続する

旧wireのFinalを改名しない。新profileの論理段階として、実行commit、ticket適格、検証中、検証成立・成熟待ち、用途別の成熟適格`MatureEligible`を区別する。失敗はRefuted／AbortedUnverified、既存Final後の合法なcaseはscope付きhold／Revokedとして、元の権利と時計へ結合する。

用途別のeligibility recordは少なくとも、policy、source WorkKey／Original、canonical state cut、依存・caseの根拠、成熟境界、元expiry、取消世代をnative stateから導く。提出者の自己申告bool、署名数、局所passをその代わりにしない。履歴snapshotで適格だったことだけで現在の消費を許さず、現在stateも再検査する。

対象consumerはcredit mint/reserve/consumeだけでなく、safe frontier、safe/live weight、settlement depth、Rule E participation、settled anchor、issuance、関連native receipt、scheduleとpermitを網羅する。未認証claimが一つのconsumerだけを通る抜け道を残さない。fee-only役務支払いを成功仕事報酬やconsensus workに加算しない。

同batchでは、合法なnotice・反証・Q・依存取消を先に処理してからFinal、成熟資格、mint、schedule、consume、feeと取引効果をcommitする。候補の署名時刻、provisional実行、body内の先頭配置でこの順序を変えない。batchより前のconsumedはprovenanceを保持し、元claim取消だけで新たな一括取引rollbackを作らない。通常のcanonical reorgによるundo/replayは別に維持する。

**受入条件:** 全consumerの読取点一覧を作り、同batchの順列、旧版／新版混在、Qの再開・正規解決、source取消、依存取消、permit未使用／既消費をnative試験する。古いcache、index欠損、worker世代違いから適格を推定しない。

## 11. P7 — 成熟・責任窓・pruning・reorgの境界を閉じる

既存native settlementには、trace retention、Final後のretirement、slice quantum maturity、依存責任を合成した成熟判定がある。新しい段階commitやFinal後の訴追権を、この変換へ明示的に接続する。0018/0020の新しいFinal後手順が現legacy courtに接続済みとは仮定しない。

notice、資料取得、検査、次手、裁定、責任、保持、資本解放、成熟、pruningの全境界を、inclusive／exclusiveと同DAA値の複数処理単位を含めて定義する。`liability_until`を含む期限なら、成功用途に応じて翌tickまで待つ等、既存のchecked変換を保つ。overflowや未割当の期限を成熟済みとして扱わない。

退役floorより後退しない規則と、いつまでどの証拠でworkを取り消せるかを整合させる。「無期限に取り消せる」「過去のsettlementは無条件に不可逆」を同時に主張しない。成熟後の新証拠、歴史上の誤認証、保管された責任、既受理取引の扱いは版付きpolicyに従い、旧claimの権利を短縮しない。

reorgでは、仕事identity、seed／draw、case、全効果once、残予約、credit／permit、consumer状態、eligibility snapshotを同じcanonical履歴で復元する。取消とreorgを混同せず、別branchの署名・seed・評決・消費を無条件に流用しない。pruning／IBDは残る取消可能性と責任を保存する。

**受入条件:** maturity直前・同値・直後の反証／資料要求、同DAA別batch、長い依存、late conviction、floor更新、欠損archive、restart、深いreorgを確認する。局所stateが一致した結果と、DAG全体のcommon-prefix証明を別の成果物にする。

## 12. P8 — 実費・非償却性とDAG理論を独立に評価する

実費評価では、完全に正しい結果と正規traceを構築できる最も安価な攻撃者を対象にする。cache、共通prefix、batch、重複job、低rank・零重み等の特殊なモデル、事前計算、別アルゴリズム、trace再構成、外注／共有を含める。正しい安価な計算を、局所反証で計算不正にできるとは仮定しない。

規範演算量と、CPU/GPU時間、memory bandwidth、保存・通信・証拠作成費用を分ける。対象profile、input/job分布、複数claimに対する償却範囲、許す前計算、乱数の前提、攻撃者の資源を示し、失敗当選試行も含めた期待費用を測る。1modelの測定を全登録modelの下界としない。モデル別の事前資格審査でこの未解決問題を隠さない。

DAGについては、認証遅延、確定前hold、誤認証、遅延反証、weight取消、分断、選択的公開、seedのbranch依存、bounded immature成分、retired floorとpruningを含むfork-choiceを固定する。定理のhonest-work／honest-Panel／ネットワーク同期／包含条件を分ける。beaconの性質だけからfork-choiceの安全性を導かない。

**受入条件:** 数学的主張の前提・対象・結論と、反例探索・実測結果を分離して提出する。段階commitの因果順序試験は全LLM計算の費用下界にならない。正常時devnet、有限状態モデルのPASS、単一partition試験はDAG定理の代わりにならない。未達項目は未達のままactivation判定へ渡す。

## 13. P9 — Studioからの同一claimによるE2Eと運用報告

同じ実LLM結果を、Studio等の実送信、nativeな試行権、各段階commit、beacon、最終ticket、0056の受付、0055の予約、Panel完全再実行、0057の本人nonce、Nodeの局所検査、成功Final、成熟、信用発行、schedule、permitの一回消費まで追う。途中でfixture claimや異なるOriginalへ差し替えない。

成功例、不正元trace、資料不供給、正直者一人、無応答多数、最後の合法異議、高D、seed停止、再起動、reorgを、同じpolicyとbinaryで報告する。資金・book・case・eligibility・消費の永続stateを確認する。実UIの送信成功だけ、digest単体一致、抽象モデルのみをE2E完了にしない。

**報告必須項目:**

- source/binary hash、policy/checker、clock/beacon adapter、capacityと料金、hardware、元WorkKey／Original。
- 各stageのcommit cutと乱数確定cut、試行数、放棄数、同一仕事の重複拒否、元期限。
- 全model共通の受付D、全候補分母、NoCapacity、invalid、未処理尾部、予約と実費・stock peak。
- cold Panelの資料取得・完全検査時間と、Nodeの局所裁定時間、包含・裁定待ち時間。
- 成功／反証／未検証Abort、返還・役務費・成功報酬、credit mint/reserve/consume、取消とreorgの各結果。
- 1 BPS測定の空slot、backlog、credit在庫、期限超過、未完了claim、通常取引とcourtの混合負荷。
- 残る前提・未実装・未測定・未証明と、公開activationを止める具体的なgate。

## 14. 完了と有効化の判定

文書の完了は、scope、版、依存順序、有限資源、state遷移、失敗終了、証拠、移行をレビュー可能に記述した状態である。native接続完了、実機容量の検証、全体安全性の成立、公開有効化とは別である。

新規受付を止めるrollbackは可能にし、既受理新版claimの予約・元時計・訴追権・返還をその版で完了させる。未割当scheme、未接続consumer、資料手順未確立、PoW費用下界未成立、DAG安全性未成立を、1 BPSや大量modelのデモで打ち消さない。新しいBFT authorityを必要とする構成は、その例外変更が別途採択されるまで無効とする。

## 15. 固定コードの監査参照

下記は2026-11-17に読んだ固定commitの根拠であり、新版runtimeの完成記録ではない。

- [palw_native_settlement_v1.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_native_settlement_v1.rs): canonical snapshot、依存責任の翌tick、native39 maturity、完全責任終了前の成熟work除外、settlement factのfilter。
- [palw_security_certification_v1.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_security_certification_v1.rs): 旧版のFinal後認証、pending accusation、frontier再計算、retired floor、pruning rollback bound。新版のFinal前認証と同義ではない。
- [0056実装計画](claim-admission-difficulty-implementation-plan-2026-11-17.md): compact入口、全route、TrialSlot／Preseal、原子的original install、観測とD、復旧・負荷試験。
- [0057実装計画](court-nonce-implementation-plan-2026-11-17.md): 元root、主体別scope、once、共有資料、単独反証、実資金、nonceの限界。
